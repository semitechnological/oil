# Linux Support

Wax supports two Linux package flows:

1. **Homebrew/Linuxbrew formulae** — the traditional Wax formula/bottle path.
2. **Wax-managed system packages** — `wax install`, `wax search`, and `wax system ...` can use Linux distribution registries and package archives directly.

Wax-managed system packages are **Nix-like in UX/state only**. They are not a Nix replacement.

## What “Wax-managed system packages” means

Wax can:

- detect a supported Linux package ecosystem
- search distribution package metadata directly
- resolve packages and dependencies from registry metadata
- download distro package archives (`.rpm`, `.deb`, `.pkg.tar.*`, `.apk`)
- extract files into a Wax/user prefix when not root
- write manifests for tracked removal
- run package post-install scripts by default where Wax can extract them (`.deb` `postinst`, RPM `%post`)
- keep Wax-owned state and generations

Wax system packages must **not** hand installation/removal off to another package manager. The system package path does not run `apt install`, `dnf install`, `pacman -S`, `apk add`, `rpm -i`, `dpkg -i`, or equivalent host-PM mutating commands.

By default, Wax runs post-install scripts it can extract from package archives. Use `wax install --no-script ...` to skip automatic post-install scripts.

Wax does **not** provide:

- Nix derivations
- hermetic builds or isolated stores
- reproducible build graphs
- full package-manager transaction triggers
- systemd/user/group/kernel-module integration
- guaranteed relocation for packages that assume `/usr`, `/etc`, `/var`, or root-owned system paths

Host-provided dependencies may be treated as satisfied. Wax may use read-only inventory/capability queries such as `rpm -q --whatprovides` to avoid unpacking base OS packages, but those queries are not install/remove handoffs. This keeps CLI tools usable without unpacking base OS packages, but it also means installs are not hermetic.

## Support matrix

| Ecosystem | Registry backend | Archive extractor | Runtime default selection | Smoke tested | Status |
| --- | --- | --- | --- | --- | --- |
| Fedora / Ultramarine / DNF/Yum RPM | Yes | Yes | `/etc/os-release` `VERSION_ID` + `uname -m` | Yes: Ultramarine Linux 43, `ripgrep` install/remove | Supported preview |
| Ubuntu / Debian APT | Yes | Yes | `/etc/os-release` `VERSION_CODENAME` / `UBUNTU_CODENAME` | Yes: Debian Bookworm container, `ripgrep` install/remove | Supported preview |
| Arch / Pacman | Yes | Yes | rolling Arch mirror + runtime arch | Yes: Arch container, `ripgrep` install/remove | Supported preview |
| Alpine / APK | Yes | Yes | `/etc/os-release` `VERSION_ID` major/minor | Yes: Alpine 3.24 container, `ripgrep` install/remove | Supported preview |
| macOS | Separate Homebrew flow | Separate Homebrew flow | Homebrew prefix/platform detection | Existing Wax flow | Supported separately |
| Windows | Separate Windows package-manager investigation | N/A for Linux system path | N/A | Not part of Linux system path | Separate work |

## Verified Fedora/Ultramarine behavior

The Fedora/DNF path has been smoke-tested on Ultramarine Linux 43 with a temporary `HOME`:

```bash
wax search ripgrep
HOME=/tmp/wax-smoke-home wax install ripgrep
/tmp/wax-smoke-home/.local/usr/bin/rg --version
HOME=/tmp/wax-smoke-home wax system status
HOME=/tmp/wax-smoke-home wax system upgrade
HOME=/tmp/wax-smoke-home wax system remove ripgrep
```

Observed behavior:

- `wax search ripgrep` returns a registry result.
- `wax install ripgrep` installs only `ripgrep` when host RPM capabilities already satisfy base dependencies.
- The `rg` binary works from the Wax prefix.
- The manifest records extracted files.
- `wax system upgrade` compares Wax-managed package versions with registry metadata and reinstalls outdated packages through Wax.
- `wax system remove ripgrep` removes the tracked files and updates status.

## Verified container behavior

APT, Pacman, and APK were smoke-tested in distro containers using the same `ripgrep` flow:

```bash
wax system search ripgrep
HOME=/tmp/wax-smoke-home wax system install ripgrep --no-script
/tmp/wax-smoke-home/.local/usr/bin/rg --version
HOME=/tmp/wax-smoke-home wax system status
HOME=/tmp/wax-smoke-home wax system upgrade
HOME=/tmp/wax-smoke-home wax system remove ripgrep
```

Observed behavior:

- registry search returns distro package metadata
- package archives and dependencies are downloaded by Wax
- archive extraction writes tracked manifests
- extracted commands run from the Wax prefix
- upgrade and remove operate on Wax-managed state

## Command behavior

On Linux, plain package commands prefer Wax’s system registry path when no formula/cask/source modifiers are requested:

```bash
wax search ripgrep
wax install ripgrep
wax system status
wax system upgrade
wax system remove ripgrep
```

Use explicit ecosystem/package qualifiers when you want the non-system formula/ecosystem path.

## Known limitations

- Post-install scripts run under Wax with `WAX_INSTALL_PREFIX`/`WAX_ROOT` set, but scripts that require a full host package-manager transaction, system users/groups, services, triggers, or kernel integration may still fail.
- Packages with hardcoded absolute paths may extract but fail at runtime.
- Shared libraries already present on the host are generally not copied into the Wax prefix.
- System packages install to `~/.local` by default, even as root. Set `WAX_SYSTEM_PREFIX=/` only when you explicitly want root-owned system paths.
- RPM registry installs currently require Fedora-compatible repository metadata. Other RPM families need repo-file parsing before Wax should select their package archives.
- Distribution metadata formats and mirrors change; registry parsing should be kept covered by tests.

## Validation guidance

For a new distro/backend, validate at least:

```bash
cargo check
cargo test system::
wax search ripgrep
HOME=/tmp/wax-smoke-home wax install ripgrep
HOME=/tmp/wax-smoke-home wax system status
HOME=/tmp/wax-smoke-home wax system upgrade
HOME=/tmp/wax-smoke-home wax system remove ripgrep
```

Then verify:

- requested package is installed
- base system dependencies are not unnecessarily unpacked into the prefix
- manifest contains installed files
- removal deletes tracked files
- status reflects the installed/removed package count
