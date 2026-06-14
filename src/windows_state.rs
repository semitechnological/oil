use crate::error::{Result, WaxError};
use crate::package_spec::Ecosystem;
use crate::ui::dirs;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsPackageManifest {
    pub ecosystem: Ecosystem,
    pub id: String,
    pub version: String,
    pub source: String,
    pub staging_dir: PathBuf,
    pub bin_links: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
    pub installed_at: i64,
}

pub fn wax_windows_root() -> Result<PathBuf> {
    Ok(dirs::home_dir()?.join(".local").join("wax"))
}

pub fn wax_bin_dir() -> Result<PathBuf> {
    Ok(wax_windows_root()?.join("bin"))
}

fn manifest_dir() -> Result<PathBuf> {
    Ok(wax_windows_root()?.join("windows").join("manifests"))
}

fn manifest_path(ecosystem: Ecosystem, id: &str) -> Result<PathBuf> {
    let safe_id: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect();
    Ok(manifest_dir()?.join(format!("{}-{}.json", ecosystem.label(), safe_id)))
}

pub fn collect_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_inner(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_inner(path: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_files_inner(&path, files)?;
        } else if file_type.is_file() || file_type.is_symlink() {
            files.push(path);
        }
    }
    Ok(())
}

impl WindowsPackageManifest {
    pub fn new(
        ecosystem: Ecosystem,
        id: impl Into<String>,
        version: impl Into<String>,
        source: impl Into<String>,
        staging_dir: PathBuf,
        bin_links: Vec<PathBuf>,
        files: Vec<PathBuf>,
    ) -> Self {
        Self {
            ecosystem,
            id: id.into(),
            version: version.into(),
            source: source.into(),
            staging_dir,
            bin_links,
            files,
            installed_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = manifest_path(self.ecosystem, &self.id)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(WaxError::JsonError)?;
        std::fs::write(path, raw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_files_recurses_and_sorts() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("bin")).unwrap();
        std::fs::write(root.join("bin/tool.exe"), b"tool").unwrap();
        std::fs::write(root.join("readme.txt"), b"readme").unwrap();

        let files = collect_files(root).unwrap();
        let names: Vec<_> = files
            .iter()
            .map(|path| {
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert_eq!(names, vec!["bin/tool.exe", "readme.txt"]);
    }
}
