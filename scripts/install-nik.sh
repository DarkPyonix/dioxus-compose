#!/usr/bin/env bash
# Usage: ./scripts/install-nik.sh [--github-env]
#
# Installs Liberica NIK 25 Full, the only toolchain that builds the Compose
# Desktop renderer as a native image on macOS (INTENT D9-macOS, SPEC PR-8:
# upstream GraalVM ships no AWT on Darwin).
#
# The install is addressed by version, so a warm cache -- a developer's home
# directory, or a restored GitHub Actions cache -- makes this a no-op: the
# ~1GB download happens once per NIK version, not once per CI run. That
# idempotency is what scripts/tests/install-nik.test.sh pins down.
#
# The archive URL and its SHA-1 are pinned below. BellSoft publishes both at
# https://api.bell-sw.com/v1/nik/releases; refresh them together when bumping
# NIK_VERSION, and keep the version in step with the developer machines
# described in INTENT D9-macOS.
#
#   --github-env   also append GRAALVM_HOME to $GITHUB_ENV, for CI steps.
#
# Prints the resulting GRAALVM_HOME on stdout; every other message goes to
# stderr, so `GRAALVM_HOME="$(scripts/install-nik.sh)"` works.

set -euo pipefail

NIK_VERSION="25.0.4.1"
NIK_JDK_BUILD="25.0.4.1+2"
NIK_VM_BUILD="25.0.4.1+1"

github_env=0
case "${1:-}" in
    "") ;;
    --github-env) github_env=1 ;;
    *) echo "usage: $0 [--github-env]" >&2; exit 2 ;;
esac

case "$(uname -s)" in
    Darwin) os="macos" ;;
    *)
        echo "error: only macOS needs Liberica NIK today (SPEC PR-8); nothing to install on $(uname -s)" >&2
        exit 1
        ;;
esac

case "$(uname -m)" in
    arm64|aarch64) arch="aarch64"; sha1="d3d655788718174017879fb64b3a91679fcd1c31" ;;
    x86_64) arch="amd64"; sha1="ebffff946560504d19db0a011d0c1281ac3d901c" ;;
    *) echo "error: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

archive="bellsoft-liberica-vm-full-openjdk${NIK_JDK_BUILD}-${NIK_VM_BUILD}-${os}-${arch}.tar.gz"
url="https://github.com/bell-sw/LibericaNIK/releases/download/${NIK_VM_BUILD}-${NIK_JDK_BUILD}/${archive}"

install_root="${NIK_INSTALL_DIR:-$HOME/.cache/dioxus-compose/nik}"
graalvm_home="$install_root/${NIK_VERSION}-${os}-${arch}"

emit() {
    echo "$graalvm_home"
    if (( github_env )); then
        [[ -n "${GITHUB_ENV:-}" ]] || { echo "error: --github-env needs \$GITHUB_ENV" >&2; exit 1; }
        echo "GRAALVM_HOME=$graalvm_home" >> "$GITHUB_ENV"
    fi
}

if [[ -x "$graalvm_home/bin/native-image" ]]; then
    echo "Liberica NIK $NIK_VERSION already installed at $graalvm_home" >&2
    emit
    exit 0
fi

echo "Downloading Liberica NIK $NIK_VERSION ($os/$arch)..." >&2
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl --fail --location --silent --show-error --retry 3 --output "$tmp/$archive" "$url"

echo "Verifying SHA-1..." >&2
actual="$(shasum -a 1 "$tmp/$archive" | cut -d' ' -f1)"
if [[ "$actual" != "$sha1" ]]; then
    echo "error: checksum mismatch for $archive" >&2
    echo "  expected $sha1" >&2
    echo "  actual   $actual" >&2
    exit 1
fi

# The macOS archive is a JDK bundle, so java.home sits a few levels down
# (.../Contents/Home). Locate it rather than guessing at --strip-components,
# then move it into place in a single rename: an interrupted run must never
# leave a half-populated $graalvm_home that the short-circuit above trusts.
mkdir -p "$tmp/stage" "$install_root"
tar -xzf "$tmp/$archive" -C "$tmp/stage"
home_bin="$(find "$tmp/stage" -type f -name native-image -path '*/bin/*' -print -quit)"
[[ -n "$home_bin" ]] || { echo "error: no bin/native-image inside $archive" >&2; exit 1; }
staged_home="$(cd "$(dirname "$home_bin")/.." && pwd)"

rm -rf "$graalvm_home" "$graalvm_home.partial"
mv "$staged_home" "$graalvm_home.partial"
mv "$graalvm_home.partial" "$graalvm_home"

echo "Installed Liberica NIK $NIK_VERSION at $graalvm_home" >&2
emit
