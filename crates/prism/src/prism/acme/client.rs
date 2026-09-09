use std::{
    path::Path,
    time::Duration,
};
use anyhow::{Context, Result, bail};
use instant_acme::{
    Account, AccountCredentials, AuthorizationStatus, ChallengeType, Identifier,
    LetsEncrypt, NewAccount, NewOrder, OrderStatus, RetryPolicy, ZeroSsl,
};

use crate::prism::acme::cloudflare::CloudflareClient;

pub fn resolve_directory_url(url_or_preset: &str) -> String {
    let s = url_or_preset.trim();
    if s.is_empty()
        || s.eq_ignore_ascii_case("production")
        || s.eq_ignore_ascii_case("letsencrypt")
    {
        LetsEncrypt::Production.url().to_string()
    } else if s.eq_ignore_ascii_case("staging")
        || s.eq_ignore_ascii_case("letsencrypt-staging")
    {
        LetsEncrypt::Staging.url().to_string()
    } else if s.eq_ignore_ascii_case("zerossl") {
        ZeroSsl::Production.url().to_string()
    } else {
        s.to_string()
    }
}

pub fn directory_url_slug(url_or_preset: &str) -> String {
    let s = url_or_preset.trim();
    if s.is_empty()
        || s.eq_ignore_ascii_case("production")
        || s.eq_ignore_ascii_case("letsencrypt")
        || s == LetsEncrypt::Production.url()
    {
        "production".to_string()
    } else if s.eq_ignore_ascii_case("staging")
        || s.eq_ignore_ascii_case("letsencrypt-staging")
        || s == LetsEncrypt::Staging.url()
    {
        "staging".to_string()
    } else if s.eq_ignore_ascii_case("zerossl")
        || s == ZeroSsl::Production.url()
    {
        "zerossl".to_string()
    } else {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(s.as_bytes());
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        let safe_prefix: String = s
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .take(24)
            .collect();
        if safe_prefix.is_empty() {
            format!("custom_{}", &hash[..12])
        } else {
            format!("custom_{}_{}", safe_prefix, &hash[..8])
        }
    }
}

pub struct AcmeClient {
    cf_client: CloudflareClient,
    directory_url: String,
    email: Option<String>,
    domains: Vec<String>,
    propagation_timeout: Duration,
}

impl AcmeClient {
    pub fn new(
        cf_client: CloudflareClient,
        directory_url: String,
        email: Option<String>,
        domains: Vec<String>,
        propagation_timeout: Duration,
    ) -> Self {
        Self {
            cf_client,
            directory_url: resolve_directory_url(&directory_url),
            email,
            domains,
            propagation_timeout,
        }
    }

    /// Load or register an ACME account, saving credentials to `storage_dir/account.json`.
    async fn load_or_create_account(&self, storage_dir: &Path) -> Result<Account> {
        let account_file = storage_dir.join("account.json");
        if account_file.exists() {
            tracing::info!(
                path = %account_file.display(),
                "ACME: loading existing account credentials"
            );
            let data = std::fs::read_to_string(&account_file)
                .context("read cached ACME account credentials")?;
            let creds: AccountCredentials = serde_json::from_str(&data)
                .context("parse cached ACME account credentials")?;
            return Account::builder()?
                .from_credentials(creds)
                .await
                .context("reconstruct ACME account from credentials");
        }

        tracing::info!(
            directory = %self.directory_url,
            email = ?self.email,
            "ACME: creating new ACME account"
        );

        let mut contact_list = Vec::new();
        if let Some(ref email) = self.email {
            let trimmed = email.trim();
            if !trimmed.is_empty() {
                contact_list.push(format!("mailto:{trimmed}"));
            }
        }

        let contact_refs: Vec<&str> = contact_list.iter().map(|s| s.as_str()).collect();
        let new_account = NewAccount {
            contact: &contact_refs,
            terms_of_service_agreed: true,
            only_return_existing: false,
        };

        let (account, creds) = Account::builder()?
            .create(&new_account, self.directory_url.clone(), None)
            .await
            .context("create ACME account")?;

        let serialized = serde_json::to_string_pretty(&creds)?;
        std::fs::write(&account_file, serialized)
            .context("save ACME account credentials")?;

        tracing::info!(
            path = %account_file.display(),
            "ACME: created and saved new account credentials"
        );

        Ok(account)
    }

    /// Execute the complete ACME DNS-01 issuance flow for configured domains.
    /// Returns `(cert_chain_pem, private_key_pem)`.
    pub async fn issue_certificate(&self, storage_dir: &Path) -> Result<(String, String)> {
        if self.domains.is_empty() {
            bail!("cannot issue certificate: no domains specified");
        }

        std::fs::create_dir_all(storage_dir)
            .with_context(|| format!("create ACME storage dir {}", storage_dir.display()))?;

        let account = self.load_or_create_account(storage_dir).await?;

        let identifiers: Vec<Identifier> = self
            .domains
            .iter()
            .map(|d| Identifier::Dns(d.clone()))
            .collect();

        tracing::info!(
            domains = ?self.domains,
            "ACME: placing new certificate order"
        );

        let mut order = account
            .new_order(&NewOrder::new(&identifiers))
            .await
            .context("submit ACME order")?;

        let mut created_records: Vec<(String /* zone_id */, String /* record_id */)> = Vec::new();

        // Process authorizations
        let mut authz_stream = order.authorizations();
        while let Some(authz_res) = authz_stream.next().await {
            let mut authz = authz_res.context("retrieve authorization")?;
            match authz.status {
                AuthorizationStatus::Valid => {
                    tracing::debug!(
                        identifier = %authz.identifier(),
                        "ACME: authorization already valid"
                    );
                    continue;
                }
                AuthorizationStatus::Pending => {}
                other => {
                    bail!("unexpected authorization status: {other:?}");
                }
            }

            let mut challenge = authz
                .challenge(ChallengeType::Dns01)
                .ok_or_else(|| anyhow::anyhow!("no dns-01 challenge found for authorization"))?;

            let challenge_ident = challenge.identifier().to_string();
            let dns_value = challenge.key_authorization().dns_value();

            // RFC 8555: TXT record name is _acme-challenge.<clean_domain>
            let clean_domain = challenge_ident.trim_start_matches("*.");
            let record_name = format!("_acme-challenge.{clean_domain}");

            tracing::info!(
                domain = %challenge_ident,
                record = %record_name,
                "ACME: resolving Cloudflare zone for DNS-01 challenge"
            );

            let zone_id = self.cf_client.resolve_zone_id(&challenge_ident).await?;
            let record_id = self
                .cf_client
                .create_txt_record(&zone_id, &record_name, &dns_value)
                .await?;

            created_records.push((zone_id, record_id));

            // Verify DNS propagation
            self.cf_client
                .wait_for_propagation(&record_name, &dns_value, self.propagation_timeout)
                .await?;

            // Tell ACME server challenge is ready
            tracing::info!(
                domain = %challenge_ident,
                "ACME: notifying CA that DNS-01 challenge is ready"
            );
            challenge.set_ready().await.context("set challenge ready")?;
        }

        // Always clean up created DNS TXT records from Cloudflare
        for (zid, rid) in &created_records {
            if let Err(err) = self.cf_client.delete_txt_record(zid, rid).await {
                tracing::warn!(zone_id = %zid, record_id = %rid, err = %err, "ACME: failed to delete DNS TXT record");
            }
        }

        // Wait for order readiness
        tracing::info!("ACME: waiting for order to become ready");
        let status = order
            .poll_ready(&RetryPolicy::default())
            .await
            .context("poll order status")?;

        if status != OrderStatus::Ready {
            bail!("ACME order was not ready after challenges; status: {status:?}");
        }

        // Finalize order to generate private key and certificate
        tracing::info!("ACME: finalizing order");
        let private_key_pem = order.finalize().await.context("finalize ACME order")?;

        tracing::info!("ACME: polling issued certificate");
        let cert_chain_pem = order
            .poll_certificate(&RetryPolicy::default())
            .await
            .context("poll issued certificate")?;

        tracing::info!(
            domains = ?self.domains,
            "ACME: certificate successfully issued"
        );

        Ok((cert_chain_pem, private_key_pem))
    }
}
