# Oil — Universal Linux Package Manager

Oil is a native Linux package manager that works on **any distribution**.
Like Nix, it installs packages to its own prefix and doesn't conflict
with the host package manager. Unlike Nix, Oil is minimal, fast, and
focused on static binaries with generation-based rollback.

## Design

- **Works everywhere** — no distro-specific bootstrap. One binary, one registry.
- **Own prefix** — installs to `/usr/local/oil` or `~/.local/oil`. No /usr pollution.
- **Static packages** — prefers statically linked binaries.
- **Generation rollback** — every install creates a snapshot. Rollback with `oil rollback`.
- **OS-gated** — compile with `--no-default-features` to strip unused registry code.
- **Optional registries** — built-in parsers for APT, DNF, Pacman, APK, XBPS repos.
  These are *read-only* data sources. Oil never delegates to host PMs.

## Quick start

```sh
# Install oil
curl -fsSL https://oil.sh/install.sh | sh

# Install packages from Oil's native registry
oil install clang lld bearssl toybox oksh

# Or bootstrap from your distro's repo (read-only)
oil search ripgrep
oil install ripgrep   # fetches from native registry, not apt/pacman
```

## Native registry

Oil's primary package source is its own registry. The default URL is
`https://packages.alpenglow.sh/oil/index.json` but you can override with
`OIL_NATIVE_INDEX`.

The index is a JSON file listing package metadata. See `sample-index.json`.

## Build

```sh
cargo build
cargo build --no-default-features --features "system-xbps,system-apk"
```

## Registry features

Oil can read packages from existing ecosystems (feature-gated):

| Feature | Registry | Extractor |
|---------|----------|-----------|
| `system-apt` | Debian/Ubuntu repos | `.deb` |
| `system-dnf` | Fedora/RHEL repos | `.rpm` |
| `system-pacman` | Arch repos | `.pkg.tar.zst` |
| `system-apk` | Alpine/Chimera repos | `.apk` |
| `system-xbps` | Void repos | `.xbps` |
| `system-nix` | Nixpkgs | Nix store |
| (always on) | — | Oil-managed prefix |
