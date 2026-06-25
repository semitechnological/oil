//! Solus eopkg package registry.
//!
//! Parses the Solus eopkg-index.xml, an XML file listing all packages
//! in the repository. Package files are .eopkg (tar.gz with metadata.xml).
use super::{PackageIndex, PackageMetadata};
use crate::error::{Result, OilError};
use std::time::{Duration, SystemTime};
use tracing::debug;
use quick_xml::events::Event;
use quick_xml::Reader;

pub struct EopkgRegistry {
    mirror: String,
    repo: String,
}

impl EopkgRegistry {
    pub fn new(mirror: &str, repo: &str) -> Self {
        Self {
            mirror: mirror.trim_end_matches('/').to_string(),
            repo: repo.to_string(),
        }
    }

    pub fn solus_default() -> Self {
        Self::new("https://solus-project.com/repositories", "main")
    }

    fn index_url(&self) -> String {
        format!("{}/eopkg-index.xml.xz", self.mirror)
    }

    fn cache_path(&self) -> Result<std::path::PathBuf> {
        let dir = crate::ui::dirs::oil_cache_dir()?.join("system");
        std::fs::create_dir_all(&dir)?;
        let safe: String = format!("{}-{}", self.mirror, self.repo)
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        Ok(dir.join(format!("eopkg-{}.json", safe)))
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

        let url = self.index_url();
        debug!("Fetching eopkg index from {}", url);
        let resp = client.get(&url).send().await.map_err(|e| {
            OilError::InstallError(format!("Failed to fetch eopkg index: {}", e))
        })?;
        if !resp.status().is_success() {
            return Err(OilError::InstallError(format!(
                "eopkg index HTTP {}",
                resp.status()
            )));
        }
        let bytes = resp.bytes().await.map_err(|e| {
            OilError::InstallError(format!("Failed to read eopkg index: {}", e))
        })?;

        // Decompress .xz
        let mut decompressed = Vec::new();
        let mut decoder = xz2::read::XzDecoder::new(&bytes[..]);
        use std::io::Read;
        decoder.read_to_end(&mut decompressed).map_err(|e| {
            OilError::InstallError(format!("Failed to decompress eopkg index: {}", e))
        })?;
        let xml = String::from_utf8(decompressed).map_err(|e| {
            OilError::InstallError(format!("eopkg index not valid UTF-8: {}", e))
        })?;

        let packages = parse_eopkg_index(&xml, &self.mirror);
        debug!("Parsed {} eopkg packages", packages.len());

        let json = serde_json::to_string(&packages)?;
        std::fs::write(&cache_path, &json)?;
        Ok(PackageIndex { packages })
    }
}

fn parse_eopkg_index(xml: &str, mirror: &str) -> Vec<PackageMetadata> {
    let mut packages = Vec::new();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();

    let mut in_package = false;
    let mut current_tag = String::new();
    let mut name = String::new();
    let mut version = String::new();
    let mut description = String::new();
    let mut release: String = String::new();
    let mut pkg_id: String = String::new();
    let mut pkg_arch: String = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local = e.local_name();
                let tag = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if tag == "Package" {
                    in_package = true;
                    name = String::new();
                    version = String::new();
                    description = String::new();
                    release = String::new();
                    pkg_id = String::new();
                    pkg_arch = String::new();
                } else if tag == "PackageURI" && in_package {
                    current_tag = "PackageURI".to_string();
                } else if tag == "Description" && in_package {
                    for attr in e.attributes().flatten() {
                        let local_name = attr.key.local_name();
                        let k = std::str::from_utf8(local_name.as_ref()).unwrap_or("");
                        if k == "summary" {
                            #[allow(deprecated)]
                            if let Ok(v) = attr.unescape_value() {
                                description = v.to_string();
                            }
                        }
                    }
                } else if in_package {
                    current_tag = tag.to_string();
                }
            }
            Ok(Event::Text(ref e)) if in_package => {
                let text = e.decode().ok().map(|c| c.to_string()).unwrap_or_default();
                match current_tag.as_str() {
                    "Name" => name = text,
                    "Version" => version = text,
                    "Release" => release = text,
                    "PackageURI" => pkg_id = text,
                    "Architecture" => pkg_arch = text,
                    _ => {}
                }
            }
            Ok(Event::End(ref e)) => {
                let local = e.local_name();
                let tag = std::str::from_utf8(local.as_ref()).unwrap_or("");
                if tag == "Package" && in_package && !name.is_empty() {
                    let pkg_version = if release.is_empty() {
                        version.clone()
                    } else {
                        format!("{}-{}", version, release)
                    };
                    let download_url = if pkg_id.starts_with("http") {
                        pkg_id.clone()
                    } else if !pkg_id.is_empty() {
                        format!("{}/{}", mirror, pkg_id)
                    } else {
                        format!(
                            "{}/packages/{}/{}-{}-{}.eopkg",
                            mirror, name, name, pkg_version, pkg_arch
                        )
                    };
                    packages.push(PackageMetadata {
                        name: name.clone(),
                        version: pkg_version,
                        description: description.clone(),
                        download_url,
                        sha256: None,
                        installed_size: 0,
                        depends: vec![],
                        provides: vec![],
                    });
                    in_package = false;
                }
                current_tag = String::new();
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    packages
}
