#!/usr/bin/env bash
# oil installer — from a clone: builds with cargo. Otherwise: GitHub Releases binary.
# Usage:
#   ./install.sh                         # in a clone → cargo build --release
#   OIL_USE_RELEASE=1 ./install.sh       # in a clone → still use releases
#   curl -fsSL …/install.sh | bash       # download latest release
#
set -euo pipefail

REPO="semitechnological/oil"
INSTALL_DIR="${OIL_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${OIL_VERSION:-}"

RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

info()  { printf "${CYAN}%s${NC}\n" "$*"; }
ok()    { printf "${GREEN}✓ %s${NC}\n" "$*"; }
warn()  { printf "${YELLOW}! %s${NC}\n" "$*" >&2; }
die()   { printf "${RED}error: %s${NC}\n" "$*" >&2; exit 1; }

path_hint() {
  if command -v oil &>/dev/null 2>&1; then
    return
  fi
  printf "\n${BOLD}Add oil to your PATH:${NC}\n"
  case "${SHELL:-}" in
    */fish) printf '  fish_add_path %s\n' "$INSTALL_DIR" ;;
    *)      printf '  echo '\''export PATH="%s:$PATH"'\'' >> ~/.bashrc  # or ~/.zshrc\n' "$INSTALL_DIR" ;;
  esac
}

detect_features() {
  local os
  os="$(uname -s)"
  if [ "$os" = "Darwin" ]; then
    echo "system-brew"
    return
  fi

  # Default cross-distro features
  local features="system-flatpak,system-snap,system-brew"

  # Distro detection via /etc/os-release
  local distro_id=""
  local distro_like=""
  if [ -f /etc/os-release ]; then
    distro_id="$(grep -E '^ID=' /etc/os-release | cut -d= -f2 | tr -d '"' | tr '[:upper:]' '[:lower:]')"
    distro_like="$(grep -E '^ID_LIKE=' /etc/os-release | cut -d= -f2 | tr -d '"' | tr '[:upper:]' '[:lower:]')"
  fi

  local all_ids=" ${distro_id} ${distro_like} "
  local pm_detected=""

  if [[ "$all_ids" =~ "debian" || "$all_ids" =~ "ubuntu" || "$all_ids" =~ "mint" || "$all_ids" =~ "pop" || "$all_ids" =~ "kali" || "$all_ids" =~ "parrot" ]]; then
    features="${features},system-apt"
    pm_detected="apt"
  elif [[ "$all_ids" =~ "fedora" || "$all_ids" =~ "rhel" || "$all_ids" =~ "centos" || "$all_ids" =~ "rocky" || "$all_ids" =~ "alma" ]]; then
    features="${features},system-dnf"
    pm_detected="dnf"
  elif [[ "$all_ids" =~ "arch" || "$all_ids" =~ "manjaro" || "$all_ids" =~ "garuda" || "$all_ids" =~ "endeavouros" || "$all_ids" =~ "artix" ]]; then
    features="${features},system-pacman"
    pm_detected="pacman"
  elif [[ "$all_ids" =~ "alpine" || "$all_ids" =~ "chimera" ]]; then
    features="${features},system-apk"
    pm_detected="apk"
  elif [[ "$all_ids" =~ "suse" || "$all_ids" =~ "opensuse" || "$all_ids" =~ "sles" ]]; then
    features="${features},system-zypper,system-dnf"
    pm_detected="zypper"
  elif [[ "$all_ids" =~ "void" ]]; then
    features="${features},system-xbps"
    pm_detected="xbps"
  elif [[ "$all_ids" =~ "solus" ]]; then
    features="${features},system-eopkg"
    pm_detected="eopkg"
  elif [[ "$all_ids" =~ "openwrt" || "$all_ids" =~ "lede" ]]; then
    features="${features},system-opkg"
    pm_detected="opkg"
  elif [[ "$all_ids" =~ "nixos" ]]; then
    features="${features},system-nix"
    pm_detected="nix"
  fi

  # Fallback to checking command presence on PATH
  if [ -z "$pm_detected" ]; then
    if command -v apt-get &>/dev/null; then
      features="${features},system-apt"
    elif command -v dnf &>/dev/null || command -v yum &>/dev/null; then
      features="${features},system-dnf"
    elif command -v pacman &>/dev/null; then
      features="${features},system-pacman"
    elif command -v apk &>/dev/null; then
      features="${features},system-apk"
    elif command -v zypper &>/dev/null; then
      features="${features},system-zypper,system-dnf"
    elif command -v xbps-install &>/dev/null; then
      features="${features},system-xbps"
    elif command -v emerge &>/dev/null; then
      features="${features},system-emerge"
    elif command -v nix-env &>/dev/null || command -v nix &>/dev/null; then
      features="${features},system-nix"
    elif command -v opkg &>/dev/null; then
      features="${features},system-opkg"
    elif command -v eopkg &>/dev/null; then
      features="${features},system-eopkg"
    elif command -v swupd &>/dev/null; then
      features="${features},system-swupd"
    fi
  fi

  echo "$features"
}

install_from_repo() {
  local root="$1"
  if ! command -v cargo &>/dev/null; then
    die "cargo not in PATH — install Rust from https://rustup.rs/ or use OIL_USE_RELEASE=1 to download a binary."
  fi

  local build_features
  if [ "${OIL_BUILD_ALL:-0}" = "1" ]; then
    info "Building oil from local checkout (${root}) with all features…"
    ( cd "$root" && cargo build --release )
  else
    build_features="${OIL_BUILD_FEATURES:-$(detect_features)}"
    info "Building oil from local checkout (${root}) with features: ${build_features}…"
    ( cd "$root" && cargo build --release --no-default-features --features "$build_features" )
  fi

  mkdir -p "$INSTALL_DIR"
  cp "${root}/target/release/oil" "${INSTALL_DIR}/oil"
  chmod +x "${INSTALL_DIR}/oil"
  ok "oil built and installed to ${INSTALL_DIR}/oil"
  path_hint
}

install_from_release() {
  local OS ARCH os arch ASSET BASE_URL TMP TMP_SHA HAVE_SHA EXPECTED ACTUAL

  OS="$(uname -s)"
  ARCH="$(uname -m)"

  case "$OS" in
    Linux)   os="linux" ;;
    Darwin)  os="macos" ;;
    FreeBSD) os="freebsd" ;;
    *)       die "Unsupported OS: $OS" ;;
  esac

  case "$ARCH" in
    x86_64|amd64)          arch="x64"   ;;
    aarch64|arm64)         arch="arm64" ;;
    *)                     die "Unsupported architecture: $ARCH" ;;
  esac

  ASSET="oil-${os}-${arch}"

  if [ "$os" = "linux" ]; then
    local distro_id=""
    local distro_like=""
    if [ -f /etc/os-release ]; then
      distro_id="$(grep -E '^ID=' /etc/os-release | cut -d= -f2 | tr -d '"' | tr '[:upper:]' '[:lower:]')"
      distro_like="$(grep -E '^ID_LIKE=' /etc/os-release | cut -d= -f2 | tr -d '"' | tr '[:upper:]' '[:lower:]')"
    fi
    local all_ids=" ${distro_id} ${distro_like} "

    if [[ "$all_ids" =~ "fedora" || "$all_ids" =~ "rhel" || "$all_ids" =~ "centos" || "$all_ids" =~ "rocky" || "$all_ids" =~ "alma" ]]; then
      ASSET="${ASSET}-fedora"
    elif [[ "$all_ids" =~ "arch" || "$all_ids" =~ "manjaro" || "$all_ids" =~ "garuda" || "$all_ids" =~ "endeavouros" || "$all_ids" =~ "artix" ]]; then
      ASSET="${ASSET}-arch"
    elif [[ "$all_ids" =~ "alpine" || "$all_ids" =~ "chimera" ]]; then
      ASSET="${ASSET}-musl"
    elif [[ "$all_ids" =~ "suse" || "$all_ids" =~ "opensuse" || "$all_ids" =~ "sles" ]]; then
      ASSET="${ASSET}-opensuse"
    elif [[ "$all_ids" =~ "void" ]]; then
      ASSET="${ASSET}-void"
    elif [[ "$all_ids" =~ "nixos" ]]; then
      ASSET="${ASSET}-nix"
    else
      # Fallback check for musl libc
      if command -v ldd &>/dev/null && ldd /bin/ls 2>&1 | grep -q 'musl'; then
        ASSET="${ASSET}-musl"
      fi
    fi
  fi

  if [ -z "$VERSION" ]; then
    info "Fetching latest release version…"
    VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
      | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"\(.*\)".*/\1/')"
    [ -n "$VERSION" ] || die "Could not determine latest version from GitHub API"
  fi

  info "Installing oil ${VERSION} (${os}/${arch}) from GitHub Releases…"

  BASE_URL="https://github.com/${REPO}/releases/download/${VERSION}"
  TMP="$(mktemp)"
  TMP_SHA="$(mktemp)"
  trap 'rm -f "${TMP:-}" "${TMP_SHA:-}"' EXIT

  if command -v curl &>/dev/null; then
    curl -fsSL --progress-bar "${BASE_URL}/${ASSET}" -o "$TMP"
    HAVE_SHA=0
    if curl -fsSL "${BASE_URL}/${ASSET}.sha256" -o "$TMP_SHA" 2>/dev/null; then
      HAVE_SHA=1
    fi
  elif command -v wget &>/dev/null; then
    wget -q --show-progress "${BASE_URL}/${ASSET}" -O "$TMP"
    HAVE_SHA=0
    if wget -q "${BASE_URL}/${ASSET}.sha256" -O "$TMP_SHA" 2>/dev/null; then
      HAVE_SHA=1
    fi
  else
    die "curl or wget is required"
  fi

  if [ "$HAVE_SHA" -eq 1 ] && [ -s "$TMP_SHA" ]; then
    EXPECTED="$(tr -d '[:space:]' < "$TMP_SHA")"
    if command -v sha256sum &>/dev/null; then
      ACTUAL="$(sha256sum "$TMP" | awk '{print $1}')"
    elif command -v shasum &>/dev/null; then
      ACTUAL="$(shasum -a 256 "$TMP" | awk '{print $1}')"
    else
      die "sha256sum or shasum is required to verify release integrity"
    fi

    [ "$ACTUAL" = "$EXPECTED" ] || die "SHA256 mismatch — download may be corrupted or tampered with
  expected: $EXPECTED
  actual:   $ACTUAL"
    ok "checksum verified"
  elif [ "${OIL_NO_VERIFY:-}" = "1" ]; then
    warn "OIL_NO_VERIFY=1 set — skipping integrity verification"
  else
    die "No checksum file for ${VERSION}; set OIL_NO_VERIFY=1 to install without integrity verification"
  fi

  chmod +x "$TMP"

  mkdir -p "$INSTALL_DIR"
  mv "$TMP" "${INSTALL_DIR}/oil"

  ok "oil ${VERSION} installed to ${INSTALL_DIR}/oil"
  path_hint
}

# ---- repo vs release -------------------------------------------------------

_src="${BASH_SOURCE[0]:-}"
if [[ -n "$_src" ]] && [[ "$(basename -- "$_src")" == "install.sh" ]] && [[ "${OIL_USE_RELEASE:-}" != "1" ]]; then
  _root="$(cd "$(dirname -- "$_src")" && pwd)"
  if [ -f "${_root}/Cargo.toml" ] && grep -q 'name = "oil"' "${_root}/Cargo.toml"; then
    install_from_repo "$_root"
    exit 0
  fi
fi

install_from_release
