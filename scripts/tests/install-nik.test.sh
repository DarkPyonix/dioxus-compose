#!/usr/bin/env bash
# Offline tests for scripts/install-nik.sh.
#
# The download path needs the network and ~1GB of disk, so it is exercised by
# CI, not here. What is tested here is the logic CI depends on being right:
# the short-circuit that makes a warm cache free, and the argument handling.

set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
install_nik="$script_dir/../install-nik.sh"

failures=0
pass() { echo "ok   - $1"; }
fail() { echo "FAIL - $1"; failures=$((failures + 1)); }
check() { if [[ "$2" == "$3" ]]; then pass "$1"; else fail "$1: expected [$3], got [$2]"; fi; }

# A fake installation, so the short-circuit can be exercised without a download.
host_arch() {
    case "$(uname -m)" in
        arm64|aarch64) echo aarch64 ;;
        *) echo amd64 ;;
    esac
}

fake_install() {
    local root="$1"
    local home="$root/25.0.4.1-macos-$(host_arch)"
    mkdir -p "$home/bin"
    printf '#!/bin/sh\n' > "$home/bin/native-image"
    chmod +x "$home/bin/native-image"
    echo "$home"
}

# --- an existing installation is reused, and its path is printed ------------
tmp="$(mktemp -d)"
expected_home="$(fake_install "$tmp/nik")"
actual_home="$(NIK_INSTALL_DIR="$tmp/nik" "$install_nik" 2>/dev/null)"
check "reuses an existing installation" "$actual_home" "$expected_home"

# --- rerunning changes nothing: no download, same answer -------------------
before="$(find "$tmp/nik" | sort)"
second_home="$(NIK_INSTALL_DIR="$tmp/nik" "$install_nik" 2>/dev/null)"
after="$(find "$tmp/nik" | sort)"
check "is idempotent (same GRAALVM_HOME)" "$second_home" "$expected_home"
check "is idempotent (tree untouched)" "$after" "$before"

# --- --github-env appends GRAALVM_HOME for later workflow steps ------------
GITHUB_ENV="$tmp/github_env" NIK_INSTALL_DIR="$tmp/nik" "$install_nik" --github-env >/dev/null 2>&1
check "--github-env writes GRAALVM_HOME" "$(cat "$tmp/github_env")" "GRAALVM_HOME=$expected_home"

# --- --github-env without $GITHUB_ENV is an error, not a silent no-op ------
env -u GITHUB_ENV NIK_INSTALL_DIR="$tmp/nik" "$install_nik" --github-env >/dev/null 2>&1
check "--github-env fails without \$GITHUB_ENV" "$?" "1"

# --- unknown flags are rejected with the usage exit code -------------------
NIK_INSTALL_DIR="$tmp/nik" "$install_nik" --wat >/dev/null 2>&1
check "rejects unknown flags" "$?" "2"

rm -rf "$tmp"

if (( failures )); then
    echo "$failures test(s) failed" >&2
    exit 1
fi
echo "all install-nik.sh tests passed"
