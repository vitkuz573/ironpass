//! Geo/asset detection for proxy-core backends.

use std::path::{Path, PathBuf};

/// Status of geo/routing assets for a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeoAssetStatus {
    pub available: bool,
}

impl GeoAssetStatus {
    pub fn new(available: bool) -> Self {
        Self { available }
    }
}

/// Detect whether `geoip.dat` and `geosite.dat` are available next to a core
/// binary or in standard system/user locations.
pub fn detect_geo_assets(binary_path: Option<&Path>) -> GeoAssetStatus {
    let candidates: Vec<PathBuf> = if let Some(bin) = binary_path {
        let mut list = vec![bin.parent().map(Path::to_path_buf).unwrap_or_default()];
        list.extend(standard_geo_dirs());
        list
    } else {
        standard_geo_dirs()
    };

    let geoip = candidates
        .iter()
        .map(|d| d.join("geoip.dat"))
        .any(|p| p.is_file());
    let geosite = candidates
        .iter()
        .map(|d| d.join("geosite.dat"))
        .any(|p| p.is_file());
    GeoAssetStatus::new(geoip && geosite)
}

/// Standard directories where geo assets are commonly installed.
fn standard_geo_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/share/xray"),
        PathBuf::from("/usr/share/v2ray"),
    ];
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".config/xray"));
    }
    dirs
}

/// Locate a core binary by explicit path or by searching PATH.
pub fn locate_core_binary(names: &[&str], explicit_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = explicit_path
        && path.is_file()
    {
        return Some(path.to_path_buf());
    }

    for name in names {
        if let Ok(path) = which_in_path(name) {
            return Some(path);
        }
    }

    None
}

fn which_in_path(name: &str) -> anyhow::Result<PathBuf> {
    let path_env = std::env::var_os("PATH").ok_or_else(|| anyhow::anyhow!("PATH not set"))?;
    for dir in std::env::split_paths(&path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("{name} not found in PATH")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_assets_report_not_available() {
        let status = detect_geo_assets(Some(Path::new("/nonexistent/bin/core")));
        assert!(!status.available);
    }

    #[test]
    fn assets_next_to_binary_are_detected() {
        let temp = std::env::temp_dir().join(format!(
            "ironpass-geo-test-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::create_dir_all(&temp);
        let _ = std::fs::File::create(temp.join("geoip.dat"));
        let _ = std::fs::File::create(temp.join("geosite.dat"));
        let status = detect_geo_assets(Some(&temp.join("xray")));
        assert!(status.available);
        let _ = std::fs::remove_dir_all(&temp);
    }
}
