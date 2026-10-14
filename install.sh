#!/bin/sh
set -eu

REPO="Dmitrytuk/cretopass"
BIN="cretopass"

# Base URL for release assets.
# Honours CRETOPASS_INSTALL_URL to allow tests (and mirrors) to point at a
# different source. An empty value is treated as unset.
BASE_URL="${CRETOPASS_INSTALL_URL:-https://github.com/$REPO/releases/latest/download}"

# --- Args ---
FORCE=0
for arg in "$@"; do
  case "$arg" in
    --force|-f) FORCE=1 ;;
  esac
done

# --- Required tools ---
for tool in curl tar install; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$tool is required but not installed." >&2
    exit 1
  fi
done

# --- Detect OS ---
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
case "$OS" in
  linux|darwin) ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

# --- Detect arch ---
ARCH=$(uname -m)
case "$OS-$ARCH" in
  linux-x86_64)              TARGET="x86_64-unknown-linux-musl" ;;
  linux-aarch64|linux-arm64) TARGET="aarch64-unknown-linux-musl" ;;
  darwin-x86_64)             TARGET="x86_64-apple-darwin" ;;
  darwin-arm64)              TARGET="aarch64-apple-darwin" ;;
  *) echo "Unsupported platform: $OS-$ARCH"; exit 1 ;;
esac

# --- Detect existing installation ---
INSTALL_DIR="$HOME/.local/bin"
BIN_PATH="$INSTALL_DIR/$BIN"

parse_version() {
  "$1" --version 2>/dev/null \
    | head -n1 \
    | grep -oE '[0-9]+\.[0-9]+\.[0-9]+[^[:space:]]*' \
    | head -n1 || true
}

PREV_VERSION=""
if [ -x "$BIN_PATH" ]; then
  PREV_VERSION=$(parse_version "$BIN_PATH")
  [ -n "$PREV_VERSION" ] && echo "Existing installation detected: $PREV_VERSION"
fi

# --- Resolve latest version (via redirect, no API rate limit) ---
LATEST_VERSION=""
LATEST_URL=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
  "https://github.com/$REPO/releases/latest" 2>/dev/null || true)
if [ -n "$LATEST_URL" ]; then
  LATEST_VERSION="${LATEST_URL##*/}"
  LATEST_VERSION="${LATEST_VERSION#v}"
fi

# --- Early exit if up to date ---
if [ -n "$PREV_VERSION" ] && [ -n "$LATEST_VERSION" ] && \
   [ "$PREV_VERSION" = "$LATEST_VERSION" ] && [ "$FORCE" -eq 0 ]; then
  echo "Already up to date: $PREV_VERSION"
  echo "Use --force to reinstall."
  exit 0
fi

# --- SHA256 tool ---
if command -v sha256sum >/dev/null 2>&1; then
  SHA_CMD="sha256sum"
elif command -v shasum >/dev/null 2>&1; then
  SHA_CMD="shasum -a 256"
else
  echo "No SHA256 tool found"; exit 1
fi

# --- TTY helper (for prompts) ---
require_tty() {
  if [ ! -r /dev/tty ]; then
    echo "Non-interactive mode: cannot prompt. Aborting." >&2
    exit 1
  fi
}

tty_available() {
  [ -r /dev/tty ] 2>/dev/null
}

# --- Download ---
URL="$BASE_URL/$BIN-$TARGET.tar.gz"
SHA_URL="$URL.sha256"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "Downloading $URL ..."
curl -fsSL "$URL" -o "$TMP/$BIN.tar.gz"

# --- Checksum (fail-soft with prompt) ---
if curl -fsSL "$SHA_URL" -o "$TMP/$BIN.sha256" 2>/dev/null; then
  EXPECTED=$(cut -d' ' -f1 < "$TMP/$BIN.sha256" | tr 'A-Z' 'a-z')
  ACTUAL=$($SHA_CMD "$TMP/$BIN.tar.gz" | cut -d' ' -f1 | tr 'A-Z' 'a-z')
  if [ "$EXPECTED" != "$ACTUAL" ]; then
    echo "WARNING: checksum mismatch!"
    require_tty
    printf "Continue anyway? [y/N] "
    read -r ans < /dev/tty
    case "$ans" in [yY]*) ;; *) echo "Aborted."; exit 1 ;; esac
  fi
else
  echo "WARNING: checksum file not found."
  require_tty
  printf "Continue without verification? [y/N] "
  read -r ans < /dev/tty
  case "$ans" in [yY]*) ;; *) echo "Aborted."; exit 1 ;; esac
fi

# --- Verify archive integrity ---
if ! tar tzf "$TMP/$BIN.tar.gz" >/dev/null 2>&1; then
  echo "ERROR: downloaded archive is corrupted." >&2
  exit 1
fi

# --- Install binary ---
mkdir -p "$INSTALL_DIR"
tar xzf "$TMP/$BIN.tar.gz" -C "$TMP"
install -m 755 "$TMP/$BIN" "$INSTALL_DIR/$BIN"

# --- Resolve NEW_VERSION before GUI integration (needed for Info.plist) ---
NEW_VERSION=$(parse_version "$BIN_PATH")

# --- GUI integration ---
install_linux_desktop() {
  APP_DIR="$HOME/.local/share/applications"
  ICON_DIR="$HOME/.local/share/icons/hicolor/256x256/apps"
  mkdir -p "$APP_DIR" "$ICON_DIR" 2>/dev/null || true

  # Icon (best effort)
  curl -fsSL "$BASE_URL/cretopass.png" \
    -o "$ICON_DIR/cretopass.png" 2>/dev/null || true

  # .desktop from release, with @EXEC@ substituted to the installed binary path
  TMP_DESKTOP="$APP_DIR/cretopass.desktop.tmp"
  if curl -fsSL "$BASE_URL/cretopass.desktop" \
      -o "$TMP_DESKTOP" 2>/dev/null; then
    sed "s|@EXEC@|$INSTALL_DIR/$BIN|g" "$TMP_DESKTOP" \
      > "$APP_DIR/cretopass.desktop" 2>/dev/null || true
    chmod +x "$APP_DIR/cretopass.desktop" 2>/dev/null || true
    rm -f "$TMP_DESKTOP" 2>/dev/null || true
  fi

  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APP_DIR" 2>/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
  fi
}

install_macos_app() {
  APP="$HOME/Applications/cretopass.app"
  rm -rf "$APP"
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

  cat > "$APP/Contents/MacOS/cretopass" <<EOF
#!/bin/sh
exec /usr/bin/open -a Terminal "$INSTALL_DIR/$BIN"
EOF
  chmod +x "$APP/Contents/MacOS/cretopass"

  curl -fsSL "$BASE_URL/cretopass.icns" \
    -o "$APP/Contents/Resources/cretopass.icns" 2>/dev/null || true

  cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
 "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>cretopass</string>
  <key>CFBundleDisplayName</key><string>Cretopass</string>
  <key>CFBundleIdentifier</key><string>com.github.dmitrytuk.cretopass</string>
  <key>CFBundleExecutable</key><string>cretopass</string>
  <key>CFBundleIconFile</key><string>cretopass</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${NEW_VERSION:-1.0.0}</string>
</dict>
</plist>
EOF

  touch "$APP"
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
    -f "$APP" 2>/dev/null || true
}

case "$OS" in
  linux)  install_linux_desktop ;;
  darwin) install_macos_app ;;
esac

# --- Completions installation ---
install_completions() {
  echo "Installing shell completions..."

  # bash
  BASH_DIR="$HOME/.local/share/bash-completion/completions"
  if [ -d "$BASH_DIR" ] || mkdir -p "$BASH_DIR" 2>/dev/null; then
    curl -fsSL "$BASE_URL/cretopass.bash" \
      -o "$BASH_DIR/$BIN" 2>/dev/null || echo "  bash completion: skipped (not available)"
  fi

  # fish
  FISH_DIR="$HOME/.config/fish/completions"
  if mkdir -p "$FISH_DIR" 2>/dev/null; then
    curl -fsSL "$BASE_URL/cretopass.fish" \
      -o "$FISH_DIR/$BIN.fish" 2>/dev/null || echo "  fish completion: skipped (not available)"
  fi

  # zsh
  ZSH_DIR="$HOME/.zsh/completions"
  if mkdir -p "$ZSH_DIR" 2>/dev/null; then
    if curl -fsSL "$BASE_URL/_cretopass" \
        -o "$ZSH_DIR/_cretopass" 2>/dev/null; then
      # Add fpath entry to .zshrc if not already present (marker-wrapped)
      RC="$HOME/.zshrc"
      if [ -f "$RC" ] && ! grep -qsF "# >>> cretopass completion >>>" "$RC"; then
        if grep -q 'compinit' "$RC" 2>/dev/null; then
          awk -v home="$HOME" '
            /compinit/ && !done {
              print "# >>> cretopass completion >>>"
              print "fpath=(\"" home "/.zsh/completions\" $fpath)"
              print "# <<< cretopass completion <<<"
              done=1
            }
            { print }
          ' "$RC" > "$RC.tmp" && mv "$RC.tmp" "$RC"
        else
          {
            echo ""
            echo "# >>> cretopass completion >>>"
            echo "fpath=(\"$HOME/.zsh/completions\" \$fpath)"
            echo "# <<< cretopass completion <<<"
          } >> "$RC"
        fi
      fi
    else
      echo "  zsh completion: skipped (not available)"
    fi
  fi
}
install_completions

# --- PATH rc prompt ---
detect_rc() {
  case "${SHELL:-}" in
    */zsh)  echo "$HOME/.zshrc" ;;
    */bash) echo "$HOME/.bashrc" ;;
    */fish) echo "$HOME/.config/fish/config.fish" ;;
    *)      echo "" ;;
  esac
}

case ":$PATH:" in
  *:"$INSTALL_DIR":*) ;;
  *)
    RC_FILE=$(detect_rc)
    if [ -n "$RC_FILE" ]; then
      # Check if marker block already exists
      if [ -f "$RC_FILE" ] && grep -qsF "# >>> cretopass PATH >>>" "$RC_FILE"; then
        echo "$INSTALL_DIR is already referenced in $RC_FILE (marker found)."
      elif tty_available; then
        printf "\n$INSTALL_DIR is not in your PATH.\nAdd it to $RC_FILE? [y/N] "
        read -r ans < /dev/tty
        case "$ans" in
          [yY]|[yY][eE][sS])
            SHELL_NAME="${SHELL##*/}"
            if [ "$SHELL_NAME" = "fish" ]; then
              {
                echo ""
                echo "# >>> cretopass PATH >>>"
                echo "fish_add_path \$HOME/.local/bin"
                echo "# <<< cretopass PATH <<<"
              } >> "$RC_FILE"
            else
              {
                echo ""
                echo "# >>> cretopass PATH >>>"
                echo "export PATH=\"\$HOME/.local/bin:\$PATH\""
                echo "# <<< cretopass PATH <<<"
              } >> "$RC_FILE"
            fi
            echo "Added $INSTALL_DIR to PATH in $RC_FILE."
            ;;
          *) echo "Skipped. You can add $INSTALL_DIR to your PATH manually." ;;
        esac
      else
        echo "WARNING: $INSTALL_DIR is not in your PATH."
        if [ -n "$RC_FILE" ]; then
          echo "Add this to $RC_FILE:"
        fi
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
      fi
    else
      echo "WARNING: $INSTALL_DIR is not in your PATH."
      echo "Add it to your shell config manually."
    fi
    ;;
esac

# --- Report ---
if [ -n "$PREV_VERSION" ]; then
  echo "Installed: $PREV_VERSION -> ${NEW_VERSION:-unknown}"
else
  echo "Installed: <none> -> ${NEW_VERSION:-unknown}"
fi

echo "Installed $BIN to $INSTALL_DIR/$BIN"