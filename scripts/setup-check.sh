#!/usr/bin/env bash
# Usage: ./scripts/setup-check.sh [--quiet]
#
# Preflight check for a development machine. Verifies every tool the build and
# scripts/check.sh need, and prints what is missing together with how to fix it.
# Exits non-zero if anything required is missing.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
renderer_dir="$repo_root/dioxus-compose-renderer"

quiet=0
case "${1:-}" in
    "") ;;
    --quiet) quiet=1 ;;
    *) echo "usage: $0 [--quiet]" >&2; exit 2 ;;
esac

failures=0

ok()   { [[ $quiet -eq 1 ]] || printf 'ok    %s\n' "$1"; }
warn() { printf 'warn  %s\n' "$1" >&2; }
fail() {
    failures=$((failures + 1))
    printf 'FAIL  %s\n' "$1" >&2
    shift
    local line
    for line in "$@"; do printf '        %s\n' "$line" >&2; done
}

# --- Rust -------------------------------------------------------------------
if command -v cargo >/dev/null 2>&1; then
    ok "cargo: $(cargo --version 2>/dev/null)"

    if cargo fmt --version >/dev/null 2>&1; then
        ok "rustfmt: $(cargo fmt --version 2>/dev/null)"
    else
        fail "rustfmt component missing (scripts/check.sh runs 'cargo fmt --all -- --check')" \
             "fix: rustup component add rustfmt"
    fi

    if cargo clippy --version >/dev/null 2>&1; then
        ok "clippy: $(cargo clippy --version 2>/dev/null)"
    else
        fail "clippy component missing (scripts/check.sh runs 'cargo clippy --workspace -- -D warnings')" \
             "fix: rustup component add clippy"
    fi
else
    fail "cargo not found on PATH" \
         "fix: install Rust from https://rustup.rs, then: rustup component add rustfmt clippy" \
         "note: rustup installs into ~/.cargo/bin; make sure it is on PATH"
fi

# --- Platform ---------------------------------------------------------------
uname_s="$(uname -s)"
case "$uname_s" in
    Darwin) ok "platform macOS: the renderer is built and verified here" ;;
    Linux)
        warn "platform Linux: the renderer is scripted (desktop/scripts/build-native-linux.sh)"
        warn "      but has not been run to a working window yet. CI builds it on each release."
        ;;
    *)
        warn "platform $uname_s: no renderer build is scripted here."
        warn "      The Rust workspace and the JVM dev shell (./kotlin run -m desktop) work."
        ;;
esac

# --- The native-image toolchain ---------------------------------------------
# Which toolchain is correct depends on the platform. macOS needs Liberica NIK Full,
# because upstream GraalVM ships no AWT on Darwin (oracle/graal#13272) and the Compose
# renderer cannot link without it. Everywhere else upstream GraalVM is the right one and
# NIK is not required, so demanding it on Linux failed a setup that was in fact correct.
graalvm_home="${GRAALVM_HOME:-}"
if [[ -z "$graalvm_home" && "$uname_s" == "Darwin" ]]; then
    # Same discovery order as dioxus-compose-renderer/desktop/scripts/env.sh.
    for candidate in "$HOME"/Library/Java/JavaVirtualMachines/bellsoft-liberica-vm-full-openjdk25*/Contents/Home; do
        [[ -x "$candidate/bin/native-image" ]] && graalvm_home="$candidate"
    done
fi

if [[ "$uname_s" == "Darwin" ]]; then
    nik_install_hint=(
        "fix: install Liberica NIK 25 Full (Java 25, 'Full' variant, NOT the standard one):"
        "       https://bell-sw.com/pages/downloads/native-image-kit/"
        "     or: brew install --cask liberica-nik-full"
        "expected location: ~/Library/Java/JavaVirtualMachines/bellsoft-liberica-vm-full-openjdk25-*"
        "desktop/scripts/env.sh uses \$GRAALVM_HOME if set, otherwise globs that path."
    )
    missing_message="no Liberica NIK found (GRAALVM_HOME unset and nothing matched the default location)"
else
    nik_install_hint=(
        "fix: install upstream GraalVM for JDK 25 with native-image and set GRAALVM_HOME:"
        "       https://www.graalvm.org/downloads/"
        "Liberica NIK is a macOS requirement only; upstream GraalVM supports AWT here."
    )
    missing_message="GRAALVM_HOME is not set"
fi

if [[ -z "$graalvm_home" ]]; then
    # Not having it is not a broken setup on a machine that only builds the Rust side,
    # which is the whole development loop apart from the renderer itself.
    warn "$missing_message"
    for hint in "${nik_install_hint[@]}"; do
        warn "      $hint"
    done
elif [[ ! -x "$graalvm_home/bin/native-image" ]]; then
    fail "GRAALVM_HOME=$graalvm_home has no bin/native-image" \
         "${nik_install_hint[@]}"
else
    ok "native-image: $graalvm_home/bin/native-image"

    if [[ "$uname_s" == "Darwin" ]]; then
        # Upstream GraalVM skips AWT on Darwin (oracle/graal#13272), so it ships no
        # static AWT archive and the Compose renderer cannot be linked with it.
        awt_archive=""
        for candidate in "$graalvm_home"/lib/static/darwin-*/libawt_lwawt.a; do
            [[ -f "$candidate" ]] && awt_archive="$candidate"
        done
        if [[ -n "$awt_archive" ]]; then
            ok "static AWT archive: $awt_archive"
        else
            fail "$graalvm_home has no lib/static/darwin-*/libawt_lwawt.a" \
                 "This looks like upstream GraalVM (or a non-Full NIK). On macOS it skips" \
                 "AWT entirely (oracle/graal#13272), so the Compose renderer cannot link." \
                 "${nik_install_hint[@]}"
        fi
    fi
fi

# --- Xcode command line tools ----------------------------------------------
if [[ "$uname_s" == "Darwin" ]]; then
    if xcode-select -p >/dev/null 2>&1 && command -v cc >/dev/null 2>&1; then
        ok "Xcode command line tools: $(xcode-select -p 2>/dev/null)"
    else
        fail "Xcode command line tools not found (needed for cc, ld and the AppKit headers)" \
             "fix: xcode-select --install"
    fi
fi

# --- Kotlin Toolchain wrapper ----------------------------------------------
kotlin_wrapper="$renderer_dir/kotlin"
if [[ ! -f "$kotlin_wrapper" ]]; then
    fail "missing $kotlin_wrapper" \
         "The wrapper is checked in; re-clone or restore it from git."
elif [[ ! -x "$kotlin_wrapper" ]]; then
    fail "$kotlin_wrapper is not executable" \
         "fix: chmod +x $kotlin_wrapper"
else
    ok "Kotlin Toolchain wrapper: $kotlin_wrapper (self-bootstrapping, no separate install)"
fi

# --- Result -----------------------------------------------------------------
if [[ $failures -gt 0 ]]; then
    printf '\n%d check(s) failed.\n' "$failures" >&2
    exit 1
fi
[[ $quiet -eq 1 ]] || echo "All checks passed."
