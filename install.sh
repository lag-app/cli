#!/bin/sh
# Lag CLI installer — https://github.com/lag-app/cli
# Usage: curl -fsSL https://raw.githubusercontent.com/lag-app/cli/main/install.sh | sh
set -e

REPO="lag-app/cli"
INSTALL_DIR="${LAG_INSTALL_DIR:-$HOME/.lag/bin}"

# --- Detect platform ----------------------------------------------------------

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin) os="darwin" ;;
  Linux)  os="linux" ;;
  *)
    echo "Error: unsupported OS: $OS" >&2
    exit 1
    ;;
esac

case "$ARCH" in
  x86_64|amd64)     arch="x86_64"; [ "$os" = "linux" ] && arch="amd64" ;;
  arm64|aarch64)     arch="aarch64"; [ "$os" = "linux" ] && arch="arm64" ;;
  *)
    echo "Error: unsupported architecture: $ARCH" >&2
    exit 1
    ;;
esac

# --- Resolve version ----------------------------------------------------------

if [ -n "$LAG_VERSION" ]; then
  VERSION="$LAG_VERSION"
else
  VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' | head -1 | sed 's/.*"v\(.*\)".*/\1/')"
fi

if [ -z "$VERSION" ]; then
  echo "Error: could not determine latest version" >&2
  exit 1
fi

TARBALL="lag-${VERSION}-${os}-${arch}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${VERSION}/${TARBALL}"

echo "Installing lag v${VERSION} (${os}/${arch})..."

# --- Download & install -------------------------------------------------------

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

curl -fsSL "$URL" -o "${TMPDIR}/${TARBALL}"
tar xzf "${TMPDIR}/${TARBALL}" -C "$TMPDIR"

mkdir -p "$INSTALL_DIR"
mv "${TMPDIR}/lag" "${INSTALL_DIR}/lag"
chmod +x "${INSTALL_DIR}/lag"

echo "Installed lag to ${INSTALL_DIR}/lag"

# --- Add to PATH --------------------------------------------------------------

add_to_path() {
  local rcfile="$1"
  local line="export PATH=\"${INSTALL_DIR}:\$PATH\""
  if [ -f "$rcfile" ] && grep -qF "$INSTALL_DIR" "$rcfile" 2>/dev/null; then
    return
  fi
  echo "" >> "$rcfile"
  echo "# Lag CLI" >> "$rcfile"
  echo "$line" >> "$rcfile"
  echo "Updated $rcfile"
}

case "$(basename "${SHELL:-/bin/sh}")" in
  zsh)
    add_to_path "$HOME/.zshrc"
    ;;
  bash)
    if [ -f "$HOME/.bash_profile" ]; then
      add_to_path "$HOME/.bash_profile"
    else
      add_to_path "$HOME/.bashrc"
    fi
    ;;
  fish)
    FISH_CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/fish/conf.d/lag.fish"
    if [ ! -f "$FISH_CONFIG" ] || ! grep -qF "$INSTALL_DIR" "$FISH_CONFIG" 2>/dev/null; then
      mkdir -p "$(dirname "$FISH_CONFIG")"
      echo "fish_add_path ${INSTALL_DIR}" > "$FISH_CONFIG"
      echo "Updated $FISH_CONFIG"
    fi
    ;;
  *)
    if [ -f "$HOME/.profile" ]; then
      add_to_path "$HOME/.profile"
    else
      add_to_path "$HOME/.bashrc"
    fi
    ;;
esac

# --- Verify -------------------------------------------------------------------

if echo "$PATH" | grep -qF "$INSTALL_DIR"; then
  echo "Done! Run 'lag --help' to get started."
else
  echo "Done! Restart your shell or run:"
  echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
  echo "Then run 'lag --help' to get started."
fi
