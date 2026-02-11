use std::path::{Component, Path, PathBuf};

use anyhow::Context;
use directories::ProjectDirs;

#[derive(Debug, Clone)]
pub struct RuntimePaths {
    pub workdir: PathBuf,
    pub middleware_dir: PathBuf,
}

pub fn resolve_runtime_paths(
    workdir: Option<PathBuf>,
    config_path: &Path,
    middleware_dir: Option<PathBuf>,
) -> anyhow::Result<RuntimePaths> {
    let workdir = resolve_workdir(workdir)?;
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let middleware_dir = resolve_middleware_dir(config_dir, middleware_dir)?;
    Ok(RuntimePaths {
        workdir,
        middleware_dir,
    })
}

fn resolve_workdir(flag_or_env: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let mut wd = match flag_or_env {
        Some(p) => {
            if p.as_os_str().is_empty() {
                anyhow::bail!("workdir: empty path");
            }
            if p.is_relative() {
                std::env::current_dir()
                    .context("workdir: resolve cwd")?
                    .join(p)
            } else {
                p
            }
        }
        None => default_workdir()?,
    };

    wd = normalize_path(wd);
    if wd.as_os_str().is_empty() {
        anyhow::bail!("workdir: empty path");
    }
    Ok(wd)
}

fn resolve_middleware_dir(
    config_dir: &Path,
    flag_or_env: Option<PathBuf>,
) -> anyhow::Result<PathBuf> {
    let mut p = match flag_or_env {
        Some(p) => {
            if p.as_os_str().is_empty() {
                anyhow::bail!("middleware dir: empty path");
            }
            if p.is_relative() {
                config_dir.join(p)
            } else {
                p
            }
        }
        None => config_dir.join("middlewares"),
    };

    p = normalize_path(p);
    if p.as_os_str().is_empty() {
        anyhow::bail!("middleware dir: empty path");
    }
    Ok(p)
}

fn default_workdir() -> anyhow::Result<PathBuf> {
    // Linux: system-wide state dir.
    #[cfg(target_os = "linux")]
    {
        return Ok(PathBuf::from("/var/lib/prism"));
    }

    // Other OSes: per-user data dir.
    #[cfg(not(target_os = "linux"))]
    {
        let proj = ProjectDirs::from("com", "summpot", "prism")
            .context("workdir: resolve user data dir")?;
        Ok(proj.data_local_dir().to_path_buf())
    }
}

fn normalize_path(p: PathBuf) -> PathBuf {
    // Pure component-level cleanup (no filesystem access): removes redundant `.` segments.
    // We intentionally do not resolve `..`.
    let mut out = PathBuf::new();
    for c in p.components() {
        if matches!(c, Component::CurDir) {
            continue;
        }
        out.push(c.as_os_str());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn middleware_dir_defaults_to_config_dir_middlewares() {
        let cd = PathBuf::from("C:/tmp/prism_cfg");
        let rp = resolve_middleware_dir(&cd, None).expect("resolve");
        assert_eq!(rp, cd.join("middlewares"));
    }

    #[test]
    fn middleware_dir_relative_is_under_config_dir() {
        let cd = PathBuf::from("C:/tmp/prism_cfg");
        let rp = resolve_middleware_dir(&cd, Some(PathBuf::from("./p"))).expect("resolve");
        assert_eq!(rp, cd.join("p"));
    }
}
