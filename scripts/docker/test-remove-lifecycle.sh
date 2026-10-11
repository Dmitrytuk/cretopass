#!/usr/bin/env bash
# Runs the `cretopass remove` lifecycle tests inside an isolated Docker container.
#
# The repo is mounted read-only at /app; all build artifacts go to /tmp/target
# inside the container, so the host working tree is never touched. The Cargo
# registry is cached in a named volume to keep repeated runs fast.
#
# Emits <INFO>/<ERROR> tagged output; use run-remove-test.sh for filtered output.
#
# Usage: scripts/docker/test-remove-lifecycle.sh

set -o pipefail

FAILED=false

fail() {
    local msg="$1"
    echo "" >&2
    echo "<ERROR>${msg}</ERROR>" >&2
    FAILED=true
}

main() {
    local script_dir repo_root dockerfile image_tag
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    repo_root="$(cd "$script_dir/../.." && pwd)"
    dockerfile="$script_dir/cretopass-remove-test.Dockerfile"
    image_tag="cretopass-remove-test"

    if ! docker info >/dev/null 2>&1; then
        fail "Docker is not running or not accessible"
        return 1
    fi

    echo "<INFO>Building Docker image: $image_tag</INFO>"
    if ! docker build -t "$image_tag" -f "$dockerfile" "$script_dir"; then
        fail "Docker image build failed"
        return 1
    fi

    echo "<INFO>Running remove lifecycle tests in container</INFO>"
    if ! docker run --rm \
        -v "$repo_root:/app:ro" \
        -v "cretopass-remove-target:/tmp/target" \
        -v "cretopass-cargo-registry:/cargo-cache" \
        -e CARGO_TARGET_DIR=/tmp/target \
        "$image_tag" \
        bash -uo pipefail -c '
            set -e
            cargo build --locked --bin cretopass
            ./tests/test-remove.sh
            cargo test --locked --all-features
            echo "<INFO>All remove lifecycle tests passed</INFO>"
        '; then
        fail "Remove lifecycle tests failed"
    fi

    echo "<INFO>Cleaning up Docker image</INFO>"
    docker image rm -f "$image_tag" >/dev/null 2>&1 || true

    echo "<INFO>Done</INFO>"
    if [[ "$FAILED" == true ]]; then
        echo "<ERROR>Script completed with errors</ERROR>"
        return 1
    fi
}

main "$@"