pub mod cloudflare;
pub mod client;

use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use anyhow::{Context, Result};
use x509_parser::prelude::*;

use crate::prism::config::AcmeConfig;
use self::cloudflare::CloudflareClient;
use self::client::AcmeClient;

#[derive(Debug, Clone)]
pub struct AcmeCertPaths {
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CertValidity {
    Valid { days_left: i64 },
    NeedsIssuance,
    NeedsRenewal { days_left: i64 },
    MissingDomains { missing: Vec<String> },
}

pub fn check_cert_validity(
    cert_path: &Path,
    requested_domains: &[String],
    renew_before_days: u32,
) -> CertValidity {
    if !cert_path.exists() {
        return CertValidity::NeedsIssuance;
    }

    let data = match std::fs::read(cert_path) {
        Ok(d) => d,
        Err(_) => return CertValidity::NeedsIssuance,
    };

    let mut cursor = std::io::Cursor::new(&data);
    let certs: Vec<rustls::pki_types::CertificateDer<'static>> =
        match rustls_pemfile::certs(&mut cursor).collect::<Result<Vec<_>, _>>() {
            Ok(c) => c,
            Err(_) => return CertValidity::NeedsIssuance,
        };

    let leaf = match certs.first() {
        Some(l) => l,
        None => return CertValidity::NeedsIssuance,
    };

    let (_, parsed) = match parse_x509_certificate(leaf.as_ref()) {
        Ok(res) => res,
        Err(_) => return CertValidity::NeedsIssuance,
    };

    let not_after = parsed.validity().not_after.timestamp();
    let now = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(_) => 0,
    };

    let seconds_remaining = not_after - now;
    let renew_threshold = (renew_before_days as i64) * 86400;

    if seconds_remaining < renew_threshold {
        let days_left = (seconds_remaining / 86400).max(0);
        return CertValidity::NeedsRenewal { days_left };
    }

    // Check SAN coverage
    let mut san_names = Vec::new();
    for ext in parsed.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                if let GeneralName::DNSName(dns) = name {
                    san_names.push(dns.to_string());
                }
            }
        }
    }

    // Fallback: check CN if no SAN
    if san_names.is_empty() {
        for rdn in parsed.subject().iter() {
            for attr in rdn.iter() {
                if attr.attr_type() == &oid_registry::OID_X509_COMMON_NAME {
                    if let Ok(cn) = attr.as_str() {
                        san_names.push(cn.to_string());
                    }
                }
            }
        }
    }

    let mut missing = Vec::new();
    for req in requested_domains {
        let covered = san_names.iter().any(|san| domain_covered(san, req));
        if !covered {
            missing.push(req.clone());
        }
    }

    if !missing.is_empty() {
        return CertValidity::MissingDomains { missing };
    }

    let days_left = seconds_remaining / 86400;
    CertValidity::Valid { days_left }
}

fn domain_covered(pattern: &str, domain: &str) -> bool {
    let p = pattern.trim().to_ascii_lowercase();
    let d = domain.trim().to_ascii_lowercase();
    if p == d {
        return true;
    }
    if let Some(suffix) = p.strip_prefix("*.") {
        if let Some((_, domain_suffix)) = d.split_once('.') {
            return domain_suffix == suffix;
        }
    }
    false
}

#[derive(Debug, Clone)]
pub struct AcmeManager {
    config: AcmeConfig,
    workdir: PathBuf,
}

impl AcmeManager {
    pub fn new(config: AcmeConfig, workdir: &Path) -> Self {
        Self {
            config,
            workdir: workdir.to_path_buf(),
        }
    }

    pub fn resolve_paths(&self) -> (PathBuf, PathBuf, PathBuf) {
        let base_dir = if !self.config.storage_dir.trim().is_empty() {
            PathBuf::from(self.config.storage_dir.trim())
        } else {
            self.workdir.join("acme")
        };

        let env_slug = client::directory_url_slug(&self.config.directory_url);
        let storage_dir = base_dir.join(env_slug);

        let cert_file = if !self.config.cert_file.trim().is_empty() {
            PathBuf::from(self.config.cert_file.trim())
        } else {
            storage_dir.join("cert.pem")
        };

        let key_file = if !self.config.key_file.trim().is_empty() {
            PathBuf::from(self.config.key_file.trim())
        } else {
            storage_dir.join("key.pem")
        };

        (storage_dir, cert_file, key_file)
    }

    /// Ensure that a valid ACME certificate exists on disk, issuing or renewing
    /// it via Cloudflare DNS-01 if necessary.
    pub async fn ensure_certificate(&self) -> Result<AcmeCertPaths> {
        let (storage_dir, cert_file, key_file) = self.resolve_paths();

        let validity = check_cert_validity(
            &cert_file,
            &self.config.domains,
            self.config.renew_before_days,
        );

        match validity {
            CertValidity::Valid { days_left } => {
                tracing::info!(
                    cert_file = %cert_file.display(),
                    days_left = days_left,
                    "ACME: existing certificate is valid; skipping issuance"
                );
                return Ok(AcmeCertPaths {
                    cert_file: cert_file.to_string_lossy().to_string(),
                    key_file: key_file.to_string_lossy().to_string(),
                });
            }
            CertValidity::NeedsRenewal { days_left } => {
                tracing::info!(
                    cert_file = %cert_file.display(),
                    days_left = days_left,
                    "ACME: certificate expires soon; renewing via Cloudflare DNS-01"
                );
            }
            CertValidity::NeedsIssuance => {
                tracing::info!(
                    cert_file = %cert_file.display(),
                    "ACME: certificate not found; requesting via Cloudflare DNS-01"
                );
            }
            CertValidity::MissingDomains { missing } => {
                tracing::info!(
                    missing = ?missing,
                    "ACME: existing certificate missing requested domains; requesting new certificate"
                );
            }
        }

        let cf_client = CloudflareClient::new(
            self.config.cloudflare.api_token.clone(),
            if self.config.cloudflare.zone_id.trim().is_empty() {
                None
            } else {
                Some(self.config.cloudflare.zone_id.trim().to_string())
            },
        );

        let acme_client = AcmeClient::new(
            cf_client,
            self.config.directory_url.clone(),
            self.config.email.clone(),
            self.config.domains.clone(),
            Duration::from_secs(self.config.cloudflare.propagation_timeout_secs),
        );

        let (cert_pem, key_pem) = acme_client.issue_certificate(&storage_dir).await?;

        if let Some(parent) = cert_file.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create parent dir {}", parent.display()))?;
        }

        std::fs::write(&cert_file, &cert_pem)
            .with_context(|| format!("write certificate to {}", cert_file.display()))?;

        std::fs::write(&key_file, &key_pem)
            .with_context(|| format!("write private key to {}", key_file.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600));
        }

        tracing::info!(
            cert_file = %cert_file.display(),
            key_file = %key_file.display(),
            "ACME: saved new certificate and private key to disk"
        );

        Ok(AcmeCertPaths {
            cert_file: cert_file.to_string_lossy().to_string(),
            key_file: key_file.to_string_lossy().to_string(),
        })
    }

    /// Sync SVCB / HTTPS records to Cloudflare DNS for all configured domains.
    pub async fn sync_svcb_records(
        &self,
        endpoints: &[crate::prism::acme::cloudflare::PublishedEndpoint],
    ) -> Result<()> {
        if self.config.cloudflare.api_token.trim().is_empty() || endpoints.is_empty() {
            return Ok(());
        }

        let cf_client = CloudflareClient::new(
            self.config.cloudflare.api_token.clone(),
            if self.config.cloudflare.zone_id.trim().is_empty() {
                None
            } else {
                Some(self.config.cloudflare.zone_id.trim().to_string())
            },
        );

        for domain in &self.config.domains {
            match cf_client.resolve_zone_id(domain).await {
                Ok(zone_id) => {
                    if let Err(err) = cf_client.sync_endpoint_svcb_records(&zone_id, domain, endpoints).await {
                        tracing::warn!(domain = %domain, err = %err, "ACME: failed to sync SVCB records to Cloudflare");
                    } else {
                        tracing::info!(domain = %domain, "ACME: synchronized SVCB records to Cloudflare");
                    }
                }
                Err(err) => {
                    tracing::warn!(domain = %domain, err = %err, "ACME: failed to resolve zone ID for SVCB sync");
                }
            }
        }

        Ok(())
    }

    /// Background renewal loop checking certificate expiry daily.
    pub async fn run_renewal_loop(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) {
        if !self.config.auto_renew {
            tracing::debug!("ACME: auto-renewal is disabled");
            return;
        }

        let check_interval = Duration::from_secs(12 * 3600); // Check twice daily
        tracing::info!(
            interval_hours = 12,
            "ACME: started background certificate auto-renewal task"
        );

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        tracing::debug!("ACME: stopping auto-renewal task");
                        break;
                    }
                }
                _ = tokio::time::sleep(check_interval) => {
                    tracing::debug!("ACME: running periodic certificate renewal check");
                    let (storage_dir, cert_file, key_file) = self.resolve_paths();
                    let validity = check_cert_validity(
                        &cert_file,
                        &self.config.domains,
                        self.config.renew_before_days,
                    );

                    match validity {
                        CertValidity::Valid { days_left } => {
                            tracing::debug!(days_left = days_left, "ACME: certificate still valid");
                        }
                        _ => {
                            tracing::info!("ACME: renewing certificate in background");
                            let cf_client = CloudflareClient::new(
                                self.config.cloudflare.api_token.clone(),
                                if self.config.cloudflare.zone_id.trim().is_empty() {
                                    None
                                } else {
                                    Some(self.config.cloudflare.zone_id.trim().to_string())
                                },
                            );
                            let acme_client = AcmeClient::new(
                                cf_client,
                                self.config.directory_url.clone(),
                                self.config.email.clone(),
                                self.config.domains.clone(),
                                Duration::from_secs(self.config.cloudflare.propagation_timeout_secs),
                            );

                            match acme_client.issue_certificate(&storage_dir).await {
                                Ok((cert_pem, key_pem)) => {
                                    let _ = std::fs::write(&cert_file, &cert_pem);
                                    let _ = std::fs::write(&key_file, &key_pem);
                                    #[cfg(unix)]
                                    {
                                        use std::os::unix::fs::PermissionsExt;
                                        let _ = std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600));
                                    }
                                    tracing::info!("ACME: successfully renewed certificate in background");
                                }
                                Err(err) => {
                                    tracing::error!(err = %err, "ACME: failed to renew certificate in background");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_covered() {
        assert!(domain_covered("example.com", "example.com"));
        assert!(domain_covered("example.com", "EXAMPLE.COM"));
        assert!(domain_covered("*.example.com", "foo.example.com"));
        assert!(domain_covered("*.example.com", "bar.example.com"));
        assert!(domain_covered("*.example.com", "*.example.com"));
        assert!(!domain_covered("*.example.com", "a.b.example.com"));
        assert!(!domain_covered("*.example.com", "example.com"));
        assert!(!domain_covered("foo.com", "bar.com"));
    }

    #[test]
    fn test_check_cert_validity_flow() {
        let temp_dir = std::env::temp_dir().join(format!("prism_acme_test_{}", rand::random::<u64>()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let cert_path = temp_dir.join("cert.pem");

        // 1. Non-existent file
        let val = check_cert_validity(&cert_path, &["test.example.com".into()], 30);
        assert_eq!(val, CertValidity::NeedsIssuance);

        // 2. Generate certificate valid for ~365 days
        let rcgen::CertifiedKey { cert, .. } =
            rcgen::generate_simple_self_signed(["test.example.com".to_string()]).unwrap();
        std::fs::write(&cert_path, cert.pem()).unwrap();

        // Matching domain & valid days
        let val = check_cert_validity(&cert_path, &["test.example.com".into()], 30);
        assert!(matches!(val, CertValidity::Valid { days_left } if days_left > 100));

        // Missing domain
        let val = check_cert_validity(&cert_path, &["other.example.com".into()], 30);
        assert!(matches!(val, CertValidity::MissingDomains { missing } if missing == vec!["other.example.com"]));

        // Renew before days very large (exceeding cert lifetime ~755,801 days for rcgen self-signed)
        let val = check_cert_validity(&cert_path, &["test.example.com".into()], 800_000);
        assert!(matches!(val, CertValidity::NeedsRenewal { .. }));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_directory_url_slug() {
        assert_eq!(client::directory_url_slug(""), "production");
        assert_eq!(client::directory_url_slug("production"), "production");
        assert_eq!(client::directory_url_slug("PRODUCTION"), "production");
        assert_eq!(client::directory_url_slug("letsencrypt"), "production");
        assert_eq!(
            client::directory_url_slug("https://acme-v02.api.letsencrypt.org/directory"),
            "production"
        );

        assert_eq!(client::directory_url_slug("staging"), "staging");
        assert_eq!(client::directory_url_slug("STAGING"), "staging");
        assert_eq!(client::directory_url_slug("letsencrypt-staging"), "staging");
        assert_eq!(
            client::directory_url_slug("https://acme-staging-v02.api.letsencrypt.org/directory"),
            "staging"
        );

        assert_eq!(client::directory_url_slug("zerossl"), "zerossl");
        assert_eq!(
            client::directory_url_slug("https://acme.zerossl.com/v2/DV90"),
            "zerossl"
        );

        let custom_slug = client::directory_url_slug("https://ca.internal.local/acme/directory");
        assert!(custom_slug.starts_with("custom_"));
    }

    #[test]
    fn test_resolve_paths_isolation() {
        let temp_workdir = std::env::temp_dir().join(format!("prism_test_workdir_{}", rand::random::<u64>()));

        // 1. Default storage_dir with staging
        let mut cfg_staging = AcmeConfig::default();
        cfg_staging.directory_url = "staging".into();
        let mgr_staging = AcmeManager::new(cfg_staging, &temp_workdir);
        let (storage_staging, cert_staging, key_staging) = mgr_staging.resolve_paths();

        assert_eq!(storage_staging, temp_workdir.join("acme").join("staging"));
        assert_eq!(cert_staging, temp_workdir.join("acme").join("staging").join("cert.pem"));
        assert_eq!(key_staging, temp_workdir.join("acme").join("staging").join("key.pem"));

        // 2. Default storage_dir with production
        let mut cfg_prod = AcmeConfig::default();
        cfg_prod.directory_url = "production".into();
        let mgr_prod = AcmeManager::new(cfg_prod, &temp_workdir);
        let (storage_prod, cert_prod, key_prod) = mgr_prod.resolve_paths();

        assert_eq!(storage_prod, temp_workdir.join("acme").join("production"));
        assert_eq!(cert_prod, temp_workdir.join("acme").join("production").join("cert.pem"));
        assert_eq!(key_prod, temp_workdir.join("acme").join("production").join("key.pem"));

        // Staging and production paths must not collide
        assert_ne!(storage_staging, storage_prod);
        assert_ne!(cert_staging, cert_prod);
        assert_ne!(key_staging, key_prod);

        // 3. Custom storage_dir
        let mut cfg_custom = AcmeConfig::default();
        cfg_custom.storage_dir = temp_workdir.join("custom_acme").to_string_lossy().to_string();
        cfg_custom.directory_url = "staging".into();
        let mgr_custom = AcmeManager::new(cfg_custom, &temp_workdir);
        let (storage_custom, cert_custom, _) = mgr_custom.resolve_paths();

        assert_eq!(storage_custom, temp_workdir.join("custom_acme").join("staging"));
        assert_eq!(cert_custom, temp_workdir.join("custom_acme").join("staging").join("cert.pem"));

        // 4. Explicit cert_file / key_file override
        let mut cfg_explicit = AcmeConfig::default();
        cfg_explicit.cert_file = temp_workdir.join("my_cert.pem").to_string_lossy().to_string();
        cfg_explicit.key_file = temp_workdir.join("my_key.pem").to_string_lossy().to_string();
        let mgr_explicit = AcmeManager::new(cfg_explicit, &temp_workdir);
        let (storage_explicit, cert_explicit, key_explicit) = mgr_explicit.resolve_paths();

        assert_eq!(storage_explicit, temp_workdir.join("acme").join("production"));
        assert_eq!(cert_explicit, temp_workdir.join("my_cert.pem"));
        assert_eq!(key_explicit, temp_workdir.join("my_key.pem"));
    }
}
