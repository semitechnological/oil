//! winget-pkgs portable **zip** installs (InstallerType zip + NestedInstallerType portable).
//! Uses the public GitHub API / raw.githubusercontent.com — no winget.exe.

use crate::bottle::BottleDownloader;
use crate::error::{Result, WaxError};
use crate::scoop;
use crate::ui::dirs;
use crate::version;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use std::path::PathBuf;
use tempfile::TempDir;
use tracing::debug;

const WINGET_PKGS_REPO_CONTENTS: &str =
    "https://api.github.com/repos/microsoft/winget-pkgs/contents";
const WINGET_PKGS_RAW: &str = "https://raw.githubusercontent.com/microsoft/winget-pkgs/master";

#[derive(Debug, Deserialize)]
struct GhContentEntry {
    name: String,
    #[serde(rename = "type")]
    entry_type: String,
    path: String,
}

fn package_id_to_content_path(id: &str) -> Result<String> {
    let parts: Vec<&str> = id.split('.').filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        return Err(WaxError::InvalidInput(
            "winget PackageIdentifier needs at least two dot-separated segments (e.g. JesseDuffield.lazygit)"
                .into(),
        ));
    }
    let first = parts[0]
        .chars()
        .next()
        .ok_or_else(|| WaxError::InvalidInput("empty winget id".into()))?
        .to_ascii_lowercase();
    Ok(format!("manifests/{}/{}", first, parts.join("/")))
}

fn github_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent(concat!("wax/", env!("CARGO_PKG_VERSION"), " (winget-resolve)"))
        .build()
        .map_err(|e| WaxError::InstallError(e.to_string()))
}

async fn gh_get_json(url: &str) -> Result<Vec<GhContentEntry>> {
    let client = github_client()?;
    let mut req = client.get(url);
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {token}"));
        }
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(WaxError::InstallError(format!(
            "GitHub API {} -> HTTP {}",
            url,
            resp.status()
        )));
    }
    let v: serde_json::Value = resp.json().await?;
    if v.is_array() {
        Ok(serde_json::from_value(v).map_err(WaxError::JsonError)?)
    } else {
        Err(WaxError::InstallError(
            "Unexpected GitHub API response (expected directory listing)".into(),
        ))
    }
}

/// True if `microsoft/winget-pkgs` has a manifest directory for this PackageIdentifier.
#[cfg(target_os = "windows")]
pub async fn winget_package_exists(package_id: &str) -> bool {
    let Ok(path) = package_id_to_content_path(package_id) else {
        return false;
    };
    let url = format!("{WINGET_PKGS_REPO_CONTENTS}/{path}?ref=master");
    gh_get_json(&url).await.map(|v| !v.is_empty()).unwrap_or(false)
}

fn winget_arch_token() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x86",
        _ => "x64",
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WingetInstallerDoc {
    installer_type: Option<String>,
    nested_installer_type: Option<String>,
    nested_installer_files: Option<Vec<WingetNestedFile>>,
    installers: Vec<WingetInstallerEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WingetNestedFile {
    relative_file_path: String,
    portable_command_alias: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct WingetInstallerEntry {
    architecture: String,
    installer_url: String,
    installer_sha256: String,
}

fn pick_installer(doc: &WingetInstallerDoc) -> Result<&WingetInstallerEntry> {
    let want = winget_arch_token();
    doc.installers
        .iter()
        .find(|i| i.architecture.eq_ignore_ascii_case(want))
        .or_else(|| doc.installers.first())
        .ok_or_else(|| WaxError::InstallError("winget manifest has no installers".into()))
}

pub async fn install_portable_zip(package_id: &str) -> Result<()> {
    if !cfg!(target_os = "windows") {
        return Err(WaxError::PlatformNotSupported(
            "winget-style portable install is only supported on Windows".into(),
        ));
    }

    let rel = package_id_to_content_path(package_id)?;
    let list_url = format!("{WINGET_PKGS_REPO_CONTENTS}/{rel}?ref=master");
    let entries = gh_get_json(&list_url).await?;

    let mut versions: Vec<String> = entries
        .iter()
        .filter(|e| e.entry_type == "dir")
        .map(|e| e.name.clone())
        .collect();
    if versions.is_empty() {
        return Err(WaxError::FormulaNotFound(format!(
            "no version folders under winget-pkgs/{rel}"
        )));
    }
    version::sort_versions(&mut versions);
    let latest = versions
        .last()
        .ok_or_else(|| WaxError::InstallError("no winget versions".into()))?
        .clone();

    let ver_url = format!("{WINGET_PKGS_REPO_CONTENTS}/{rel}/{latest}?ref=master");
    let files = gh_get_json(&ver_url).await?;
    let installer_yaml = files
        .iter()
        .find(|e| e.name.ends_with(".installer.yaml") && e.entry_type == "file")
        .ok_or_else(|| {
            WaxError::InstallError(
                "No .installer.yaml in latest winget version (wax only supports installer manifests)"
                    .into(),
            )
        })?;

    let yaml_path = &installer_yaml.path;
    let raw_url = format!("{WINGET_PKGS_RAW}/{yaml_path}");
    debug!("Fetching winget installer yaml {}", raw_url);
    let yaml_text = github_client()?
        .get(&raw_url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let doc: WingetInstallerDoc =
        serde_yaml::from_str(&yaml_text).map_err(|e| WaxError::ParseError(e.to_string()))?;

    let inst_type = doc.installer_type.as_deref().unwrap_or("");
    let nested = doc.nested_installer_type.as_deref().unwrap_or("");
    if !inst_type.eq_ignore_ascii_case("zip") || !nested.eq_ignore_ascii_case("portable") {
        return Err(WaxError::InstallError(format!(
            "wax only supports winget zip+portable manifests (got InstallerType={inst_type}, NestedInstallerType={nested})"
        )));
    }

    let inst = pick_installer(&doc)?;
    let sha_expected = inst
        .installer_sha256
        .trim()
        .to_ascii_lowercase();

    let tmp = TempDir::new()?;
    let archive_path = tmp.path().join("winget.zip");

    let dl = BottleDownloader::new();
    let size = dl.probe_size(&inst.installer_url).await;
    let conns = BottleDownloader::num_connections(
        size,
        BottleDownloader::MAX_CONNECTIONS_PER_DOWNLOAD,
    );
    let pb = ProgressBar::new(0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.cyan} {msg} [{bar:30.cyan/blue}] {bytes}/{total_bytes}")
            .unwrap()
            .progress_chars("=>-"),
    );
    pb.set_message(format!("{} {}", package_id, latest));

    dl.download(&inst.installer_url, &archive_path, Some(&pb), conns)
        .await?;
    pb.finish_and_clear();

    BottleDownloader::verify_checksum(&archive_path, &sha_expected)?;

    let extract_root = tmp.path().join("extract");
    std::fs::create_dir_all(&extract_root)?;
    scoop::extract_zip_file(&archive_path, &extract_root)?;

    let bin_dir = dirs::home_dir()?
        .join(".local")
        .join("wax")
        .join("bin");
    std::fs::create_dir_all(&bin_dir)?;

    let nested_files = doc.nested_installer_files.as_ref().ok_or_else(|| {
        WaxError::InstallError("winget manifest missing NestedInstallerFiles".into())
    })?;

    for nf in nested_files {
        let rel = PathBuf::from(nf.relative_file_path.replace('\\', "/"));
        let src = extract_root.join(&rel);
        if !src.exists() {
            return Err(WaxError::InstallError(format!(
                "nested portable file missing: {}",
                src.display()
            )));
        }
        let dest_name = nf
            .portable_command_alias
            .as_ref()
            .map(|s| format!("{s}.exe"))
            .unwrap_or_else(|| {
                src.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "app.exe".into())
            });
        let dest = bin_dir.join(dest_name);
        if dest.exists() {
            let _ = std::fs::remove_file(&dest);
        }
        std::fs::copy(&src, &dest)?;
    }

    let staging = dirs::home_dir()?
        .join(".local")
        .join("wax")
        .join("winget-apps")
        .join(package_id.replace('.', "_"))
        .join(&latest);
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(staging.parent().unwrap())?;
    scoop::copy_dir_all(&extract_root, &staging)?;

    println!(
        "Installed {} {} (winget portable zip) — binaries under:\n  {}",
        package_id,
        latest,
        bin_dir.display()
    );

    Ok(())
}
