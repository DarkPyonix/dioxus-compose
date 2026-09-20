#!/usr/bin/env bash
# Usage: ./scripts/tests/setup-check.test.sh
#
# Exercises scripts/setup-check.sh. The happy path asserts that this machine is
# actually set up; the failure cases run the script against a synthetic HOME and
# PATH so that each "missing tool" branch is covered without touching the machine.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
setup_check="$repo_root/scripts/setup-check.sh"

tests_run=0
tests_failed=0

# assert_run <name> <expected status> <expected substring|-> <command...>
assert_run() {
    local name="$1" expected_status="$2" expected_substring="$3"
    shift 3
    tests_run=$((tests_run + 1))
    local output status
    output="$("$@" 2>&1)"
    status=$?
    if [[ "$status" != "$expected_status" ]]; then
        tests_failed=$((tests_failed + 1))
        printf 'FAIL %s: expected exit %s, got %s\n%s\n' "$name" "$expected_status" "$status" "$output" >&2
        return
    fi
    if [[ "$expected_substring" != "-" && "$output" != *"$expected_substring"* ]]; then
        tests_failed=$((tests_failed + 1))
        printf 'FAIL %s: output did not contain %q\n%s\n' "$name" "$expected_substring" "$output" >&2
        return
    fi
    printf 'ok   %s\n' "$name"
}

# This machine is a configured development machine, so the real run must pass.
assert_run "setup_check_passes_on_this_machine" 0 "All checks passed." \
    "$setup_check"

assert_run "setup_check_quiet_mode_exits_zero" 0 "-" \
    "$setup_check" --quiet

assert_run "setup_check_rejects_unknown_argument" 2 "usage:" \
    "$setup_check" --nonsense

# No cargo on PATH, and an empty HOME so the NIK glob finds nothing.
real_home="$HOME"
empty_home="$(mktemp -d)"
trap 'rm -rf "$empty_home"' EXIT

assert_run "setup_check_reports_missing_cargo" 1 "cargo not found on PATH" \
    env -u GRAALVM_HOME PATH=/usr/bin:/bin HOME="$empty_home" "$setup_check"

# Which toolchain is missing, and whether its absence is fatal, depends on the platform.
# macOS cannot build the renderer without Liberica NIK Full, so that is an error. Anywhere
# else upstream GraalVM is the right toolchain and not having it only blocks the renderer,
# which is why it warns and exits zero: the Rust side is the whole loop apart from that.
if [[ "$(uname -s)" == "Darwin" ]]; then
    assert_run "setup_check_reports_missing_nik" 1 "no Liberica NIK found" \
        env -u GRAALVM_HOME HOME="$empty_home" "$setup_check"
else
    # Keep rustup discoverable. The empty HOME above exists to make the macOS toolchain
    # glob find nothing, and that glob is not used here, but rustup does live under HOME,
    # so blanking it would report missing rustfmt and clippy that are installed.
    assert_run "setup_check_warns_about_a_missing_graalvm" 0 "GRAALVM_HOME is not set" \
        env -u GRAALVM_HOME HOME="$empty_home" \
            RUSTUP_HOME="${RUSTUP_HOME:-$real_home/.rustup}" \
            CARGO_HOME="${CARGO_HOME:-$real_home/.cargo}" \
            "$setup_check"
fi

# A GRAALVM_HOME that exists but has no native-image at all. Wrong on every platform:
# the variable was set, so someone meant to point at a toolchain.
bogus_home="$empty_home/bogus-jdk"
mkdir -p "$bogus_home/bin"
assert_run "setup_check_reports_graalvm_home_without_native_image" 1 "has no bin/native-image" \
    env GRAALVM_HOME="$bogus_home" "$setup_check"

# A GraalVM-shaped install with native-image but no static AWT archive: this is
# what upstream GraalVM looks like on macOS (oracle/graal#13272).
fake_graal="$empty_home/fake-graalvm"
mkdir -p "$fake_graal/bin" "$fake_graal/lib/static/darwin-aarch64"
printf '#!/bin/sh\nexit 0\n' > "$fake_graal/bin/native-image"
chmod +x "$fake_graal/bin/native-image"
if [[ "$(uname -s)" == "Darwin" ]]; then
    assert_run "setup_check_rejects_plain_graalvm_on_macos" 1 "libawt_lwawt.a" \
        env GRAALVM_HOME="$fake_graal" "$setup_check"

    # Adding the archive is what makes a NIK Full install acceptable.
    : > "$fake_graal/lib/static/darwin-aarch64/libawt_lwawt.a"
    assert_run "setup_check_accepts_nik_full_shaped_install" 0 "static AWT archive" \
        env GRAALVM_HOME="$fake_graal" "$setup_check"
fi

printf '\n%d test(s), %d failure(s)\n' "$tests_run" "$tests_failed"
[[ $tests_failed -eq 0 ]]
