//! Configuration file support for the Relift CLI.
//!
//! Relift supports a `relift.toml` configuration file that provides defaults
//! for scan, recovery, and filter options. Command-line arguments always take
//! precedence over config-file values.
//!
//! Config file locations (searched in order, first match wins):
//! 1. The path passed to `--config`
//! 2. `./relift.toml` (current working directory)
//! 3. `$HOME/.config/relift/config.toml` (Unix) or `%APPDATA%\relift\config.toml` (Windows)

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Default maximum candidates for a scan.
pub const DEFAULT_MAX_CANDIDATES: usize = 1_000_000;

/// All configurable options, loaded from `relift.toml` with CLI overrides.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub scan: ScanConfig,
    #[serde(default)]
    pub recover: RecoverConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub filter: FilterConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ScanConfig {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub include_live: Option<bool>,
    #[serde(default)]
    pub carve: Option<bool>,
    #[serde(default)]
    pub max_candidates: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RecoverConfig {
    #[serde(default)]
    pub verify: Option<bool>,
    #[serde(default)]
    pub min_confidence: Option<String>,
    #[serde(default)]
    pub no_verify: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct OutputConfig {
    #[serde(default)]
    pub dest: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FilterConfig {
    #[serde(default)]
    pub ext: Option<Vec<String>>,
    #[serde(default)]
    pub format: Option<Vec<String>>,
    #[serde(default)]
    pub name: Option<Vec<String>>,
    #[serde(default)]
    pub path: Option<Vec<String>>,
    #[serde(default)]
    pub min_confidence: Option<String>,
}

/// Resolve the config file path.
///
/// Returns the explicit `--config` path if provided, otherwise checks the
/// default locations in order and returns the first one that exists.
pub fn resolve_config_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }

    // 1. Current directory
    let cwd = PathBuf::from("relift.toml");
    if cwd.is_file() {
        return Some(cwd);
    }

    // 2. User config directory
    if let Some(dir) = user_config_dir() {
        let p = dir.join("relift").join("config.toml");
        if p.is_file() {
            return Some(p);
        }
    }

    None
}

/// The user-level config directory.
#[cfg(target_os = "windows")]
fn user_config_dir() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn user_config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config"))
        })
}

/// Load configuration from a file, returning defaults if no file is found.
pub fn load_config(explicit: Option<&Path>) -> anyhow::Result<Config> {
    match resolve_config_path(explicit) {
        Some(path) => {
            let contents = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("could not read config {}: {e}", path.display()))?;
            let config: Config = toml::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("could not parse config {}: {e}", path.display()))?;
            Ok(config)
        }
        None => Ok(Config::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_valid() {
        let c = Config::default();
        assert_eq!(c.scan.max_candidates, None);
        assert_eq!(c.recover.verify, None);
    }

    #[test]
    fn load_config_with_no_file_returns_defaults() {
        let c = load_config(None).unwrap();
        assert_eq!(c.scan.max_candidates, None);
    }

    #[test]
    fn parse_minimal_config() {
        let toml_str = r#"
[scan]
mode = "deep"
carve = true
max_candidates = 500000

[recover]
verify = false
min_confidence = "likely"

[filter]
ext = ["jpg", "png"]
"#;
        let c: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(c.scan.mode.as_deref(), Some("deep"));
        assert_eq!(c.scan.carve, Some(true));
        assert_eq!(c.scan.max_candidates, Some(500_000));
        assert_eq!(c.recover.verify, Some(false));
        assert_eq!(c.recover.min_confidence.as_deref(), Some("likely"));
        assert_eq!(c.filter.ext, Some(vec!["jpg".to_string(), "png".to_string()]));
    }
}
