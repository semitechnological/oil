#[cfg(any(feature = "system-apk", feature = "system-all"))]
pub mod apk;
#[cfg(any(feature = "system-apt", feature = "system-opkg", feature = "system-all"))]
pub mod deb;
#[cfg(any(feature = "system-pacman", feature = "system-all"))]
pub mod pacman;
#[cfg(any(feature = "system-dnf", feature = "system-all"))]
pub mod rpm;
#[cfg(any(feature = "system-xbps", feature = "system-all"))]
pub mod xbps;
pub mod nar;

use crate::error::{OilError, Result};
use std::path::{Component, Path, PathBuf};

/// Relative archive member path that cannot escape `dest_dir`.
/// Rejects empty names, absolute paths, and any `..` component.
pub(crate) fn archive_rel_path(entry: &str) -> Option<&str> {
    let stripped = entry.strip_prefix("./").unwrap_or(entry);
    if stripped.is_empty() || stripped == "." {
        return None;
    }
    let path = Path::new(stripped);
    if path.is_absolute() {
        return None;
    }
    if path.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(stripped)
}

/// Extract a package and return (files, dirs) — absolute paths of everything extracted.
/// `dest_dir` is the install root.
pub fn extract_package_tracked(
    path: &Path,
    dest_dir: &Path,
) -> Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let name = path.to_string_lossy();
    #[cfg(any(feature = "system-apt", feature = "system-opkg", feature = "system-all"))]
    if name.ends_with(".deb") || name.ends_with(".ipk") {
        return deb::extract_tracked(path, dest_dir);
    }
    #[cfg(any(feature = "system-pacman", feature = "system-all"))]
    if name.ends_with(".pkg.tar.zst") || name.ends_with(".pkg.tar.xz") || name.ends_with(".pkg.tar.gz") {
        return pacman::extract_tracked(path, dest_dir);
    }
    #[cfg(any(feature = "system-apk", feature = "system-all"))]
    if name.ends_with(".apk") {
        return apk::extract_tracked(path, dest_dir);
    }
    #[cfg(any(feature = "system-dnf", feature = "system-all"))]
    if name.ends_with(".rpm") {
        return rpm::extract_tracked(path, dest_dir);
    }
    #[cfg(any(feature = "system-xbps", feature = "system-all"))]
    if name.ends_with(".xbps") {
        return xbps::extract_tracked(path, dest_dir);
    }
    if name.ends_with(".nar") || name.ends_with(".nar.zst") {
        return nar::extract_nar_tracked(path, dest_dir);
    }
    Err(OilError::InstallError(format!(
        "unknown package format: {}",
        name
    )))
}

#[cfg(test)]
mod tests {
    use super::archive_rel_path;

    #[test]
    fn archive_rel_path_accepts_normal_members() {
        assert_eq!(archive_rel_path("usr/bin/rg"), Some("usr/bin/rg"));
        assert_eq!(archive_rel_path("./usr/bin/rg"), Some("usr/bin/rg"));
        assert_eq!(archive_rel_path("foo..bar"), Some("foo..bar"));
    }

    #[test]
    fn archive_rel_path_rejects_parent_and_absolute() {
        assert_eq!(archive_rel_path(""), None);
        assert_eq!(archive_rel_path("."), None);
        assert_eq!(archive_rel_path("./"), None);
        assert_eq!(archive_rel_path("../etc/passwd"), None);
        assert_eq!(archive_rel_path("usr/../etc/passwd"), None);
        assert_eq!(archive_rel_path("/etc/passwd"), None);
    }
}
