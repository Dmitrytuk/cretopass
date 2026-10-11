#!/bin/bash
# End-to-end test for `cretopass remove`.
#
# Builds the binary, installs a fake set of artifacts into an isolated $HOME,
# runs `cretopass remove` against it, and asserts nothing is left behind.
#
# Usage: tests/test-remove.sh

set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> Building cretopass"
cargo build --quiet --bin cretopass

# Honour CARGO_TARGET_DIR so the script works both on the host and inside a
# container where the source is mounted read-only and the target lives in /tmp.
TARGET_DIR="${CARGO_TARGET_DIR:-$(pwd)/target}"
BIN="$TARGET_DIR/debug/cretopass"
TMP_HOME="$(mktemp -d)"
trap 'rm -rf "$TMP_HOME"' EXIT

export HOME="$TMP_HOME"
export CRETOPASS_CONFIG_DIR="$TMP_HOME/.config/cretopass"

echo "==> Creating fake installation in $TMP_HOME"

# Binary (must sit under $HOME/.local/bin so `detect` sees a self-managed install)
mkdir -p "$TMP_HOME/.local/bin"
cp "$BIN" "$TMP_HOME/.local/bin/cretopass"

# Config directory + a data file
mkdir -p "$CRETOPASS_CONFIG_DIR"
echo "secret" > "$CRETOPASS_CONFIG_DIR/vault.dat"

# Shell completions
mkdir -p "$TMP_HOME/.local/share/bash-completion/completions"
echo "bash" > "$TMP_HOME/.local/share/bash-completion/completions/cretopass"
mkdir -p "$TMP_HOME/.zsh/completions"
echo "zsh" > "$TMP_HOME/.zsh/completions/_cretopass"
mkdir -p "$TMP_HOME/.config/fish/completions"
echo "fish" > "$TMP_HOME/.config/fish/completions/cretopass.fish"

# GUI desktop + icon
mkdir -p "$TMP_HOME/.local/share/applications"
echo "desktop" > "$TMP_HOME/.local/share/applications/cretopass.desktop"
mkdir -p "$TMP_HOME/.local/share/icons/hicolor/256x256/apps"
echo "png" > "$TMP_HOME/.local/share/icons/hicolor/256x256/apps/cretopass.png"

# rc files with PATH + zsh completion marker blocks
mkdir -p "$TMP_HOME/.config/fish"
cat > "$TMP_HOME/.bashrc" <<'EOF'
export PATH="$HOME/bin:$PATH"
# >>> cretopass PATH >>>
export PATH="$HOME/.local/bin:$PATH"
# <<< cretopass PATH <<<
alias ll='ls -la'
EOF
cat > "$TMP_HOME/.zshrc" <<'EOF'
compinit
# >>> cretopass completion >>>
fpath=("$HOME/.zsh/completions" $fpath)
# <<< cretopass completion <<<
# >>> cretopass PATH >>>
export PATH="$HOME/.local/bin:$PATH"
# <<< cretopass PATH <<<
EOF
cat > "$TMP_HOME/.config/fish/config.fish" <<'EOF'
set -gx PATH $HOME/bin $PATH
# >>> cretopass PATH >>>
fish_add_path $HOME/.local/bin
# <<< cretopass PATH <<<
EOF

echo "==> Running cretopass remove"
echo "y" | "$TMP_HOME/.local/bin/cretopass" remove

echo "==> Verifying nothing is left behind"

fail=0

check_removed() {
    local path="$1"
    if [ -e "$path" ]; then
        echo "FAIL: still exists: $path"
        fail=1
    fi
}

check_removed "$TMP_HOME/.local/bin/cretopass"
check_removed "$CRETOPASS_CONFIG_DIR"
check_removed "$TMP_HOME/.local/share/bash-completion/completions/cretopass"
check_removed "$TMP_HOME/.zsh/completions/_cretopass"
check_removed "$TMP_HOME/.config/fish/completions/cretopass.fish"
check_removed "$TMP_HOME/.local/share/applications/cretopass.desktop"
check_removed "$TMP_HOME/.local/share/icons/hicolor/256x256/apps/cretopass.png"

# rc files must survive but be marker-free
for rc in "$TMP_HOME/.bashrc" "$TMP_HOME/.zshrc" "$TMP_HOME/.config/fish/config.fish"; do
    if [ ! -e "$rc" ]; then
        echo "FAIL: rc file was deleted: $rc"
        fail=1
        continue
    fi
    if grep -q "cretopass" "$rc"; then
        echo "FAIL: marker remains in $rc:"
        grep -n "cretopass" "$rc" || true
        fail=1
    fi
done

if [ "$fail" -eq 0 ]; then
    echo "PASS: remove cleaned up everything"
else
    echo "FAIL: remove left artifacts behind"
    exit 1
fi