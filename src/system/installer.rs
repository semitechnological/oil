use crate::bottle::BottleDownloader;
use crate::error::{Result, WaxError};
use crate::system::extractor::extract_package_tracked;
use crate::system::manifest::FileManifest;
use crate::system::registry::PackageMetadata;
use crate::system::scripts::run_post_install_script;
use crate::ui::{ProgressBarGuard, PROGRESS_BAR_CHARS, PROGRESS_BAR_TEMPLATE};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use sha2::Digest;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::Semaphore;
use tracing::debug;

type PackageManifestData = (usize, String, String, PathBuf, Vec<PathBuf>, Vec<PathBuf>);

pub struct SystemInstaller {
    downloader: Arc<BottleDownloader>,
}

impl SystemInstaller {
    pub fn new() -> Self {
        Self {
            downloader: Arc::new(BottleDownloader::new()),
        }
    }

    /// Download and install a set of packages (already dependency-resolved).
    /// Uses parallel downloads like the bottle installer.
    /// Returns (name, version) pairs for successfully installed packages.
    pub async fn install_packages(
        &self,
        packages: &[PackageMetadata],
        prefix: &Path,
        run_scripts: bool,
    ) -> Result<Vec<(String, String)>> {
        if packages.is_empty() {
            return Ok(vec![]);
        }

        std::fs::create_dir_all(prefix)?;

        let mp = MultiProgress::new();
        let semaphore = Arc::new(Semaphore::new(BottleDownloader::GLOBAL_CONNECTION_POOL));

        // Probe sizes first so we can allocate connections proportionally
        let sizes: Vec<u64> = {
            let mut futs = Vec::new();
            for pkg in packages {
                let dl = Arc::clone(&self.downloader);
                let url = pkg.download_url.clone();
                futs.push(async move { dl.probe_size(&url).await });
            }
            futures::future::join_all(futs).await
        };

        let total_size: u64 = sizes.iter().sum();
        let pool = BottleDownloader::GLOBAL_CONNECTION_POOL;

        let tmp_dir = TempDir::new()?;
        let mut tasks = Vec::new();

        for (index, (pkg, &size)) in packages.iter().zip(sizes.iter()).enumerate() {
            let max_conns = if total_size == 0 {
                1
            } else {
                ((size as f64 / total_size as f64) * pool as f64)
                    .round()
                    .max(1.0) as usize
            };

            let pkg_name = pkg.name.clone();
            let pkg_version = pkg.version.clone();
            let url = pkg.download_url.clone();
            let sha256 = pkg.sha256.clone();

            // Derive filename from URL
            let filename = url
                .split('/')
                .next_back()
                .unwrap_or("package.bin")
                .to_string();
            let dest = tmp_dir.path().join(&filename);

            let pb = mp.add(ProgressBar::new(size.max(1)));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(PROGRESS_BAR_TEMPLATE)
                    .unwrap()
                    .progress_chars(PROGRESS_BAR_CHARS),
            );
            pb.set_message(format!("{} {}", pkg_name, pkg_version));

            let dl = Arc::clone(&self.downloader);
            let sem = Arc::clone(&semaphore);
            let prefix_buf = prefix.to_path_buf();
            let pb_clone = pb.clone();

            tasks.push(tokio::spawn(async move {
                let _permit = sem
                    .acquire_many(max_conns as u32)
                    .await
                    .map_err(|e| WaxError::InstallError(format!("Semaphore error: {}", e)))?;

                debug!("Downloading {} from {}", pkg_name, url);
                let mut clear_guard = ProgressBarGuard::new(&pb_clone);
                dl.download(&url, &dest, Some(&pb_clone), max_conns, None)
                    .await?;
                clear_guard.clear_now();

                // Verify SHA256 if available
                if let Some(ref expected) = sha256 {
                    let mut file = std::fs::File::open(&dest)?;
                    let mut hasher = sha2::Sha256::new();
                    let mut buf = [0u8; 8192];
                    loop {
                        let n = file.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        hasher.update(&buf[..n]);
                    }
                    let actual = format!("{:x}", hasher.finalize());
                    if actual != *expected {
                        return Err(WaxError::ChecksumMismatch {
                            expected: expected.clone(),
                            actual,
                        });
                    }
                }

                // Extract package and track files.
                let (files, dirs) = extract_package_tracked(&dest, &prefix_buf)?;
                debug!("Extracted {} to {:?}", pkg_name, prefix_buf);

                Ok::<PackageManifestData, WaxError>((
                    index,
                    pkg_name,
                    pkg_version,
                    dest,
                    files,
                    dirs,
                ))
            }));
        }

        let mut installed = Vec::new();
        let mut manifest_data: Vec<PackageManifestData> = Vec::new();

        let mut failures = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok((index, name, version, package_path, files, dirs))) => {
                    manifest_data.push((
                        index,
                        name.clone(),
                        version.clone(),
                        package_path,
                        files,
                        dirs,
                    ));
                    installed.push((name, version));
                }
                Ok(Err(e)) => failures.push(e.to_string()),
                Err(e) => failures.push(format!("package task failed to join: {}", e)),
            }
        }

        if !failures.is_empty() {
            return Err(WaxError::InstallError(format!(
                "failed to install {} of {} packages:\n{}",
                failures.len(),
                packages.len(),
                failures
                    .into_iter()
                    .map(|failure| format!("  - {failure}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
        }

        manifest_data.sort_by_key(|(index, _, _, _, _, _)| *index);

        if run_scripts {
            for (_, name, _, package_path, _, _) in &manifest_data {
                run_post_install_script(package_path, prefix).map_err(|e| {
                    WaxError::InstallError(format!(
                        "post-install script for {} failed: {}",
                        name, e
                    ))
                })?;
            }
        }

        // Save manifests for each successfully installed package. A missing
        // manifest would make wax think the install never happened, so surface
        // this as an install failure rather than silently losing state.
        for (_, name, version, _, files, dirs) in manifest_data {
            let manifest = FileManifest {
                package: name,
                version,
                files,
                dirs,
                installed_at: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            };
            manifest.save().await.map_err(|e| {
                WaxError::InstallError(format!(
                    "failed to save file manifest for {}: {}",
                    manifest.package, e
                ))
            })?;
        }

        if installed.len() != packages.len() {
            return Err(WaxError::InstallError(format!(
                "installed {} of {} resolved packages",
                installed.len(),
                packages.len()
            )));
        }

        Ok(installed)
    }

    /// Determine the install prefix based on whether we have root.
    pub fn install_prefix() -> std::path::PathBuf {
        // If running as root, install to system root
        #[cfg(unix)]
        if unsafe { libc::getuid() } == 0 {
            return std::path::PathBuf::from("/");
        }

        // Non-root: install to ~/.local
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(".local");
        }

        // Last resort: ~/.wax/system/root
        crate::ui::dirs::wax_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
            .join("system")
            .join("root")
    }
}
