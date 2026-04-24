/// Nix-inspired system package management for wax.
///
/// Wax keeps its own declarative state and immutable generations while using
/// the host package manager as the execution backend. This keeps the UX
/// platform-neutral: the active backend may be Homebrew on macOS or a native
/// OS package manager on Linux, but wax-owned state remains the source of
/// truth for managed packages.
pub mod distro;
pub mod extractor;
pub mod generations;
pub mod installer;
pub mod manifest;
#[allow(dead_code)]
pub mod query;
pub mod registry;
pub mod resolver;
pub mod state;

use crate::error::{Result, WaxError};
use crate::system::distro::DistroInfo;
use crate::system::generations::{Generation, GenerationManager};
use crate::system::installer::SystemInstaller;
use crate::system::manifest::FileManifest;
use crate::system::registry::{PackageIndex, PackageMetadata};
use crate::system::resolver::Resolver;
use crate::system::state::SystemState;
use crate::system_pm::SystemPm;
use console::style;
use std::collections::{HashMap, HashSet};

pub struct SystemManager {
    pm: SystemPm,
    platform_label: String,
    gen_mgr: GenerationManager,
    installer: SystemInstaller,
}

impl SystemManager {
    pub async fn detect() -> Result<Option<Self>> {
        let Some(pm) = SystemPm::detect().await else {
            return Ok(None);
        };

        let platform_label = if cfg!(target_os = "macos") {
            "macOS".to_string()
        } else if let Some(distro) = DistroInfo::detect().await? {
            if distro.version.is_empty() {
                distro.name
            } else {
                format!("{} {}", distro.name, distro.version)
            }
        } else {
            std::env::consts::OS.to_string()
        };

        let gen_mgr = GenerationManager::new().await?;
        Ok(Some(Self {
            pm,
            platform_label,
            gen_mgr,
            installer: SystemInstaller::new(),
        }))
    }

    pub fn distro_label(&self) -> &str {
        &self.platform_label
    }

    pub async fn upgrade_all(&self) -> Result<()> {
        println!(
            "{} upgrading managed packages via {}",
            style("→").cyan(),
            style(self.pm.name()).bold()
        );

        self.snapshot("pre-upgrade").await?;
        self.pm.upgrade_all().await?;

        let mut state = SystemState::load().await?;
        self.refresh_tracked_state(&mut state).await?;
        state.save().await?;

        let gen = self
            .gen_mgr
            .create("upgrade", state.installed_packages())
            .await?;

        println!(
            "  {} generation {} created",
            style("✓").green(),
            style(gen.id).bold()
        );
        Ok(())
    }

    pub async fn install(&self, packages: &[String]) -> Result<()> {
        self.apply_install(packages, false).await
    }

    pub async fn add(&self, packages: &[String]) -> Result<()> {
        self.apply_install(packages, true).await
    }

    async fn apply_install(&self, packages: &[String], declare: bool) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        self.snapshot(&format!(
            "pre-{} {}",
            if declare { "add" } else { "install" },
            packages.join(" ")
        ))
        .await?;

        let mut state = SystemState::load().await?;
        if declare {
            for pkg in packages {
                state.declare(pkg);
            }
            state.save().await?;
        }

        self.pm.install(packages).await?;

        let live = self.live_packages().await?;
        let live_map: HashMap<String, Option<String>> = live.into_iter().collect();
        for pkg in packages {
            let version = live_map.get(pkg).cloned().unwrap_or(None);
            state.mark_installed(pkg, version, declare || state.is_declared(pkg));
        }
        self.refresh_tracked_state(&mut state).await?;
        state.save().await?;

        let gen = self
            .gen_mgr
            .create(
                &format!(
                    "{} {}",
                    if declare { "add" } else { "install" },
                    packages.join(" ")
                ),
                state.installed_packages(),
            )
            .await?;

        println!(
            "  {} generation {} created",
            style("✓").green(),
            style(gen.id).bold()
        );
        Ok(())
    }

    pub async fn remove(&self, packages: &[String]) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        self.snapshot(&format!("pre-remove {}", packages.join(" ")))
            .await?;
        self.remove_managed_packages(packages).await?;

        let mut state = SystemState::load().await?;
        for pkg in packages {
            state.undeclare(pkg);
            state.mark_removed(pkg);
        }
        self.refresh_tracked_state(&mut state).await?;
        state.save().await?;

        let gen = self
            .gen_mgr
            .create(
                &format!("remove {}", packages.join(" ")),
                state.installed_packages(),
            )
            .await?;

        println!(
            "  {} generation {} created",
            style("✓").green(),
            style(gen.id).bold()
        );
        Ok(())
    }

    pub async fn sync_declared(&self) -> Result<()> {
        let mut state = SystemState::load().await?;
        self.refresh_tracked_state(&mut state).await?;
        state.save().await?;

        if state.declared.is_empty() {
            println!("no declared system packages");
            return Ok(());
        }

        let live_set: HashSet<_> = state.installed.keys().map(|s| s.as_str()).collect();
        let declared_set: HashSet<_> = state.declared.iter().map(|s| s.as_str()).collect();

        let to_install: Vec<String> = declared_set
            .difference(&live_set)
            .map(|s| s.to_string())
            .collect();
        let to_remove: Vec<String> = live_set
            .difference(&declared_set)
            .map(|s| s.to_string())
            .collect();

        if to_install.is_empty() && to_remove.is_empty() {
            println!(
                "{} all declared system packages are installed",
                style("✓").green()
            );
            return Ok(());
        }

        if !to_remove.is_empty() {
            println!("removing {} undeclared managed packages:", to_remove.len());
            for pkg in &to_remove {
                println!("  {} {}", style("-").yellow(), style(pkg).magenta());
            }
            self.remove_managed_packages(&to_remove).await?;
            for pkg in &to_remove {
                state.mark_removed(pkg);
            }
            state.save().await?;
        }

        if to_install.is_empty() {
            state.save().await?;
            let gen = self
                .gen_mgr
                .create("sync", state.installed_packages())
                .await?;
            println!(
                "  {} generation {} created",
                style("✓").green(),
                style(gen.id).bold()
            );
            return Ok(());
        }

        println!("installing {} missing declared packages:", to_install.len());
        for pkg in &to_install {
            println!("  {} {}", style("+").green(), style(pkg).magenta());
        }

        self.apply_install(&to_install, true).await
    }

    pub async fn list_generations(&self) -> Result<Vec<Generation>> {
        self.gen_mgr.list().await
    }

    pub async fn current_generation(&self) -> Result<Option<Generation>> {
        self.gen_mgr.current().await
    }

    pub async fn rollback(&self, id: Option<u32>) -> Result<()> {
        let target_id = match id {
            Some(i) => i,
            None => self.gen_mgr.previous_id().await?.ok_or_else(|| {
                WaxError::InstallError("no previous generation to roll back to".into())
            })?,
        };

        let target =
            self.gen_mgr.get(target_id).await?.ok_or_else(|| {
                WaxError::InstallError(format!("generation {} not found", target_id))
            })?;

        let mut state = SystemState::load().await?;
        self.refresh_tracked_state(&mut state).await?;

        let current = state.installed_packages();
        let (to_install, to_remove) = GenerationManager::diff_records(&current, &target.packages);

        if to_install.is_empty() && to_remove.is_empty() {
            println!(
                "{} already at generation {}",
                style("✓").green(),
                style(target_id).bold()
            );
            return Ok(());
        }

        println!(
            "{} rolling back to generation {} ({})",
            style("→").cyan(),
            style(target_id).bold(),
            style(&target.reason).dim()
        );

        if !to_remove.is_empty() {
            let names: Vec<String> = to_remove.iter().map(|p| p.name.clone()).collect();
            println!("  removing: {}", names.join(", "));
            self.remove_managed_packages(&names).await?;
            for name in &names {
                state.mark_removed(name);
            }
        }

        if !to_install.is_empty() {
            let names: Vec<String> = to_install.iter().map(|p| p.name.clone()).collect();
            println!("  installing: {}", names.join(", "));
            self.pm.install(&names).await?;
            let declared_names: HashSet<String> = state.declared.iter().cloned().collect();
            for pkg in &to_install {
                state.mark_installed(
                    &pkg.name,
                    pkg.version.clone(),
                    declared_names.contains(&pkg.name),
                );
            }
        }

        self.refresh_tracked_state(&mut state).await?;
        state.save().await?;

        let new_gen = self
            .gen_mgr
            .create(
                &format!("rollback to gen-{}", target_id),
                state.installed_packages(),
            )
            .await?;

        println!(
            "{} rolled back — new generation {}",
            style("✓").green(),
            style(new_gen.id).bold()
        );
        Ok(())
    }

    /// Install packages directly from distro registry (bypass host PM).
    pub async fn install_direct(&self, packages: &[String], declare: bool) -> Result<()> {
        if packages.is_empty() {
            return Ok(());
        }

        let index = self.load_registry().await?;
        let resolver = Resolver::new(&index);
        let resolved = resolver.resolve(packages)?;

        if resolved.is_empty() {
            return Err(WaxError::InstallError(format!(
                "no packages found in registry for: {}",
                packages.join(", ")
            )));
        }

        let names: Vec<String> = resolved.iter().map(|p| p.name.clone()).collect();
        println!(
            "{} direct-installing {} package(s) via nix-like installer",
            style("→").cyan(),
            style(names.len()).bold()
        );
        for pkg in &resolved {
            println!("  {} {}", style("+").green(), style(&pkg.name).magenta());
        }

        self.snapshot(&format!(
            "pre-direct-{} {}",
            if declare { "add" } else { "install" },
            packages.join(" ")
        ))
        .await?;

        let prefix = SystemInstaller::install_prefix();
        let metadata: Vec<PackageMetadata> =
            resolved.iter().map(|&p| p.clone()).collect();
        let installed = self.installer.install_packages(&metadata, &prefix).await?;

        let mut state = SystemState::load().await?;
        if declare {
            for pkg in packages {
                state.declare(pkg);
            }
        }
        for (name, version) in installed {
            state.mark_installed(&name, Some(version), declare || state.is_declared(&name));
        }
        self.refresh_tracked_state(&mut state).await?;
        state.save().await?;

        let gen = self
            .gen_mgr
            .create(
                &format!(
                    "direct-{} {}",
                    if declare { "add" } else { "install" },
                    packages.join(" ")
                ),
                state.installed_packages(),
            )
            .await?;

        println!(
            "  {} generation {} created",
            style("✓").green(),
            style(gen.id).bold()
        );
        Ok(())
    }

    async fn load_registry(&self) -> Result<PackageIndex> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| WaxError::InstallError(format!("HTTP client: {}", e)))?;

        match self.pm {
            SystemPm::Apt => {
                let reg = crate::system::registry::apt::AptRegistry::ubuntu_default();
                reg.load(&client).await
            }
            SystemPm::Dnf | SystemPm::Yum => {
                let reg = crate::system::registry::dnf::DnfRegistry::fedora_default();
                reg.load(&client).await
            }
            SystemPm::Pacman => {
                let reg = crate::system::registry::pacman::PacmanRegistry::arch_default();
                reg.load(&client).await
            }
            SystemPm::Apk => {
                let reg = crate::system::registry::apk::ApkRegistry::alpine_default();
                reg.load(&client).await
            }
            _ => Err(WaxError::PlatformNotSupported(
                "direct install not supported for this package manager".into(),
            )),
        }
    }

    pub async fn status(&self) -> Result<()> {
        let mut state = SystemState::load().await?;
        self.refresh_tracked_state(&mut state).await?;
        state.save().await?;
        let current = self.gen_mgr.current().await?;

        println!(
            "{} {}",
            style("platform").bold(),
            style(self.distro_label()).cyan()
        );
        println!(
            "{} {}",
            style("pm      ").bold(),
            style(self.pm.name()).cyan()
        );

        if let Some(gen) = &current {
            println!(
                "{} gen-{} ({}, {})",
                style("gen     ").bold(),
                style(gen.id).bold(),
                style(&gen.reason).dim(),
                gen.age_string()
            );
        } else {
            println!("{} none", style("gen     ").bold());
        }

        println!(
            "{} {} declared, {} installed",
            style("pkgs    ").bold(),
            state.declared.len(),
            state.installed.len()
        );

        if !state.declared.is_empty() {
            println!();
            println!("{}:", style("declared").bold());
            let live: HashSet<_> = state.installed.keys().collect();
            for pkg in &state.declared {
                if live.contains(pkg) {
                    println!("  {} {}", style("✓").green(), style(pkg).magenta());
                } else {
                    println!(
                        "  {} {} {}",
                        style("✗").red(),
                        style(pkg).magenta(),
                        style("(not installed)").dim()
                    );
                }
            }
        }

        Ok(())
    }

    async fn live_packages(&self) -> Result<Vec<(String, Option<String>)>> {
        let mut packages = self.pm.list_installed().await?;
        for manifest in FileManifest::list_all().await? {
            if let Some(existing) = packages
                .iter_mut()
                .find(|(name, _)| *name == manifest.package)
            {
                existing.1 = Some(manifest.version.clone());
            } else {
                packages.push((manifest.package.clone(), Some(manifest.version.clone())));
            }
        }
        packages.sort_by(|a, b| a.0.cmp(&b.0));
        packages.dedup_by(|a, b| a.0 == b.0);
        Ok(packages)
    }

    async fn snapshot(&self, reason: &str) -> Result<()> {
        let mut state = SystemState::load().await?;
        self.refresh_tracked_state(&mut state).await?;
        state.save().await?;

        let packages = state.installed_packages();
        if !packages.is_empty() {
            self.gen_mgr.create(reason, packages).await?;
        }
        Ok(())
    }

    async fn refresh_tracked_state(&self, state: &mut SystemState) -> Result<()> {
        let live = self.live_packages().await?;
        let live_map: HashMap<String, Option<String>> = live.into_iter().collect();
        let installed_names: Vec<String> = state.installed.keys().cloned().collect();

        for name in installed_names {
            if let Some(version) = live_map.get(&name) {
                let declared = state.is_declared(&name);
                state.mark_installed(&name, version.clone(), declared);
            } else {
                state.mark_removed(&name);
            }
        }

        for pkg in &state.declared.clone() {
            if let Some(version) = live_map.get(pkg) {
                state.mark_installed(pkg, version.clone(), true);
            }
        }

        Ok(())
    }

    async fn remove_managed_packages(&self, packages: &[String]) -> Result<()> {
        let mut pm_packages = Vec::new();

        for package in packages {
            if let Some(manifest) = FileManifest::load_any_version(package).await? {
                for file in manifest.files.iter().rev() {
                    if file.exists() || file.symlink_metadata().is_ok() {
                        let _ = tokio::fs::remove_file(file).await;
                    }
                }

                let mut dirs = manifest.dirs.clone();
                dirs.sort_by_key(|b| std::cmp::Reverse(b.components().count()));
                for dir in &dirs {
                    let _ = tokio::fs::remove_dir(dir).await;
                }

                if let Ok(path) = FileManifest::manifest_path_pub(package, &manifest.version) {
                    let _ = tokio::fs::remove_file(path).await;
                }
            } else {
                pm_packages.push(package.clone());
            }
        }

        if !pm_packages.is_empty() {
            self.pm.remove(&pm_packages).await?;
        }

        Ok(())
    }
}
