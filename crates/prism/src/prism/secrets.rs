//! OS credential-store helpers for tunnel tokens.
//!
//! Raw tokens prefer the platform store (Windows Credential Manager, macOS
//! Keychain, Linux Secret Service). Callers must keep a SQLite blob fallback
//! when these functions report that the keyring was not used.

const SERVICE: &str = "com.summpot.prism";

fn account_for_profile(profile_id: &str) -> String {
    let mut out = String::from("tunnel:");
    for ch in profile_id.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out == "tunnel:" {
        out.push_str("default");
    }
    out
}

/// Stores `token` for `profile_id`. Returns `true` when the OS keyring accepted it.
pub fn store_tunnel_token(profile_id: &str, token: &str) -> bool {
    let token = token.trim();
    if profile_id.trim().is_empty() || token.is_empty() {
        return false;
    }
    let Ok(entry) = keyring::Entry::new(SERVICE, &account_for_profile(profile_id)) else {
        return false;
    };
    entry.set_password(token).is_ok()
}

/// Loads a previously stored tunnel token from the OS keyring.
pub fn load_tunnel_token(profile_id: &str) -> Option<String> {
    if profile_id.trim().is_empty() {
        return None;
    }
    let entry = keyring::Entry::new(SERVICE, &account_for_profile(profile_id)).ok()?;
    match entry.get_password() {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

/// Deletes a tunnel token from the OS keyring. Missing entries are ignored.
pub fn delete_tunnel_token(profile_id: &str) {
    if profile_id.trim().is_empty() {
        return;
    }
    if let Ok(entry) = keyring::Entry::new(SERVICE, &account_for_profile(profile_id)) {
        let _ = entry.delete_credential();
    }
}

#[cfg(test)]
mod tests {
    use super::account_for_profile;

    #[test]
    fn account_sanitizes_profile_id() {
        assert_eq!(account_for_profile("profile-1"), "tunnel:profile-1");
        assert_eq!(account_for_profile("a/b:c"), "tunnel:a_b_c");
        assert_eq!(account_for_profile(""), "tunnel:default");
    }
}
