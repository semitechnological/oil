use super::{PackageIndex, PackageMetadata};
use crate::error::{Result, WaxError};
use flate2::read::GzDecoder;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::time::{Duration, SystemTime};
use tracing::{debug, warn};

pub struct DnfRegistry {
    baseurl: String,
}

impl DnfRegistry {
    pub fn new(baseurl: &str) -> Self {
        Self {
            baseurl: baseurl.trim_end_matches('/').to_string(),
        }
    }

    pub fn fedora_default() -> Self {
        Self::new("https://dl.fedoraproject.org/pub/fedora/linux/releases/39/Everything/x86_64/os/")
    }

    fn cache_path(&self) -> Result<std::path::PathBuf> {
        let dir = crate::ui::dirs::wax_cache_dir()?.join("system");
        std::fs::create_dir_all(&dir)?;
        let safe: String = self
            .baseurl
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect();
        let short: String = safe
            .chars()
            .rev()
            .take(40)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        Ok(dir.join(format!("dnf-{}.json", short)))
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
            debug!("Loading DNF index from cache: {:?}", cache_path);
            let data = std::fs::read_to_string(&cache_path)?;
            let packages: Vec<PackageMetadata> = serde_json::from_str(&data)?;
            return Ok(PackageIndex { packages });
        }

        debug!("Fetching DNF repomd.xml from {}", self.baseurl);

        let repomd_url = format!("{}/repodata/repomd.xml", self.baseurl);
        let resp =
            client.get(&repomd_url).send().await.map_err(|e| {
                WaxError::InstallError(format!("Failed to fetch repomd.xml: {}", e))
            })?;

        if !resp.status().is_success() {
            return Err(WaxError::InstallError(format!(
                "Failed to fetch repomd.xml: HTTP {}",
                resp.status()
            )));
        }

        let repomd_xml = resp
            .text()
            .await
            .map_err(|e| WaxError::InstallError(format!("Failed to read repomd.xml: {}", e)))?;

        let primary_location = find_primary_location(&repomd_xml).ok_or_else(|| {
            WaxError::InstallError("Could not find primary.xml in repomd.xml".to_string())
        })?;

        let primary_url = format!("{}/{}", self.baseurl, primary_location);
        debug!("Fetching primary index: {}", primary_url);

        let resp =
            client.get(&primary_url).send().await.map_err(|e| {
                WaxError::InstallError(format!("Failed to fetch primary.xml: {}", e))
            })?;

        if !resp.status().is_success() {
            return Err(WaxError::InstallError(format!(
                "Failed to fetch primary index: HTTP {}",
                resp.status()
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| WaxError::InstallError(format!("Failed to read primary index: {}", e)))?;

        let xml_content = if primary_location.ends_with(".gz") {
            let mut decoder = GzDecoder::new(&bytes[..]);
            let mut s = String::new();
            decoder.read_to_string(&mut s).map_err(|e| {
                WaxError::InstallError(format!("Failed to decompress primary.xml.gz: {}", e))
            })?;
            s
        } else if primary_location.ends_with(".zst") {
            let mut decoder = zstd::Decoder::new(&bytes[..]).map_err(|e| {
                WaxError::InstallError(format!("Failed to create zstd decoder: {}", e))
            })?;
            let mut s = String::new();
            decoder.read_to_string(&mut s).map_err(|e| {
                WaxError::InstallError(format!("Failed to decompress primary.xml.zst: {}", e))
            })?;
            s
        } else {
            String::from_utf8(bytes.to_vec()).map_err(|e| {
                WaxError::InstallError(format!("primary.xml is not valid UTF-8: {}", e))
            })?
        };

        let packages = parse_primary_xml(&xml_content, &self.baseurl)
            .map_err(|e| WaxError::InstallError(format!("Failed to parse primary.xml: {}", e)))?;

        debug!("Parsed {} packages from DNF repo", packages.len());

        if packages.is_empty() {
            warn!("DNF index returned 0 packages — possible parse error");
        }

        let json = serde_json::to_string(&packages)?;
        std::fs::write(&cache_path, &json)?;

        Ok(PackageIndex { packages })
    }
}

/// Helper to get local name of a quick-xml attribute key as an owned String.
fn attr_local_name(attr: &quick_xml::events::attributes::Attribute) -> String {
    let local = attr.key.local_name();
    std::str::from_utf8(local.as_ref())
        .unwrap_or("")
        .to_string()
}

fn find_primary_location(repomd_xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(repomd_xml);
    let mut in_primary = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local_name = {
                    let local = e.local_name();
                    std::str::from_utf8(local.as_ref())
                        .unwrap_or("")
                        .to_string()
                };

                if local_name == "data" {
                    in_primary = false;
                    for attr in e.attributes().flatten() {
                        let key = attr_local_name(&attr);
                        let val = attr.unescape_value().unwrap_or_default();
                        if key == "type" && val.as_ref() == "primary" {
                            in_primary = true;
                        }
                    }
                }

                if in_primary && local_name == "location" {
                    for attr in e.attributes().flatten() {
                        let key = attr_local_name(&attr);
                        let val = attr.unescape_value().unwrap_or_default();
                        if key == "href" {
                            return Some(val.to_string());
                        }
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local_name = {
                    let local = e.local_name();
                    std::str::from_utf8(local.as_ref())
                        .unwrap_or("")
                        .to_string()
                };
                if local_name == "data" {
                    in_primary = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    None
}

fn parse_primary_xml(xml: &str, baseurl: &str) -> Result<Vec<PackageMetadata>> {
    let mut reader = Reader::from_str(xml);
    let mut packages = Vec::new();
    let mut buf = Vec::new();

    let mut in_package = false;
    let mut current_tag = String::new();

    let mut name = String::new();
    let mut version = String::new();
    let mut description = String::new();
    let mut location_href = String::new();
    let mut sha256: Option<String> = None;
    let mut installed_size: u64 = 0;
    let mut depends: Vec<String> = Vec::new();
    let mut in_requires = false;
    let mut checksum_is_sha256 = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let local_name = {
                    let local = e.local_name();
                    std::str::from_utf8(local.as_ref())
                        .unwrap_or("")
                        .to_string()
                };

                match local_name.as_str() {
                    "package" => {
                        let mut is_rpm = false;
                        for attr in e.attributes().flatten() {
                            let key = attr_local_name(&attr);
                            let val = attr.unescape_value().unwrap_or_default();
                            if key == "type" && val.as_ref() == "rpm" {
                                is_rpm = true;
                            }
                        }
                        if is_rpm {
                            in_package = true;
                            name = String::new();
                            version = String::new();
                            description = String::new();
                            location_href = String::new();
                            sha256 = None;
                            installed_size = 0;
                            depends = Vec::new();
                        }
                    }
                    "version" if in_package => {
                        let mut ver = String::new();
                        let mut rel = String::new();
                        for attr in e.attributes().flatten() {
                            let key = attr_local_name(&attr);
                            let val = attr.unescape_value().unwrap_or_default();
                            match key.as_str() {
                                "ver" => ver = val.to_string(),
                                "rel" => rel = val.to_string(),
                                _ => {}
                            }
                        }
                        if !rel.is_empty() {
                            version = format!("{}-{}", ver, rel);
                        } else {
                            version = ver;
                        }
                    }
                    "location" if in_package => {
                        for attr in e.attributes().flatten() {
                            let key = attr_local_name(&attr);
                            let val = attr.unescape_value().unwrap_or_default();
                            if key == "href" {
                                location_href = val.to_string();
                            }
                        }
                    }
                    "checksum" if in_package => {
                        checksum_is_sha256 = false;
                        for attr in e.attributes().flatten() {
                            let key = attr_local_name(&attr);
                            let val = attr.unescape_value().unwrap_or_default();
                            if key == "type" && val.as_ref() == "sha256" {
                                checksum_is_sha256 = true;
                            }
                        }
                        if checksum_is_sha256 {
                            current_tag = "checksum".to_string();
                        }
                    }
                    "size" if in_package => {
                        for attr in e.attributes().flatten() {
                            let key = attr_local_name(&attr);
                            let val = attr.unescape_value().unwrap_or_default();
                            if key == "installed" {
                                installed_size = val.parse().unwrap_or(0);
                            }
                        }
                    }
                    "requires" => in_requires = true,
                    "entry" if in_package && in_requires => {
                        for attr in e.attributes().flatten() {
                            let key = attr_local_name(&attr);
                            let val = attr.unescape_value().unwrap_or_default();
                            if key == "name" {
                                let dname = val.trim_start_matches('/');
                                if !dname.starts_with("rpmlib(") {
                                    depends.push(dname.to_string());
                                }
                            }
                        }
                    }
                    _ => {
                        if in_package {
                            current_tag = local_name;
                        }
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_package {
                    let text = e
                        .decode()
                        .ok()
                        .and_then(|text| unescape(&text).ok().map(|text| text.into_owned()))
                        .unwrap_or_default();
                    match current_tag.as_str() {
                        "name" => name = text,
                        "summary" => {
                            if description.is_empty() {
                                description = text;
                            }
                        }
                        "description" => {
                            if description.is_empty() {
                                description = text.lines().next().unwrap_or("").to_string();
                            }
                        }
                        "checksum" if checksum_is_sha256 => sha256 = Some(text),
                        _ => {}
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                let local_name = {
                    let local = e.local_name();
                    std::str::from_utf8(local.as_ref())
                        .unwrap_or("")
                        .to_string()
                };
                match local_name.as_str() {
                    "package" if in_package => {
                        if !name.is_empty() && !version.is_empty() && !location_href.is_empty() {
                            let download_url = format!("{}/{}", baseurl, location_href);
                            packages.push(PackageMetadata {
                                name: name.clone(),
                                version: version.clone(),
                                description: description.clone(),
                                download_url,
                                sha256: sha256.clone(),
                                installed_size,
                                depends: depends.clone(),
                                provides: Vec::new(),
                            });
                        }
                        in_package = false;
                        current_tag = String::new();
                        checksum_is_sha256 = false;
                    }
                    "requires" => in_requires = false,
                    _ => {
                        current_tag = String::new();
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                warn!("XML parse error in primary.xml: {}", e);
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(packages)
}
