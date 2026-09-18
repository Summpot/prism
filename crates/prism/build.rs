fn main() {
    let commit = std::env::var("PRISM_COMMIT_HASH")
        .or_else(|_| std::env::var("GITHUB_SHA"))
        .unwrap_or_else(|_| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short=7", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "unknown".to_string())
        });
    let short_commit: String = commit.chars().take(7).collect();

    let build_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    println!("cargo:rustc-env=PRISM_COMMIT_HASH={short_commit}");
    println!("cargo:rustc-env=PRISM_BUILD_TIME={build_time}");

    #[cfg(feature = "desktop")]
    tauri_build::build();
}
