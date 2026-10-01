#!/usr/bin/env bash
# set -uo pipefail

FAILED=false

fail() {
    local msg="$1"
    echo "" >&2
    echo "<ERROR>${msg}</ERROR>" >&2
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

echo "<INFO>Patching pkgver=${VERSION} in PKGBUILD files</INFO>"
mkdir -p "$TMP_PKGBUILD_DIR/cretopass" "$TMP_PKGBUILD_DIR/cretopass-gui"
sed "s/^pkgver=.*/pkgver=${VERSION}/" "$REPO_ROOT/packaging/arch/cretopass/PKGBUILD" \
    > "$TMP_PKGBUILD_DIR/cretopass/PKGBUILD"
sed "s/^pkgver=.*/pkgver=${VERSION}/" "$REPO_ROOT/packaging/arch/cretopass-gui/PKGBUILD" \
    > "$TMP_PKGBUILD_DIR/cretopass-gui/PKGBUILD"

echo "<INFO>Preparing local source tarball (cretopass-${VERSION})</INFO>"
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
    echo "<INFO>Building Docker image: $IMAGE_TAG</INFO>"
    if ! docker build -t "$IMAGE_TAG" -f "$DOCKERFILE" "$SCRIPT_DIR"; then
        fail "Docker image build failed"
    fi
fi

if [[ "$FAILED" != true && -n "$VERSION" ]]; then
    echo "<INFO>Running AUR package tests in container</INFO>"
    if ! docker run --rm \
        -e VERSION="$VERSION" \
        -v "$REPO_ROOT:/home/builder/repo:ro" \
        -v "$TARBALL:/home/builder/target/cretopass-${VERSION}.tar.gz:ro" \
        -v "$GUI_TARBALL:/home/builder/target/cretopass-gui-${VERSION}.tar.gz:ro" \
        -v "$TMP_PKGBUILD_DIR/cretopass/PKGBUILD:/home/builder/repo/packaging/arch/cretopass/PKGBUILD:ro" \
        -v "$TMP_PKGBUILD_DIR/cretopass-gui/PKGBUILD:/home/builder/repo/packaging/arch/cretopass-gui/PKGBUILD:ro" \
        "$IMAGE_TAG" bash -uo pipefail -c '
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
        echo "<INFO>Building package: cretopass</INFO>"
        cd /home/builder/repo/packaging/arch/cretopass
        makepkg --syncdeps --cleanbuild --force --noconfirm

        CRETOPASS_PKG=$(ls "$PKGDEST"/cretopass-*.pkg.tar.zst 2>/dev/null | grep -v -- '-debug-' | head -1)

        echo "<INFO>namcap: cretopass</INFO>"
        namcap "$CRETOPASS_PKG"

        echo "<INFO>Installing cretopass</INFO>"
        sudo pacman -U --noconfirm "$CRETOPASS_PKG"

        echo "<INFO>Smoke testing cretopass</INFO>"
        cretopass --help >/dev/null
        cretopass --version | grep -q "$VERSION"
        test -f /usr/bin/cretopass
        test -f /usr/share/man/man1/cretopass.1
        test -f /usr/share/bash-completion/completions/cretopass
        test -f /usr/share/zsh/site-functions/_cretopass
        test -f /usr/share/fish/vendor_completions.d/cretopass.fish
        echo "<INFO>cretopass OK</INFO>"

        # ---------------------------------------------------------------
        # cretopass-gui
        # ---------------------------------------------------------------
        echo "<INFO>Building package: cretopass-gui</INFO>"
        cd /home/builder/repo/packaging/arch/cretopass-gui
        makepkg --syncdeps --cleanbuild --force --noconfirm

        CRETOPASS_GUI_PKG=$(ls "$PKGDEST"/cretopass-gui-*.pkg.tar.zst 2>/dev/null | grep -v -- '-debug-' | head -1)

        echo "<INFO>namcap: cretopass-gui</INFO>"
        namcap "$CRETOPASS_GUI_PKG"

        echo "<INFO>Installing cretopass-gui</INFO>"
        sudo pacman -U --noconfirm "$CRETOPASS_GUI_PKG"

        echo "<INFO>Smoke testing cretopass-gui</INFO>"
        desktop-file-validate /usr/share/applications/cretopass-gui.desktop
        for size in 16 22 24 32 48 64 128 256 512; do
            test -f "/usr/share/icons/hicolor/${size}x${size}/apps/cretopass-gui.png"
        done
        test -f /usr/share/icons/hicolor/scalable/apps/cretopass-gui.svg
        test -f /usr/share/icons/hicolor/symbolic/apps/cretopass-gui-symbolic.svg
        echo "<INFO>cretopass-gui OK</INFO>"

        echo "<INFO>All AUR package tests passed</INFO>"
    '; then
        fail "AUR package tests failed"
    fi
fi

echo "<INFO>Cleaning up</INFO>"
rm -f "$TARBALL" "$GUI_TARBALL"
rm -rf "$TMP_PKGBUILD_DIR"
docker image rm -f "$IMAGE_TAG" >/dev/null 2>&1 || true

echo "<INFO>Done</INFO>"
if [[ "$FAILED" == true ]]; then
    echo "<ERROR>Script completed with errors</ERROR>"
fi
