use crate::error::Result;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum PackageFormat {
    Brew,
    Deb, // apt / dpkg
    Rpm, // dnf / rpm
    Pacman,
    Apk, // Alpine
    Other,
}

#[derive(Debug, Clone)]
pub struct DistroInfo {
    #[allow(dead_code)]
    pub id: String,
    pub name: String,
    pub version: String,
    #[allow(dead_code)]
    pub format: PackageFormat,
}

impl DistroInfo {
    pub async fn detect() -> Result<Option<Self>> {
        if cfg!(target_os = "macos") {
            let version =
                crate::bottle::run_command_with_timeout("sw_vers", &["-productVersion"], 1)
                    .unwrap_or_default();
            return Ok(Some(DistroInfo {
                id: "macos".to_string(),
                name: "macOS".to_string(),
                version,
                format: PackageFormat::Brew,
            }));
        }

        let path = "/etc/os-release";
        let Ok(raw) = tokio::fs::read_to_string(path).await else {
            return Ok(None);
        };

        let fields: HashMap<String, String> = raw
            .lines()
            .filter_map(|line| {
                let (k, v) = line.split_once('=')?;
                let v = v.trim_matches('"').to_string();
                Some((k.to_string(), v))
            })
            .collect();

        let id = fields.get("ID").cloned().unwrap_or_default();
        let name = fields.get("NAME").cloned().unwrap_or_else(|| id.clone());
        let version = fields
            .get("VERSION_ID")
            .cloned()
            .unwrap_or_else(|| fields.get("VERSION").cloned().unwrap_or_default());

        let id_like = fields.get("ID_LIKE").cloned().unwrap_or_default();

        let format = detect_format(&id, &id_like);

        Ok(Some(DistroInfo {
            id,
            name,
            version,
            format,
        }))
    }
}

fn detect_format(id: &str, id_like: &str) -> PackageFormat {
    let id = id.to_lowercase();
    let like = id_like.to_lowercase();

    let deb_ids = [
        "debian",
        "ubuntu",
        "linuxmint",
        "pop",
        "elementary",
        "kali",
        "parrot",
    ];
    let rpm_ids = [
        "fedora", "rhel", "centos", "rocky", "alma", "opensuse", "suse", "oracle",
    ];
    let pacman_ids = ["arch", "manjaro", "endeavouros", "garuda", "artix"];
    let apk_ids = ["alpine"];

    if deb_ids.iter().any(|d| id.contains(d) || like.contains(d)) {
        PackageFormat::Deb
    } else if rpm_ids.iter().any(|d| id.contains(d) || like.contains(d)) {
        PackageFormat::Rpm
    } else if pacman_ids
        .iter()
        .any(|d| id.contains(d) || like.contains(d))
    {
        PackageFormat::Pacman
    } else if apk_ids.iter().any(|d| id.contains(d) || like.contains(d)) {
        PackageFormat::Apk
    } else {
        PackageFormat::Other
    }
}
