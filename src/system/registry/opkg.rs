//! OpenWrt opkg package registry.
//!
//! IPK packages are ar archives (same format as .deb). The Packages index
//! format matches Debian's Packages.gz — reuse the APT parser.
use super::{PackageIndex, PackageMetadata};
use crate::error::{Result, OilError};
use std::time::{Duration, SystemTime};
use tracing::debug;

pub struct OpkgRegistry {
    baseurl: String,
    arch: String,
}

impl OpkgRegistry {
    pub fn new(baseurl: &str, arch: &str) -> Self {
        Self {
            baseurl: baseurl.trim_end_matches('/').to_string(),
            arch: arch.to_string(),
        }
    }

    pub fn openwrt_default() -> Self {
        let arch = std::env::consts::ARCH;
        let openwrt_arch = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64_cortex-a53",
            "arm" => "arm_arm1176jzf_s_vfp",
            "mips" => "mips_24kc",
            other => other,
        };
        // ponytail: snapshot release, not tracking the rolling "snapshots" branch
        Self::new(
            "https://downloads.openwrt.org/releases/23.05.5/packages",
            openwrt_arch,
        )
    }

    fn repo_url(&self) -> String {
        format!("{}/{}/Packages.gz", self.baseurl, self.arch)
    }

    fn cache_path(&self) -> Result<std::path::PathBuf> {
        let dir = crate::ui::dirs::oil_cache_dir()?.join("system");
        std::fs::create_dir_all(&dir)?;
        let safe: String = format!("{}-{}", self.baseurl, self.arch)
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        Ok(dir.join(format!("opkg-{}.json", safe)))
    }

    fn is_cache_fresh(path: &std::path::Path) -> bool {
        if let Ok(meta) = std::fs::metadata(path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                    return elapsed < Duration::from_secs(24 * 3600);
                }
            }
        }
        false
    }

    pub async fn load(&self, client: &reqwest::Client) -> Result<PackageIndex> {
        let cache_path = self.cache_path()?;
        if Self::is_cache_fresh(&cache_path) {
            let data = std::fs::read_to_string(&cache_path)?;
            let packages: Vec<PackageMetadata> = serde_json::from_str(&data)?;
            return Ok(PackageIndex { packages });
        }

        let url = self.repo_url();
        debug!("Fetching opkg Packages from {}", url);
        let resp = client.get(&url).send().await.map_err(|e| {
            OilError::InstallError(format!("Failed to fetch opkg repo: {}", e))
        })?;
        if !resp.status().is_success() {
            return Err(OilError::InstallError(format!(
                "opkg repo HTTP {}",
                resp.status()
            )));
        }
        let bytes = resp.bytes().await.map_err(|e| {
            OilError::InstallError(format!("Failed to read opkg repo: {}", e))
        })?;

        let mut decompressed = String::new();
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        use std::io::Read;
        decoder.read_to_string(&mut decompressed).map_err(|e| {
            OilError::InstallError(format!("Failed to decompress Packages.gz: {}", e))
        })?;

        // Reuse the APT registry's Packages file parser — same format
        let packages = crate::system::registry::apt::parse_packages_file(
            &decompressed,
            &self.baseurl,
        );
        debug!("Parsed {} opkg packages", packages.len());

        let json = serde_json::to_string(&packages)?;
        std::fs::write(&cache_path, &json)?;
        Ok(PackageIndex { packages })
    }
}
