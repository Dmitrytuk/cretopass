#!/usr/bin/env bash
set -uo pipefail

error_handler() {
    local exit_code=$?
    echo "" >&2
    echo "==> Script failed with exit code ${exit_code}" >&2
    echo "    Error at line ${BASH_LINENO[0]}: ${BASH_COMMAND}" >&2
    read -r -p "Press Enter to exit..."
    exit "$exit_code"
}
trap error_handler ERR

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DOCKERFILE="$SCRIPT_DIR/cretopass-aur-test.Dockerfile"
IMAGE_TAG="cretopass-aur-test"

VERSION="$(awk -F'"' '/^version[[:space:]]*=/{print $2; exit}' "$REPO_ROOT/Cargo.toml")"
if [[ -z "$VERSION" ]]; then
	echo "error: unable to determine version from Cargo.toml" >&2
	exit 1
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
    echo "error: Docker is not running or not accessible" >&2
    exit 1
fi

echo "==> Building Docker image: $IMAGE_TAG"
docker build -t "$IMAGE_TAG" -f "$DOCKERFILE" "$SCRIPT_DIR"

echo "==> Running AUR package tests in container"
docker run --rm \
	-v "$REPO_ROOT:/home/builder/repo:ro" \
	-v "$TARBALL:/home/builder/target/cretopass-${VERSION}.tar.gz:ro" \
	-v "$GUI_TARBALL:/home/builder/target/cretopass-gui-${VERSION}.tar.gz:ro" \
	-v "$TMP_PKGBUILD_DIR/cretopass/PKGBUILD:/home/builder/repo/packaging/arch/cretopass/PKGBUILD:ro" \
	-v "$TMP_PKGBUILD_DIR/cretopass-gui/PKGBUILD:/home/builder/repo/packaging/arch/cretopass-gui/PKGBUILD:ro" \
	"$IMAGE_TAG" bash -euo pipefail -c '
	SRCDEST=/home/builder/target
	export SRCDEST
	export CARGO_HOME=/home/builder/.cargo
	export RUSTUP_HOME=/home/builder/.rustup

	# ---------------------------------------------------------------
	# cretopass
	# ---------------------------------------------------------------
	echo "==> Building package: cretopass"
	cd /home/builder/repo/packaging/arch/cretopass
	makepkg --syncdeps --cleanbuild --force --noconfirm

	echo "==> namcap: cretopass"
	namcap ./cretopass-*.pkg.tar.zst

	echo "==> Installing cretopass"
	sudo pacman -U --noconfirm ./cretopass-*.pkg.tar.zst

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
	makepkg --syncdeps --cleanbuild --force --noconfirm

	echo "==> namcap: cretopass-gui"
	namcap ./cretopass-gui-*.pkg.tar.zst

	echo "==> Installing cretopass-gui"
	sudo pacman -U --noconfirm ./cretopass-gui-*.pkg.tar.zst

	echo "==> Smoke testing cretopass-gui"
	desktop-file-validate /usr/share/applications/cretopass-gui.desktop
	for size in 16 22 24 32 48 64 128 256 512; do
		test -f "/usr/share/icons/hicolor/${size}x${size}/apps/cretopass-gui.png"
	done
	test -f /usr/share/icons/hicolor/scalable/apps/cretopass-gui.svg
	test -f /usr/share/icons/hicolor/symbolic/apps/cretopass-gui-symbolic.svg
	echo "=== cretopass-gui OK ==="

	echo "==> All AUR package tests passed"
'

echo "==> Cleaning up"
rm -f "$TARBALL" "$GUI_TARBALL"
rm -rf "$TMP_PKGBUILD_DIR"
docker image rm -f "$IMAGE_TAG" >/dev/null 2>&1 || true

echo "==> Done"
read -r -p "Press Enter to exit..."