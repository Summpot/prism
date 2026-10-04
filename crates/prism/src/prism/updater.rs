//! Signed in-app updater. Checks the GitHub release manifest, verifies the
//! asset with minisign, then replaces the running binary.

use std::collections::HashMap;
use std::path::PathBuf;
use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

pub const DEFAULT_PUBKEY: &str =
    "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDYyQkZCRDc2OEM4QjQxNzQKUldSMFFZdU1kcjIvWW81cTdBZU9MTkZ6T2QvazhFWWM2RTU1T2tJUUVIOFYwSlgwYmQwUmk1Z0kK";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResponse {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub date: Option<String>,
    pub body: Option<String>,
    pub channel: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestPlatform {
    pub signature: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateManifest {
    pub version: String,
    pub notes: Option<String>,
    pub pub_date: Option<String>,
    pub platforms: HashMap<String, ManifestPlatform>,
}

pub fn parse_minisign_pubkey(raw: &str) -> anyhow::Result<PublicKey> {
    let trimmed = raw.trim();
    if let Ok(pk) = PublicKey::from_base64(trimmed) {
        return Ok(pk);
    }
    if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
        if let Ok(s) = String::from_utf8(decoded_bytes) {
            for line in s.lines() {
                let l = line.trim();
                if l.starts_with("RW") {
                    if let Ok(pk) = PublicKey::from_base64(l) {
                        return Ok(pk);
                    }
                }
            }
        }
    }
    for line in trimmed.lines() {
        let l = line.trim();
        if l.starts_with("RW") {
            if let Ok(pk) = PublicKey::from_base64(l) {
                return Ok(pk);
            }
        }
    }
    anyhow::bail!("failed to parse Minisign public key from provided configuration")
}

pub fn parse_minisign_signature(raw: &str) -> anyhow::Result<Signature> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("signature is empty");
    }
    // 1. Try decoding directly as raw multiline format
    if let Ok(sig) = Signature::decode(trimmed) {
        return Ok(sig);
    }
    // 2. Try decoding from base64 (Tauri updater format)
    if let Ok(decoded_bytes) = base64::engine::general_purpose::STANDARD.decode(trimmed) {
        if let Ok(s) = String::from_utf8(decoded_bytes) {
            if let Ok(sig) = Signature::decode(&s) {
                return Ok(sig);
            }
        }
    }
    anyhow::bail!("failed to parse Minisign signature: invalid format or encoding")
}

pub fn current_platform_keys() -> &'static [&'static str] {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return &["windows-x86_64"];
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    return &["windows-aarch64"];
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return &["darwin-x86_64"];
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return &["darwin-aarch64"];
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return &["linux-x86_64"];
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return &["linux-aarch64"];

    #[cfg(not(any(
        all(target_os = "windows", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))
    )))]
    return &[];
}

fn extract_commit_identifier(version: &str, notes: Option<&str>) -> Option<String> {
    if let Some(notes) = notes {
        if let Some(start) = notes.rfind('(') {
            if let Some(end) = notes[start..].find(')') {
                let candidate = notes[start + 1..start + end].trim();
                if candidate.len() >= 7 && candidate.chars().all(|c| c.is_ascii_hexdigit()) {
                    return Some(candidate.to_ascii_lowercase());
                }
            }
        }
    }
    for part in version.split(|c| c == '.' || c == '-' || c == '+') {
        if part.len() >= 7 && part.chars().all(|c| c.is_ascii_hexdigit()) {
            return Some(part.to_ascii_lowercase());
        }
    }
    None
}

pub fn is_version_newer(
    current_ver: &str,
    remote_ver: &str,
    remote_notes: Option<&str>,
    remote_pub_date: Option<&str>,
    is_dev: bool,
) -> bool {
    if !is_dev {
        return remote_ver > current_ver;
    }

    let local_commit = env!("PRISM_COMMIT_HASH").trim().to_ascii_lowercase();
    let local_build_time: u64 = env!("PRISM_BUILD_TIME").parse().unwrap_or(0);

    // 1. If remote commit hash can be extracted and local commit hash is known
    if let Some(remote_commit) = extract_commit_identifier(remote_ver, remote_notes) {
        if !local_commit.is_empty() && local_commit != "unknown" {
            if remote_commit.starts_with(&local_commit) || local_commit.starts_with(&remote_commit) {
                return false;
            }
            if let Some(pub_date_str) = remote_pub_date {
                if let Ok(dt) = humantime::parse_rfc3339(pub_date_str) {
                    let remote_ts = dt
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    if remote_ts > 0 && local_build_time > 0 {
                        return remote_ts > local_build_time;
                    }
                }
            }
            return true;
        }
    }

    // 2. Publication timestamp check
    if let Some(pub_date_str) = remote_pub_date {
        if let Ok(dt) = humantime::parse_rfc3339(pub_date_str) {
            let remote_ts = dt
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            if remote_ts > 0 && local_build_time > 0 {
                return remote_ts > local_build_time;
            }
        }
    }

    // 3. Fallback to basic string comparison
    remote_ver != current_ver
}

pub async fn check_update(
    client: &reqwest::Client,
    channel: &str,
    current_version: &str,
) -> anyhow::Result<(UpdateCheckResponse, Option<(UpdateManifest, ManifestPlatform)>)> {
    let endpoint = match channel {
        "dev" => "https://github.com/Summpot/prism/releases/download/dev/latest.json",
        _ => "https://github.com/Summpot/prism/releases/latest/download/latest.json",
    };

    let resp = client
        .get(endpoint)
        .header("User-Agent", "Prism-Client-Updater")
        .send()
        .await?
        .error_for_status()?;

    let manifest: UpdateManifest = resp.json().await?;

    let keys = current_platform_keys();
    let mut matched_platform: Option<ManifestPlatform> = None;
    for k in keys {
        if let Some(p) = manifest.platforms.get(*k) {
            if !p.signature.trim().is_empty() {
                matched_platform = Some(p.clone());
                break;
            } else {
                tracing::warn!(platform = %k, "platform manifest entry has empty signature; skipping");
            }
        }
    }

    let is_dev = channel == "dev";
    let is_newer = is_version_newer(
        current_version,
        &manifest.version,
        manifest.notes.as_deref(),
        manifest.pub_date.as_deref(),
        is_dev,
    );

    let has_platform = matched_platform.is_some();
    let available = is_newer && has_platform;

    let response = UpdateCheckResponse {
        available,
        current_version: current_version.to_string(),
        version: if available { Some(manifest.version.clone()) } else { None },
        date: if available { manifest.pub_date.clone() } else { None },
        body: if available { manifest.notes.clone() } else { None },
        channel: channel.to_string(),
    };

    let update_data = if available {
        matched_platform.map(|p| (manifest, p))
    } else {
        None
    };

    Ok((response, update_data))
}

pub async fn download_and_install_update(
    client: &reqwest::Client,
    pubkey_str: &str,
    target_platform: &ManifestPlatform,
    version: &str,
) -> anyhow::Result<()> {
    tracing::info!(url = %target_platform.url, version = %version, "downloading update asset");

    let resp = client
        .get(&target_platform.url)
        .header("User-Agent", "Prism-Client-Updater")
        .send()
        .await?
        .error_for_status()?;

    let asset_bytes = resp.bytes().await?;

    // 1. Minisign verification
    let pk = parse_minisign_pubkey(pubkey_str)?;
    let sig = parse_minisign_signature(&target_platform.signature)
        .map_err(|e| anyhow::anyhow!("failed to decode signature: {e}"))?;
    pk.verify(&asset_bytes, &sig, false)
        .map_err(|e| anyhow::anyhow!("signature verification failed: {e}"))?;

    tracing::info!("update asset signature verified successfully");

    // 2. Perform installation
    let filename = target_platform
        .url
        .rsplit('/')
        .next()
        .unwrap_or("update.bin");

    let temp_dir = std::env::temp_dir();
    let temp_asset_path = temp_dir.join(format!("prism-update-{}-{}", version, filename));
    std::fs::write(&temp_asset_path, &asset_bytes)?;

    #[cfg(target_os = "windows")]
    {
        if filename.ends_with(".exe") {
            tracing::info!(path = ?temp_asset_path, "spawning Windows NSIS installer to update Prism");
            spawn_windows_installer_and_restart(&temp_asset_path)?;
        } else {
            anyhow::bail!("unsupported Windows update package format: {filename}");
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;

        let target_path = if filename.ends_with(".tar.gz") {
            let unpack_dir = temp_dir.join(format!("prism-unpack-{}", version));
            let _ = std::fs::create_dir_all(&unpack_dir);
            let status = std::process::Command::new("tar")
                .args(["-xzf", temp_asset_path.to_str().unwrap(), "-C", unpack_dir.to_str().unwrap()])
                .status()?;
            if !status.success() {
                anyhow::bail!("failed to extract Linux update tarball");
            }

            let mut extracted: Option<PathBuf> = None;
            if let Ok(entries) = std::fs::read_dir(&unpack_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().map(|e| e == "AppImage").unwrap_or(false)
                        || p.file_name().map(|n| n.to_string_lossy().contains("AppImage")).unwrap_or(false)
                    {
                        extracted = Some(p);
                        break;
                    }
                }
            }
            extracted.ok_or_else(|| anyhow::anyhow!("no AppImage found in downloaded update archive"))?
        } else {
            temp_asset_path
        };

        let mut perms = std::fs::metadata(&target_path)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&target_path, perms)?;

        if let Ok(appimage_path) = std::env::var("APPIMAGE") {
            tracing::info!(source = ?target_path, target = ?appimage_path, "replacing running AppImage");
            std::fs::copy(&target_path, &appimage_path)?;
            let args = clean_restart_args(std::env::args().skip(1));
            let pid = std::process::id();
            std::process::Command::new(&appimage_path)
                .args(args)
                .env("PRISM_RESTART_PID", pid.to_string())
                .env("PRISM_UPDATED", "1")
                .spawn()?;
            std::process::exit(0);
        } else {
            self_replace::self_replace(&target_path)?;
            restart_app()?;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if filename.ends_with(".tar.gz") {
            let unpack_dir = temp_dir.join(format!("prism-unpack-{}", version));
            let _ = std::fs::create_dir_all(&unpack_dir);
            let status = std::process::Command::new("tar")
                .args(["-xzf", temp_asset_path.to_str().unwrap(), "-C", unpack_dir.to_str().unwrap()])
                .status()?;
            if !status.success() {
                anyhow::bail!("failed to extract macOS app tarball");
            }

            // Find extracted .app directory
            let mut extracted_app: Option<PathBuf> = None;
            if let Ok(entries) = std::fs::read_dir(&unpack_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().map(|e| e == "app").unwrap_or(false) {
                        extracted_app = Some(p);
                        break;
                    }
                }
            }

            if let Some(app_bundle) = extracted_app {
                let current_exe = std::env::current_exe()?;
                // Current exe is inside Prism.app/Contents/MacOS/prism
                let target_app = current_exe
                    .ancestors()
                    .find(|p| p.extension().map(|e| e == "app").unwrap_or(false))
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("/Applications/Prism.app"));

                tracing::info!(source = ?app_bundle, target = ?target_app, "replacing macOS app bundle");
                let _ = std::process::Command::new("cp")
                    .args(["-R", app_bundle.to_str().unwrap(), target_app.to_str().unwrap()])
                    .status();

                let _ = std::process::Command::new("open")
                    .args(["-n", target_app.to_str().unwrap(), "--args", "--updated"])
                    .spawn();
                std::process::exit(0);
            } else {
                anyhow::bail!("no .app bundle found in downloaded archive");
            }
        } else {
            self_replace::self_replace(&temp_asset_path)?;
            restart_app()?;
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        self_replace::self_replace(&temp_asset_path)?;
        restart_app()?;
    }

    Ok(())
}

pub fn clean_restart_args<I, S>(raw_args: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut cleaned: Vec<String> = raw_args
        .into_iter()
        .map(|s| s.as_ref().to_string())
        .filter(|a| a != "--autostart")
        .collect();
    if !cleaned.iter().any(|a| a == "--updated") {
        cleaned.push("--updated".to_string());
    }
    cleaned
}

#[cfg(target_os = "windows")]
fn spawn_windows_installer_and_restart(installer_path: &std::path::Path) -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let install_dir = current_exe
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let pid = std::process::id();
    let current_args = clean_restart_args(std::env::args().skip(1));

    tracing::info!(
        installer = ?installer_path,
        target_dir = ?install_dir,
        current_exe = ?current_exe,
        pid,
        args = ?current_args,
        "spawning Windows installer helper to replace binary and restart"
    );

    let ps_installer = installer_path.display().to_string().replace('\'', "''");
    let ps_target_exe = current_exe.display().to_string().replace('\'', "''");
    let ps_install_dir = install_dir.display().to_string().replace('\'', "''");

    let args_clause = if current_args.is_empty() {
        String::new()
    } else {
        let quoted: Vec<String> = current_args
            .iter()
            .map(|a| format!("'{}'", a.replace('\'', "''")))
            .collect();
        format!("-ArgumentList {}", quoted.join(","))
    };

    let ps_script = format!(
        "Wait-Process -Id {pid} -Timeout 20 -ErrorAction SilentlyContinue; \
         Start-Sleep -Milliseconds 600; \
         $p = Start-Process -FilePath '{ps_installer}' -ArgumentList '/S', \"/D={ps_install_dir}\" -Wait -PassThru; \
         Start-Sleep -Milliseconds 500; \
         $env:PRISM_UPDATED = '1'; \
         $env:PRISM_RESTART_PID = '{pid}'; \
         if (Test-Path -LiteralPath '{ps_target_exe}') {{ \
             Start-Process -FilePath '{ps_target_exe}' -WindowStyle Normal {args_clause}; \
         }} \
         Remove-Item -LiteralPath '{ps_installer}' -Force -ErrorAction SilentlyContinue;"
    );

    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &ps_script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;

    std::process::exit(0);
}

pub fn restart_app() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let args = clean_restart_args(std::env::args().skip(1));
    let pid = std::process::id();
    tracing::info!(exe = ?current_exe, pid, args = ?args, "restarting Prism client");
    std::process::Command::new(current_exe)
        .args(args)
        .env("PRISM_RESTART_PID", pid.to_string())
        .env("PRISM_UPDATED", "1")
        .spawn()?;
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minisign_pubkey() {
        assert!(parse_minisign_pubkey(DEFAULT_PUBKEY).is_ok());
        let raw = "RWR0QYuMdr2/Yo5q7AeOLNFzOd/k8EYc6E55OkIQEH8V0JX0bd0Ri5gI";
        assert!(parse_minisign_pubkey(raw).is_ok());
        assert!(parse_minisign_pubkey("invalid-key").is_err());
    }

    #[test]
    fn test_extract_commit_identifier() {
        assert_eq!(
            extract_commit_identifier("0.1.0-dev.b6b85c5", Some("Prism dev build (b6b85c5)")),
            Some("b6b85c5".to_string())
        );
        assert_eq!(
            extract_commit_identifier("0.1.0-dev.1234567", None),
            Some("1234567".to_string())
        );
        assert_eq!(
            extract_commit_identifier("0.1.0", None),
            None
        );
    }

    #[test]
    fn release_channel_compares_versions_as_ordered_strings() {
        assert!(is_version_newer("0.1.0", "0.2.0", None, None, false));
        assert!(!is_version_newer("0.2.0", "0.2.0", None, None, false));
        assert!(!is_version_newer("0.2.0", "0.1.9", None, None, false));
    }

    #[test]
    fn test_parse_minisign_signature() {
        let b64_sig = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVSMFFZdU1kcjIvWXNldE9sNkEwd2szSUU5K2dDZkxMcGlPY3RwUFViR3o1TWFjS00vdlR2Z2xzMHFHU2tOOUY4WktqSlh0WFNyYmdCenZIVjFhSFQzNUN1QVprU2ZLdUFnPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkwNTkyOTYyCWZpbGU6UHJpc21fMC4xLjAtZGV2LmU0MTg4NDdfYW1kNjQuQXBwSW1hZ2UudGFyLmd6CkJUV1hLSjZ6RjV3ejFNKzNySHRkOUJxaUU5SmF2YzZZWTVRUkRjZTNBV0lsYTNXQzhINVFoNGNpYTYxdGdKajQyWTNZM2swdjEya0R5ZUZHTncxR0FnPT0K";
        assert!(parse_minisign_signature(b64_sig).is_ok());
        assert!(parse_minisign_signature("").is_err());
        assert!(parse_minisign_signature("   ").is_err());
        assert!(parse_minisign_signature("not-a-sig").is_err());
    }

    #[test]
    fn test_clean_restart_args() {
        let raw = vec!["--autostart", "--foo", "bar"];
        let cleaned = clean_restart_args(raw);
        assert_eq!(cleaned, vec!["--foo", "bar", "--updated"]);
        assert!(!cleaned.iter().any(|a| a == "--autostart"));
        assert!(cleaned.iter().any(|a| a == "--updated"));

        // If already has --updated, don't duplicate
        let raw2 = vec!["--updated"];
        let cleaned2 = clean_restart_args(raw2);
        assert_eq!(cleaned2, vec!["--updated"]);
    }
}
