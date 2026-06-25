# Oil — Universal Linux Package Manager

Oil is a native Linux package manager that works on **any distribution**.
Like Nix, it installs packages to its own prefix and doesn't conflict
with the host package manager. Unlike Nix, Oil is minimal, fast, and
focused on generation-based rollback.

## Design

- **Works everywhere** — no distro-specific bootstrap. One binary.
- **Own prefix** — installs to `/usr/local/oil` or `~/.local/oil`. No /usr pollution.
- **Generation rollback** — every install creates a snapshot. Rollback with `oil rollback`.
- **OS-gated** — compile with `--no-default-features` to strip unused system registries.
- **System registries** — built-in parsers for APT, DNF, Pacman, APK, XBPS, Nix, and Linuxbrew repos.
  These are *read-only* data sources for resolving packages. Oil never delegates to host package managers.
  Installed files and Oil-owned state remain the source of truth.

## Quick start

```sh
# Install oil (latest release)
curl -fsSL https://github.com/semitechnological/oil/releases/latest/download/oil-linux-x64 -o ~/.local/bin/oil
chmod +x ~/.local/bin/oil

# Or build from source
cargo install --git https://github.com/semitechnological/oil

# Bootstrap packages from your distro's repos (read-only)
oil search ripgrep
oil install ripgrep
```

## Build

```sh
cargo build
cargo build --no-default-features --features "system-xbps,system-apk"
```

## Install script

A convenience installer is included:

```sh
# From a clone:
./install.sh

# Or download the latest release binary:
curl -fsSL https://raw.githubusercontent.com/semitechnological/oil/master/install.sh | bash
```

## System registries

Oil resolves packages through existing distribution ecosystems (feature-gated):

| Feature | Registry | Extractor | Systems |
|---------|----------|-----------|---------|
| `system-apt` | Debian/Ubuntu repos | `.deb` | Debian, Ubuntu, Mint, Pop, Kali, Parrot |
| `system-dnf` | Fedora/RHEL repos | `.rpm` | Fedora, RHEL, CentOS, Rocky, Alma |
| `system-zypper` | openSUSE repos (via RPM metadata) | `.rpm` | openSUSE (Tumbleweed/Leap), SLES |
| `system-pacman` | Arch repos | `.pkg.tar.zst` | Arch, Manjaro, EndeavourOS, Garuda, Artix |
| `system-apk` | Alpine/Chimera repos | `.apk` | Alpine, Chimera Linux |
| `system-xbps` | Void repos | `.xbps` | Void Linux |
| `system-nix` | Nixpkgs | Nix store | NixOS |
| `system-opkg` | OpenWrt Packages.gz | `.ipk` (via deb extractor) | OpenWrt, LEDE |
| `system-eopkg` | Solus eopkg-index.xml | `.eopkg` | Solus |
| `system-brew` | Linuxbrew | Bottles | macOS, Linux |
| `system-flatpak` | — (detect/search/list only) | daemon-based | Cross-distro |
| `system-snap` | — (detect/search/list only) | daemon-based | Cross-distro |
| `system-emerge` | — (detect/list only) | source-based | Gentoo |
| `system-freebsd` | — (detect/list only) | CLI passthrough | FreeBSD |
| `system-guix` | — (detect/list only) | CLI passthrough | Guix System |
| `system-pkgadd` | — (detect/list only) | CLI passthrough | OpenBSD |
| `system-swupd` | — (detect/list only) | CLI passthrough | Clear Linux |

Default builds include all registries (`system-all`).
Compile with `--no-default-features --features "system-xbps,system-apk"` to select only what you need.
