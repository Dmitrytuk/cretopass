#!/usr/bin/env bash
# set -uo pipefail

FAILED=false

AGENT=${AGENT:-1}
if [[ "$AGENT" == "1" ]]; then
    AGENT_OUT=/dev/null
else
    AGENT_OUT=/dev/stdout
fi

fail() {
    local msg="$1"
    echo "" >&2
    echo "==> ERROR: ${msg}" >&2
    if [[ -t 0 ]]; then
        echo -n "Press Enter to continue..."; read -r
    fi
    FAILED=true
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DOCKERFILE="$SCRIPT_DIR/cretopass-aur-test.Dockerfile"
IMAGE_TAG="cretopass-aur-test"

VERSION="$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$REPO_ROOT/Cargo.toml")"
if [[ -z "$VERSION" ]]; then
    fail "Unable to determine version from Cargo.toml"
fi

TARBALL="/tmp/cretopass-${VERSION}.tar.gz"
GUI_TARBALL="/tmp/cretopass-gui-${VERSION}.tar.gz"

TMP_PKGBUILD_DIR="$(mktemp -d /tmp/cretopass-pkgbuilds.XXXXXX)"

echo "==> Patching pkgver=${VERSION} in PKGBUILD files"
mkdir -p "$TMP_PKGBUILD_DIR/cretopass" "$TMP_PKGBUILD_DIR/cretopass-gui"
sed "s/^pkgver=.*/pkgver=${VERSION}/" "$REPO_ROOT/packaging/arch/cretopass/PKGBUILD" \
    > "$TMP_PKGBUILD_DIR/cretopass/PKGBUILD"
sed "s/^pkgver=.*/pkgver=${VERSION}/" "$REPO_ROOT/packaging/arch/cretopass-gui/PKGBUILD" \
    > "$TMP_PKGBUILD_DIR/cretopass-gui/PKGBUILD"

echo "==> Preparing local source tarball (cretopass-${VERSION})"
rm -f "$TARBALL"
git -C "$REPO_ROOT" archive \
    --format=tar.gz \
    --prefix="cretopass-${VERSION}/" \
    --output="$TARBALL" \
    HEAD

cp "$TARBALL" "$GUI_TARBALL"

if ! docker info >/dev/null 2>&1; then
    fail "Docker is not running or not accessible"
fi

if [[ "$FAILED" != true && -n "$VERSION" ]]; then
    echo "==> Building Docker image: $IMAGE_TAG"
    if ! docker build -t "$IMAGE_TAG" -f "$DOCKERFILE" "$SCRIPT_DIR" &>"$AGENT_OUT"; then
        fail "Docker image build failed"
    fi
fi

if [[ "$FAILED" != true && -n "$VERSION" ]]; then
    echo "==> Running AUR package tests in container"
    if ! docker run --rm \
        -e AGENT \
        -v "$REPO_ROOT:/home/builder/repo:ro" \
        -v "$TARBALL:/home/builder/target/cretopass-${VERSION}.tar.gz:ro" \
        -v "$GUI_TARBALL:/home/builder/target/cretopass-gui-${VERSION}.tar.gz:ro" \
        -v "$TMP_PKGBUILD_DIR/cretopass/PKGBUILD:/home/builder/repo/packaging/arch/cretopass/PKGBUILD:ro" \
        -v "$TMP_PKGBUILD_DIR/cretopass-gui/PKGBUILD:/home/builder/repo/packaging/arch/cretopass-gui/PKGBUILD:ro" \
        "$IMAGE_TAG" bash -uo pipefail -c '
        AGENT=${AGENT:-0}
        if [[ "$AGENT" == "1" ]]; then
            AGENT_OUT=/dev/null
        else
            AGENT_OUT=/dev/stdout
        fi

        SRCDEST=/home/builder/target
        export SRCDEST
        export CARGO_HOME=/home/builder/.cargo
        export RUSTUP_HOME=/home/builder/.rustup
        export BUILDDIR=/tmp/makepkg-build
        export PKGDEST=/tmp/makepkg-pkg
        mkdir -p "$PKGDEST"

        # ---------------------------------------------------------------
        # cretopass
        # ---------------------------------------------------------------
        echo "==> Building package: cretopass"
        cd /home/builder/repo/packaging/arch/cretopass
        makepkg --syncdeps --cleanbuild --force --noconfirm &>"$AGENT_OUT"

        CRETOPASS_PKG=$(ls "$PKGDEST"/cretopass-*.pkg.tar.zst 2>/dev/null | grep -v -- '-debug-' | head -1)

        echo "==> namcap: cretopass"
        namcap "$CRETOPASS_PKG" &>"$AGENT_OUT"

        echo "==> Installing cretopass"
        sudo pacman -U --noconfirm "$CRETOPASS_PKG" &>"$AGENT_OUT"

        echo "==> Smoke testing cretopass"
        cretopass --help >/dev/null
        cretopass --version | grep -q "$VERSION"
        test -f /usr/bin/cretopass
        test -f /usr/share/man/man1/cretopass.1
        test -f /usr/share/bash-completion/completions/cretopass
        test -f /usr/share/zsh/site-functions/_cretopass
        test -f /usr/share/fish/vendor_completions.d/cretopass.fish
        echo "=== cretopass OK ==="

        # ---------------------------------------------------------------
        # cretopass-gui
        # ---------------------------------------------------------------
        echo "==> Building package: cretopass-gui"
        cd /home/builder/repo/packaging/arch/cretopass-gui
        makepkg --syncdeps --cleanbuild --force --noconfirm &>"$AGENT_OUT"

        CRETOPASS_GUI_PKG=$(ls "$PKGDEST"/cretopass-gui-*.pkg.tar.zst 2>/dev/null | grep -v -- '-debug-' | head -1)

        echo "==> namcap: cretopass-gui"
        namcap "$CRETOPASS_GUI_PKG" &>"$AGENT_OUT"

        echo "==> Installing cretopass-gui"
        sudo pacman -U --noconfirm "$CRETOPASS_GUI_PKG" &>"$AGENT_OUT"

        echo "==> Smoke testing cretopass-gui"
        desktop-file-validate /usr/share/applications/cretopass-gui.desktop
        for size in 16 22 24 32 48 64 128 256 512; do
            test -f "/usr/share/icons/hicolor/${size}x${size}/apps/cretopass-gui.png"
        done
        test -f /usr/share/icons/hicolor/scalable/apps/cretopass-gui.svg
        test -f /usr/share/icons/hicolor/symbolic/apps/cretopass-gui-symbolic.svg
        echo "=== cretopass-gui OK ==="

        echo "==> All AUR package tests passed"
    '; then
        fail "AUR package tests failed"
    fi
fi

echo "==> Cleaning up"
rm -f "$TARBALL" "$GUI_TARBALL"
rm -rf "$TMP_PKGBUILD_DIR"
docker image rm -f "$IMAGE_TAG" >/dev/null 2>&1 || true

echo "==> Done"
if [[ "$FAILED" == true ]]; then
    echo "==> Script completed with errors (see above)"
fi
