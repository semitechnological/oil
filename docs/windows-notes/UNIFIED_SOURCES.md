# Unified Windows-oriented package sources in wax

wax can treat **Homebrew** (cached index), **Scoop** (Main bucket JSON), **winget-pkgs** (GitHub YAML portable zips), and **Chocolatey** (community `.nupkg`) as install/search targets. Downloads use wax’s **multipart HTTP** where applicable; no Scoop PowerShell and no `winget.exe` are required for the supported subsets.

## Bang prefixes

| Prefix | Meaning |
|--------|---------|
| `scoop/` | Force Scoop Main manifest (`scoop/ripgrep`). |
| `choco/` or `chocolatey/` | Force Chocolatey gallery id (`choco/git`). |
| `winget/` | Force winget **PackageIdentifier** (`winget/JesseDuffield.lazygit`). |
| `brew/` or `homebrew/` | Force Homebrew-style resolution (`brew/openssl`). |
| *(none)* | **Auto**: on Windows, probe all sources in parallel and pick the **fastest** tier that matches (brew → scoop → winget → chocolatey). |

Tap-style names (`user/repo/formula`) and version pins (`pkg@version`) skip auto-routing and use the normal Homebrew path.

## “Fastest” when names collide

`Ecosystem::speed_rank` (lower = preferred): **brew (0) < scoop (1) < winget (2) < chocolatey (3)**.

- **`wax search`**: Homebrew formulae/casks are listed first; remote hits that duplicate an existing formula or cask **name** are hidden so the faster catalogue wins.
- **Remote-only dedupe**: If the same id appears in Scoop, winget, and Chocolatey, the **fastest** source is kept.

## What is actually installed

| Source | wax behaviour |
|--------|----------------|
| **brew** | Existing bottle/source/cask flows. |
| **scoop** | JSON manifest → zip/tar.gz → `~/.local/wax/bin` (Windows). No `pre_install` / `installer` scripts. |
| **winget** | Latest version under `manifests/<letter>/…` on **microsoft/winget-pkgs**; **only** `InstallerType: zip` + `NestedInstallerType: portable` manifests. |
| **chocolatey** | Latest `.nupkg` → extract → copy `tools/**/*.exe` (filters obvious uninstall/choco helpers). Script-only packages fail with a clear error. |

MSI/EXE/MSIX installers from winget or Chocolatey are **out of scope** for wax-managed install (use vendor installers or `winget.exe` / `choco.exe`).

## Search

- **Unified** (no prefix): Homebrew + Scoop Main index (GitHub tree, cached 24h) + Chocolatey HTML search + optional winget GitHub **code search** if `GITHUB_TOKEN` is set.
- **Prefixed** (`scoop/foo`, …): Only that catalogue.

## Related files

- `src/package_spec.rs` — bang parsing and speed ordering.
- `src/ecosystem_install.rs` — auto pick + forced install routing.
- `src/remote_search.rs` — merged search and dedupe.
- `src/scoop.rs`, `src/winget_install.rs`, `src/chocolatey.rs` — download + extract + shim to `~/.local/wax/bin`.

See also [DESK_RESEARCH.md](DESK_RESEARCH.md) and [WINDOWS_PACKAGE_MANAGER_INVESTIGATION.md](../WINDOWS_PACKAGE_MANAGER_INVESTIGATION.md).
