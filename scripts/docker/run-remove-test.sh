#!/usr/bin/env bash
# Wrapper that runs the `cretopass remove` lifecycle tests with filtered output.
#
# Runs test-remove-lifecycle.sh. By default filters output to <INFO>/<ERROR> tags.
#
# Flags:
#   -v    Full verbose output (run test-remove-lifecycle.sh as-is)
#   -h    Show this help message

set -o pipefail

main() {
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    TEST_SCRIPT="$SCRIPT_DIR/test-remove-lifecycle.sh"

    VERBOSE=false

    while [[ $# -gt 0 ]]; do
        case "$1" in
            -v) VERBOSE=true; shift ;;
            -h)
                echo "Wrapper that runs cretopass remove lifecycle tests with filtered output."
                echo ""
                echo "Runs test-remove-lifecycle.sh. By default filters output to <INFO>/<ERROR> tags."
                echo ""
                echo "Flags:"
                echo "  -v    Full verbose output (run test-remove-lifecycle.sh as-is)"
                echo "  -h    Show this help message"
                return 0
                ;;
            *)  echo "Usage: $0 [-v] [-h]" >&2; return 2 ;;
        esac
    done

    if [[ "$VERBOSE" == true ]]; then
        exec "$TEST_SCRIPT"
    fi

    if ! command -v pv >/dev/null 2>&1; then
        echo "ERROR: pv is required. Install it or use -v for full output." >&2
        return 1
    fi
    if ! command -v perl >/dev/null 2>&1; then
        echo "ERROR: perl is required. Install it or use -v for full output." >&2
        return 1
    fi

    "$TEST_SCRIPT" 2>&1 | pv | perl -0777 -ne 'print "$&\n" while /<(INFO|ERROR)>.*?<\/\1>/gs'
}

main "$@"