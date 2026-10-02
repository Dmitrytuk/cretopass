#!/usr/bin/env bash
# set -uo pipefail

FAILED=false

fail() {
    local msg="$1"
    echo "" >&2
    echo "<ERROR>${msg}</ERROR>" >&2
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

        AUR_ERRORS=false

        echo "<INFO>Tarball: man page and packaging paths</INFO>"
        tar tzf "/home/builder/target/cretopass-${VERSION}.tar.gz" | grep -E "(cretopass\.1|packaging/arch/cretopass/)" || echo "<ERROR>no packaging files in tarball</ERROR>"
        echo "<INFO>PKGBUILD man install line</INFO>"
        grep -n "cretopass.1" /home/builder/repo/packaging/arch/cretopass/PKGBUILD || echo "<ERROR>no cretopass.1 reference in PKGBUILD</ERROR>"

        # ---------------------------------------------------------------
        # cretopass
        # ---------------------------------------------------------------
        echo "<INFO>Building package: cretopass</INFO>"
        cd /home/builder/repo/packaging/arch/cretopass
        if ! makepkg --syncdeps --cleanbuild --force --noconfirm; then
            echo "<ERROR>makepkg failed for cretopass</ERROR>"
            exit 1
        fi

        echo "<INFO>makepkg BUILDDIR top-level</INFO>"
        ls -la "$BUILDDIR" 2>/dev/null | while read -r line; do echo "<INFO>  $line</INFO>"; done
        echo "<INFO>makepkg BUILDDIR/src contents</INFO>"
        if [ -d "$BUILDDIR/src" ]; then
            ls -la "$BUILDDIR/src" 2>/dev/null | while read -r line; do echo "<INFO>  $line</INFO>"; done
        else
            echo "<ERROR>BUILDDIR/src not found</ERROR>"
        fi
        echo "<INFO>makepkg find: cretopass.1 under BUILDDIR</INFO>"
        find "$BUILDDIR" -name "cretopass.1" 2>/dev/null | while read -r f; do echo "<INFO>  found: $f</INFO>"; done
        if [ ${PIPESTATUS[0]} -ne 0 ]; then echo "<ERROR>no cretopass.1 in BUILDDIR</ERROR>"; fi
        echo "<INFO>makepkg find: man dirs under BUILDDIR</INFO>"
        find "$BUILDDIR" -type d -name "man*" 2>/dev/null | while read -r f; do echo "<INFO>  found: $f</INFO>"; done
        if [ ${PIPESTATUS[0]} -ne 0 ]; then echo "<ERROR>no man dirs in BUILDDIR</ERROR>"; fi
        echo "<INFO>makepkg pkgdir: list files inside man1</INFO>"
        ls -la "$BUILDDIR"/cretopass/pkg/cretopass/usr/share/man/man1/ 2>/dev/null | while read -r line; do echo "<INFO>  $line</INFO>"; done

        CRETOPASS_PKG=$(ls "$PKGDEST"/cretopass-*.pkg.tar.zst 2>/dev/null | grep -v -- '-debug-' | head -1)
        if [[ -z "$CRETOPASS_PKG" ]]; then
            echo "<ERROR>cretopass package not found in $PKGDEST</ERROR>"
            exit 1
        fi

        echo "<INFO>namcap: cretopass</INFO>"
        NAMCAP_OUT=$(namcap "$CRETOPASS_PKG")
        echo "<NAMCAP>$NAMCAP_OUT</NAMCAP>"

        echo "<INFO>Installing cretopass</INFO>"
        if ! sudo pacman -U --noconfirm "$CRETOPASS_PKG"; then
            echo "<ERROR>Failed to install cretopass</ERROR>"
            exit 1
        fi

        echo "<INFO>Smoke testing cretopass</INFO>"
        cretopass --help >/dev/null || { echo "<ERROR>cretopass --help failed</ERROR>"; AUR_ERRORS=true; }
        cretopass --version | grep -q "$VERSION" || { echo "<ERROR>cretopass --version mismatch</ERROR>"; AUR_ERRORS=true; }
        test -f /usr/bin/cretopass || { echo "<ERROR>/usr/bin/cretopass missing</ERROR>"; AUR_ERRORS=true; }
        test -f /usr/share/man/man1/cretopass.1 || { echo "<ERROR>man page missing</ERROR>"; AUR_ERRORS=true; }
        test -f /usr/share/bash-completion/completions/cretopass || { echo "<ERROR>bash completion missing</ERROR>"; AUR_ERRORS=true; }
        test -f /usr/share/zsh/site-functions/_cretopass || { echo "<ERROR>zsh completion missing</ERROR>"; AUR_ERRORS=true; }
        test -f /usr/share/fish/vendor_completions.d/cretopass.fish || { echo "<ERROR>fish completion missing</ERROR>"; AUR_ERRORS=true; }
        echo "<INFO>cretopass OK</INFO>"

        # ---------------------------------------------------------------
        # cretopass-gui
        # ---------------------------------------------------------------
        echo "<INFO>Building package: cretopass-gui</INFO>"
        cd /home/builder/repo/packaging/arch/cretopass-gui
        if ! makepkg --syncdeps --cleanbuild --force --noconfirm; then
            echo "<ERROR>makepkg failed for cretopass-gui</ERROR>"
            exit 1
        fi

        CRETOPASS_GUI_PKG=$(ls "$PKGDEST"/cretopass-gui-*.pkg.tar.zst 2>/dev/null | grep -v -- '-debug-' | head -1)
        if [[ -z "$CRETOPASS_GUI_PKG" ]]; then
            echo "<ERROR>cretopass-gui package not found in $PKGDEST</ERROR>"
            exit 1
        fi

        echo "<INFO>namcap: cretopass-gui</INFO>"
        NAMCAP_OUT=$(namcap "$CRETOPASS_GUI_PKG")
        echo "<NAMCAP>$NAMCAP_OUT</NAMCAP>"

        echo "<INFO>Installing cretopass-gui</INFO>"
        if ! sudo pacman -U --noconfirm "$CRETOPASS_GUI_PKG"; then
            echo "<ERROR>Failed to install cretopass-gui</ERROR>"
            exit 1
        fi

        echo "<INFO>Smoke testing cretopass-gui</INFO>"
        desktop-file-validate /usr/share/applications/cretopass-gui.desktop || { echo "<ERROR>desktop file validation failed</ERROR>"; AUR_ERRORS=true; }
        for size in 16 22 24 32 48 64 128 256 512; do
            test -f "/usr/share/icons/hicolor/${size}x${size}/apps/cretopass-gui.png" || { echo "<ERROR>icon ${size}x${size} missing</ERROR>"; AUR_ERRORS=true; }
        done
        test -f /usr/share/icons/hicolor/scalable/apps/cretopass-gui.svg || { echo "<ERROR>scalable icon missing</ERROR>"; AUR_ERRORS=true; }
        test -f /usr/share/icons/hicolor/symbolic/apps/cretopass-gui-symbolic.svg || { echo "<ERROR>symbolic icon missing</ERROR>"; AUR_ERRORS=true; }
        echo "<INFO>cretopass-gui OK</INFO>"

        if [[ "$AUR_ERRORS" == true ]]; then
            exit 1
        fi

        echo "<INFO>All AUR package tests passed</INFO>"
    '; then
        fail "AUR package tests failed"
    fi

    echo "<INFO>Running AUR package tests in container ended.</INFO>"
fi

echo "<INFO>Cleaning up</INFO>"
rm -f "$TARBALL" "$GUI_TARBALL"
rm -rf "$TMP_PKGBUILD_DIR"
docker image rm -f "$IMAGE_TAG" >/dev/null 2>&1 || true

echo "<INFO>Done</INFO>"
if [[ "$FAILED" == true ]]; then
    echo "<ERROR>Script completed with errors</ERROR>"
fi
