//! Native OS package manager integration.
//!
//! Detects whichever system package manager is present and provides a unified
//! interface for install, upgrade, and listing operations.  This lets wax act
//! as a single entry point for both Homebrew-formula packages and OS-level
//! packages (apt, dnf, pacman, apk, zypper, emerge, yum, xbps-install, nix).

use crate::error::{Result, WaxError};
use crate::formula_parser::FormulaParser;
use console::style;
use sha2::{Digest, Sha256};
use tokio::process::Command;
use tracing::{debug, warn};

/// A detected system package manager.
#[derive(Debug, Clone, PartialEq)]
pub enum SystemPm {
    Brew,
    Apt,
    Dnf,
    Pacman,
    Apk,
    Zypper,
    Emerge,
    Yum,
    Xbps,
    Nix,
}

impl SystemPm {
    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Brew => "brew",
            Self::Apt => "apt",
            Self::Dnf => "dnf",
            Self::Pacman => "pacman",
            Self::Apk => "apk",
            Self::Zypper => "zypper",
            Self::Emerge => "emerge",
            Self::Yum => "yum",
            Self::Xbps => "xbps-install",
            Self::Nix => "nix-env",
        }
    }

    /// Detect the most appropriate system package manager on the current host.
    pub async fn detect() -> Option<Self> {
        if cfg!(target_os = "macos") {
            None
        } else {
            let candidates: &[(&str, Self)] = &[
                ("apt-get", Self::Apt),
                ("dnf", Self::Dnf),
                ("pacman", Self::Pacman),
                ("apk", Self::Apk),
                ("zypper", Self::Zypper),
                ("emerge", Self::Emerge),
                ("yum", Self::Yum),
                ("xbps-install", Self::Xbps),
                ("nix-env", Self::Nix),
            ];

            for (bin, pm) in candidates {
                if which(bin).await {
                    debug!("Detected system package manager: {}", bin);
                    return Some(pm.clone());
                }
            }
            None
        }
    }

    /// Upgrade all packages managed by this PM.
    /// Streams output directly to the terminal (many upgrade commands are
    /// interactive / produce a lot of output).
    pub async fn upgrade_all(&self) -> Result<()> {
        // For apt we need to do "update" then "upgrade" as two steps.
        match self {
            Self::Brew => {
                run_visible("brew", &["update"]).await?;
                run_visible("brew", &["upgrade"]).await?;
            }
            Self::Apt => {
                run_visible("sudo", &["apt-get", "update", "-q"]).await?;
                run_visible("sudo", &["apt-get", "upgrade", "-y"]).await?;
            }
            Self::Dnf => {
                run_visible("sudo", &["dnf", "upgrade", "--refresh", "-y"]).await?;
            }
            Self::Pacman => {
                run_visible("sudo", &["pacman", "-Syu", "--noconfirm"]).await?;
            }
            Self::Apk => {
                run_visible("sudo", &["apk", "upgrade"]).await?;
            }
            Self::Zypper => {
                run_visible("sudo", &["zypper", "refresh"]).await?;
                run_visible("sudo", &["zypper", "update", "-y"]).await?;
            }
            Self::Emerge => {
                run_visible("sudo", &["emerge", "--sync"]).await?;
                run_visible(
                    "sudo",
                    &["emerge", "--update", "--deep", "--newuse", "@world"],
                )
                .await?;
            }
            Self::Yum => {
                run_visible("sudo", &["yum", "update", "-y"]).await?;
            }
            Self::Xbps => {
                run_visible("sudo", &["xbps-install", "-Su"]).await?;
            }
            Self::Nix => {
                run_visible("nix-channel", &["--update"]).await?;
                run_visible("nix-env", &["-u", "*"]).await?;
            }
        }
        Ok(())
    }

    /// Install one or more packages via the system PM.
    pub async fn install(&self, packages: &[String]) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }
        let pkg_args: Vec<&str> = packages.iter().map(|s| s.as_str()).collect();

        match self {
            Self::Brew => {
                let mut args = vec!["install"];
                args.extend_from_slice(&pkg_args);
                run_visible("brew", &args).await?;
            }
            Self::Apt => {
                let mut args = vec!["apt-get", "install", "-y"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Dnf => {
                let mut args = vec!["dnf", "install", "-y"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Pacman => {
                let mut args = vec!["pacman", "-S", "--noconfirm"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Apk => {
                let mut args = vec!["apk", "add"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Zypper => {
                let mut args = vec!["zypper", "install", "-y"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Emerge => {
                let mut args: Vec<&str> = vec!["emerge"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Yum => {
                let mut args = vec!["yum", "install", "-y"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Xbps => {
                let mut args = vec!["xbps-install", "-S"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Nix => {
                let mut args = vec!["-i"];
                args.extend_from_slice(&pkg_args);
                run_visible("nix-env", &args).await?;
            }
        }
        Ok(())
    }

    /// List packages currently installed by this package manager.
    pub async fn list_installed(&self) -> Result<Vec<(String, Option<String>)>> {
        match self {
            Self::Brew => list_installed_with("brew", &["list", "--versions"]).await,
            Self::Apt => {
                list_installed_with("dpkg-query", &["-W", r#"-f=${Package}\t${Version}\n"#]).await
            }
            Self::Dnf | Self::Yum | Self::Zypper => {
                list_installed_with(
                    "rpm",
                    &["-qa", "--queryformat", "%{NAME}\t%{VERSION}-%{RELEASE}\n"],
                )
                .await
            }
            Self::Pacman => list_installed_with("pacman", &["-Q"]).await,
            Self::Apk => list_installed_with("apk", &["info", "-v"]).await,
            Self::Emerge => list_installed_with("qlist", &["-ICv"]).await,
            Self::Xbps => list_installed_with("xbps-query", &["-l"]).await,
            Self::Nix => list_installed_with("nix-env", &["-q"]).await,
        }
    }

    pub async fn remove(&self, packages: &[String]) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        let pkg_args: Vec<&str> = packages.iter().map(|s| s.as_str()).collect();

        match self {
            Self::Brew => {
                let mut args = vec!["uninstall"];
                args.extend_from_slice(&pkg_args);
                run_visible("brew", &args).await?;
            }
            Self::Apt => {
                let mut args = vec!["apt-get", "remove", "-y"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Dnf => {
                let mut args = vec!["dnf", "remove", "-y"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Pacman => {
                let mut args = vec!["pacman", "-R", "--noconfirm"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Apk => {
                let mut args = vec!["apk", "del"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Zypper => {
                let mut args = vec!["zypper", "remove", "-y"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Emerge => {
                let mut args = vec!["emerge", "--unmerge"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Yum => {
                let mut args = vec!["yum", "remove", "-y"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Xbps => {
                let mut args = vec!["xbps-remove", "-R"];
                args.extend_from_slice(&pkg_args);
                run_visible("sudo", &args).await?;
            }
            Self::Nix => {
                let mut args = vec!["-e"];
                args.extend_from_slice(&pkg_args);
                run_visible("nix-env", &args).await?;
            }
        }
        Ok(())
    }

    /// Install a cask (GUI app) on Linux.
    ///
    /// Strategy (in order):
    /// 0. Fetch the Homebrew cask `.rb` file and look for an `on_linux` block
    ///    containing a native download URL (.deb / .rpm / .AppImage).
    /// 1. Try snap (with and without `--classic`).
    /// 2. Try flatpak via Flathub.
    /// 3. Fall back to the native system package manager.
    pub async fn install_cask(&self, cask_name: &str) -> Result<()> {
        // 0. Try Homebrew cask .rb — uses the same metadata Homebrew itself uses.
        if let Ok(rb) = FormulaParser::fetch_cask_rb(cask_name).await {
            if let Some(artifact) = FormulaParser::parse_cask_linux_artifact(&rb) {
                debug!(
                    "Found on_linux artifact for {}: {}",
                    cask_name, artifact.url
                );
                match self
                    .install_linux_artifact(cask_name, &artifact.url, artifact.sha256.as_deref())
                    .await
                {
                    Ok(()) => return Ok(()),
                    Err(e) => {
                        warn!(
                            "Homebrew cask artifact install failed for {}: {}. \
                             Falling back to snap/flatpak/native PM — the package \
                             installed may differ from the macOS version.",
                            cask_name, e
                        );
                        eprintln!(
                            "  {} Homebrew .rb download failed ({}); trying snap/flatpak/native PM…",
                            style("!").yellow(),
                            e
                        );
                    }
                }
            }
        }

        // 1. Try snap — no extra repo setup needed on Ubuntu/Debian/derivatives.
        if which("snap").await {
            // Some snaps need --classic confinement; try both.
            for args in &[
                vec!["install", cask_name],
                vec!["install", "--classic", cask_name],
            ] {
                let ok = tokio::process::Command::new("snap")
                    .args(args.as_slice())
                    .status()
                    .await
                    .map(|s| s.success())
                    .unwrap_or(false);
                if ok {
                    return Ok(());
                }
            }
        }

        // 2. Try flatpak via Flathub — good GUI app coverage on Fedora/etc.
        if which("flatpak").await {
            // Ensure Flathub remote exists (harmless if already present).
            let _ = tokio::process::Command::new("flatpak")
                .args([
                    "remote-add",
                    "--if-not-exists",
                    "flathub",
                    "https://dl.flathub.org/repo/flathub.flatpakrepo",
                ])
                .output()
                .await;

            let ok = tokio::process::Command::new("flatpak")
                .args(["install", "-y", "flathub", cask_name])
                .status()
                .await
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                return Ok(());
            }
        }

        // 3. Fall back to native package manager (apt, dnf, pacman, etc.).
        self.install(&[cask_name.to_string()]).await
    }

    /// Download a Linux artifact from `url` and install it based on its extension.
    async fn install_linux_artifact(
        &self,
        name: &str,
        url: &str,
        sha256: Option<&str>,
    ) -> Result<()> {
        // Determine the artifact type from the URL extension (ignore query string).
        let ext = url
            .split('?')
            .next()
            .unwrap_or(url)
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_lowercase();

        println!(
            "  {} downloading {} ({})…",
            style("→").cyan(),
            style(name).magenta(),
            ext
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(|e| WaxError::InstallError(format!("HTTP client: {}", e)))?;

        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| WaxError::InstallError(format!("Download failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(WaxError::InstallError(format!(
                "HTTP {} downloading {}",
                response.status(),
                name
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| WaxError::InstallError(format!("Read response: {}", e)))?;

        // Verify checksum if provided.
        if let Some(expected) = sha256 {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let computed = format!("{:x}", hasher.finalize());
            if computed != expected {
                return Err(WaxError::InstallError(format!(
                    "{} checksum mismatch: expected {}, got {}",
                    name, expected, computed
                )));
            }
        }

        // Write to a secure temp file (unpredictable name, not world-accessible via /tmp race).
        let mut temp_file = tempfile::Builder::new()
            .suffix(&format!(".{}", ext))
            .tempfile()
            .map_err(|e| WaxError::InstallError(format!("Create temp file: {}", e)))?;
        use std::io::Write as _;
        temp_file
            .write_all(&bytes)
            .map_err(|e| WaxError::InstallError(format!("Write temp file: {}", e)))?;
        let temp_path = temp_file.path().to_path_buf();
        let temp_str = temp_path.to_string_lossy().into_owned();

        match ext.as_str() {
            "deb" => run_visible("sudo", &["dpkg", "-i", &temp_str]).await,
            "rpm" => run_visible("sudo", &["rpm", "-i", "--force", &temp_str]).await,
            "appimage" => {
                let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
                let bin_dir = format!("{home}/.local/bin");
                tokio::fs::create_dir_all(&bin_dir).await.ok();
                let dest = format!("{bin_dir}/{name}.AppImage");
                tokio::fs::copy(&temp_path, std::path::Path::new(&dest))
                    .await
                    .map_err(|e| WaxError::InstallError(format!("Copy AppImage: {}", e)))?;
                run_visible("chmod", &["+x", &dest]).await
            }
            _ => Err(WaxError::InstallError(format!(
                "Unsupported artifact extension '.{}' from Homebrew cask .rb",
                ext
            ))),
        }
        // temp_file drops here, deleting the temp file automatically
    }
}

/// Check if a binary exists on PATH.
async fn which(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a command, inheriting stdin/stdout/stderr so the user sees all output
/// and can interact (e.g. sudo password prompt).
async fn run_visible(program: &str, args: &[&str]) -> Result<()> {
    println!(
        "  {} {} {}",
        style("→").cyan(),
        style(program).dim(),
        args.join(" ")
    );

    let status = Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .await
        .map_err(|e| WaxError::InstallError(format!("Failed to run {}: {}", program, e)))?;

    if !status.success() {
        return Err(WaxError::InstallError(format!(
            "{} exited with status {}",
            program,
            status.code().unwrap_or(-1)
        )));
    }
    Ok(())
}

async fn list_installed_with(
    program: &str,
    args: &[&str],
) -> Result<Vec<(String, Option<String>)>> {
    let output = Command::new(program).args(args).output().await;
    let Ok(output) = output else {
        return Ok(Vec::new());
    };
    if !output.status.success() {
        return Ok(Vec::new());
    }

    let mut packages = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (name, version) = if program == "apk" {
            if let Some(idx) = line.rfind('-') {
                let name = &line[..idx];
                let version = &line[idx + 1..];
                if version
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
                {
                    (name.to_string(), Some(version.to_string()))
                } else {
                    (line.to_string(), None)
                }
            } else {
                (line.to_string(), None)
            }
        } else if program == "xbps-query" {
            let rest = line.strip_prefix("ii ").unwrap_or(line);
            if let Some((name, version)) = rest.rsplit_once('-') {
                (name.to_string(), Some(version.to_string()))
            } else {
                (rest.to_string(), None)
            }
        } else if program == "nix-env" {
            if let Some((name, version)) = line.rsplit_once('-') {
                (name.to_string(), Some(version.to_string()))
            } else {
                (line.to_string(), None)
            }
        } else if let Some((name, version)) = line.split_once('\t') {
            (name.trim().to_string(), Some(version.trim().to_string()))
        } else {
            let mut split = line.split_whitespace();
            let Some(name) = split.next() else {
                continue;
            };
            (name.to_string(), split.next().map(|s| s.to_string()))
        };

        if name.is_empty() {
            continue;
        }
        packages.push((name, version));
    }

    packages.sort_by(|a, b| a.0.cmp(&b.0));
    packages.dedup_by(|a, b| a.0 == b.0);
    Ok(packages)
}
