#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_SCRIPT="$SCRIPT_DIR/test-aur-packages.sh"

VERBOSE=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        -v) VERBOSE=true; shift ;;
        *)  echo "Usage: $0 [-v]" >&2; exit 2 ;;
    esac
done

if [[ "$VERBOSE" == true ]]; then
    exec "$TEST_SCRIPT"
fi

if ! command -v pv >/dev/null 2>&1; then
    echo "ERROR: pv is required. Install it or use -v for full output." >&2
    exit 1
fi
if ! command -v perl >/dev/null 2>&1; then
    echo "ERROR: perl is required. Install it or use -v for full output." >&2
    exit 1
fi

"$TEST_SCRIPT" 2>&1 | pv | perl -0777 -ne 'print "$&\n" while /<(INFO|ERROR|NAMCAP)>.*?<\/\1>/gs'
