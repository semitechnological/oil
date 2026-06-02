use crate::error::{Result, WaxError};
use crate::system::registry::{parse_dep_name, PackageIndex, PackageMetadata};
use std::collections::HashSet;
use tracing::warn;

pub struct Resolver<'a> {
    index: &'a PackageIndex,
}

impl<'a> Resolver<'a> {
    pub fn new(index: &'a PackageIndex) -> Self {
        Self { index }
    }

    /// Resolve the full install closure for the requested packages.
    /// Returns packages in topological order (dependencies before dependents).
    #[cfg(test)]
    pub fn resolve(&self, packages: &[String]) -> Result<Vec<&'a PackageMetadata>> {
        self.resolve_with_satisfied(packages, |_| false)
    }

    pub fn resolve_with_satisfied<F>(
        &self,
        packages: &[String],
        dep_satisfied: F,
    ) -> Result<Vec<&'a PackageMetadata>>
    where
        F: Fn(&str) -> bool,
    {
        let mut visited: HashSet<String> = HashSet::new();
        let mut pushed: HashSet<String> = HashSet::new();
        let mut result: Vec<&'a PackageMetadata> = Vec::new();
        let mut missing_requested = Vec::new();

        for pkg in packages {
            let name = parse_dep_name(pkg).to_string();
            if self
                .visit(
                    &name,
                    true,
                    &dep_satisfied,
                    &mut visited,
                    &mut pushed,
                    &mut result,
                )
                .is_none()
            {
                missing_requested.push(pkg.clone());
            }
        }

        if !missing_requested.is_empty() {
            return Err(WaxError::InstallError(format!(
                "package{} not found in system registry: {}",
                if missing_requested.len() == 1 {
                    ""
                } else {
                    "s"
                },
                missing_requested.join(", ")
            )));
        }

        Ok(result)
    }

    fn visit<F>(
        &self,
        name: &str,
        requested: bool,
        dep_satisfied: &F,
        visited: &mut HashSet<String>,
        pushed: &mut HashSet<String>,
        result: &mut Vec<&'a PackageMetadata>,
    ) -> Option<&'a PackageMetadata>
    where
        F: Fn(&str) -> bool,
    {
        if !visited.insert(name.to_string()) {
            return self.index.find(name);
        }

        if !requested && dep_satisfied(name) {
            return None;
        }

        let meta = match self.index.find(name) {
            Some(m) => m,
            None => {
                if !requested {
                    warn!("Dependency not found in index (skipping): {}", name);
                }
                return None;
            }
        };

        // Mark the concrete package name as visited too. Dependencies often
        // resolve via a virtual provide (for example an RPM soname), and this
        // prevents the same package being emitted again if it later appears by
        // its real package name.
        visited.insert(meta.name.clone());

        // Visit all deps recursively (DFS post-order ensures deps come first)
        for dep_raw in &meta.depends {
            let dep_name = parse_dep_name(dep_raw);
            if dep_name.is_empty() {
                continue;
            }
            self.visit(dep_name, false, dep_satisfied, visited, pushed, result);
        }

        // Push this package after all its deps.
        if pushed.insert(meta.name.clone()) {
            result.push(meta);
        }
        Some(meta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::registry::{PackageIndex, PackageMetadata};

    fn make_pkg(name: &str, version: &str, depends: &[&str]) -> PackageMetadata {
        PackageMetadata {
            name: name.to_string(),
            version: version.to_string(),
            description: "".to_string(),
            download_url: "".to_string(),
            sha256: None,
            installed_size: 0,
            depends: depends.iter().map(|s| s.to_string()).collect(),
            provides: vec![],
        }
    }

    #[test]
    fn test_resolve_no_deps() {
        let index = PackageIndex {
            packages: vec![make_pkg("curl", "8.0.0", &[])],
        };
        let resolver = Resolver::new(&index);
        let result = resolver.resolve(&["curl".to_string()]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "curl");
    }

    #[test]
    fn test_resolve_with_deps() {
        let index = PackageIndex {
            packages: vec![
                make_pkg("curl", "8.0.0", &["libc6", "libssl3"]),
                make_pkg("libc6", "2.35", &[]),
                make_pkg("libssl3", "3.0.0", &["libc6"]),
            ],
        };
        let resolver = Resolver::new(&index);
        let result = resolver.resolve(&["curl".to_string()]).unwrap();
        let names: Vec<_> = result.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"curl"));
        assert!(names.contains(&"libc6"));
        assert!(names.contains(&"libssl3"));
        let libc_pos = names.iter().position(|&n| n == "libc6").unwrap();
        let curl_pos = names.iter().position(|&n| n == "curl").unwrap();
        assert!(libc_pos < curl_pos);
    }

    #[test]
    fn test_resolve_missing_dep_skipped() {
        let index = PackageIndex {
            packages: vec![
                make_pkg("nginx", "1.24.0", &["libpcre3", "missing-virtual-pkg"]),
                make_pkg("libpcre3", "8.45", &[]),
            ],
        };
        let resolver = Resolver::new(&index);
        let result = resolver.resolve(&["nginx".to_string()]).unwrap();
        let names: Vec<_> = result.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"nginx"));
        assert!(names.contains(&"libpcre3"));
    }

    #[test]
    fn test_resolve_missing_requested_fails() {
        let index = PackageIndex {
            packages: vec![make_pkg("curl", "8.0.0", &[])],
        };
        let resolver = Resolver::new(&index);
        let err = resolver.resolve(&["ripgrep".to_string()]).unwrap_err();
        assert!(err.to_string().contains("ripgrep"));
    }

    #[test]
    fn test_resolve_virtual_provide_deduplicates_concrete_package() {
        let mut glibc = make_pkg("glibc", "2.39", &[]);
        glibc.provides = vec!["libc.so.6()(64bit)".to_string()];
        let index = PackageIndex {
            packages: vec![
                make_pkg("ripgrep", "14.1.1", &["libc.so.6()(64bit)"]),
                glibc,
            ],
        };
        let resolver = Resolver::new(&index);
        let result = resolver
            .resolve(&["ripgrep".to_string(), "glibc".to_string()])
            .unwrap();
        let names: Vec<_> = result.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names.iter().filter(|&&name| name == "glibc").count(), 1);
        assert!(names.contains(&"ripgrep"));
    }

    #[test]
    fn test_resolve_skips_host_satisfied_dependency() {
        let mut glibc = make_pkg("glibc", "2.39", &[]);
        glibc.provides = vec!["libc.so.6()(64bit)".to_string()];
        let index = PackageIndex {
            packages: vec![
                make_pkg("ripgrep", "14.1.1", &["libc.so.6()(64bit)"]),
                glibc,
            ],
        };
        let resolver = Resolver::new(&index);
        let result = resolver
            .resolve_with_satisfied(&["ripgrep".to_string()], |dep| dep == "libc.so.6()(64bit)")
            .unwrap();
        let names: Vec<_> = result.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["ripgrep"]);
    }

    #[test]
    fn test_resolve_deduplicates() {
        let index = PackageIndex {
            packages: vec![
                make_pkg("curl", "8.0.0", &["libc6"]),
                make_pkg("wget", "1.21.0", &["libc6"]),
                make_pkg("libc6", "2.35", &[]),
            ],
        };
        let resolver = Resolver::new(&index);
        let result = resolver
            .resolve(&["curl".to_string(), "wget".to_string()])
            .unwrap();
        let libc_count = result.iter().filter(|p| p.name == "libc6").count();
        assert_eq!(libc_count, 1);
    }
}
