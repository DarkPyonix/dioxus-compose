#!/usr/bin/env bash
# Usage: ./scripts/apply-patches.sh
#
# Builds the patched copy of blitz-dom that the workspace's [patch.crates-io] entry points
# at: .scratch/patched/blitz-dom-0.2.4. Run it once after a fresh clone and again whenever
# a file under scripts/patches/ changes. Until it has run, cargo and rust-analyzer fail with
# "failed to load source for dependency `blitz-dom`".
#
# What it does:
#   1. downloads the blitz-dom crate from crates.io;
#   2. checks its sha256 against the checksum crates.io published for that version, which
#      is the one Cargo.lock held before the dependency became a path dependency (a path
#      dependency has no checksum line, so the value lives here);
#   3. extracts it and applies every scripts/patches/blitz-dom-<version>/*.patch in order
#      with `patch -p1`.
#
# It is idempotent: a marker file holds the hash of the patch set, and the copy is
# extracted afresh only when that hash differs or the copy is missing.
#
# Needs bash, curl, tar, patch and sha256sum or shasum, all of which macOS, Linux and Git
# Bash provide.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

crate=blitz-dom
version=0.2.4
checksum=c81d0f520bad6798a54cc4360bcabd245dc0aba2ce5917217d1d82ac5ee6bfed
patch_dir="$repo_root/scripts/patches/$crate-$version"
work="$repo_root/.scratch/patched"
target="$work/$crate-$version"
marker="$target/.patch-set-hash"

# sha256 of stdin, as a bare hex string.
sha256_stdin() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum | awk '{print $1}'
    else
        shasum -a 256 | awk '{print $1}'
    fi
}

shopt -s nullglob
patches=("$patch_dir"/*.patch)
shopt -u nullglob
if [[ ${#patches[@]} -eq 0 ]]; then
    echo "apply-patches: no patches in $patch_dir" >&2
    exit 1
fi

# The file names and contents, in a fixed order, so renaming or reordering a patch counts.
want="$(
    for file in "${patches[@]}"; do
        basename "$file"
        cat "$file"
    done | sha256_stdin
)"

if [[ -f "$marker" && "$(cat "$marker")" == "$want" ]]; then
    echo "apply-patches: $crate-$version is already patched"
    exit 0
fi

mkdir -p "$work"
archive="$work/$crate-$version.crate"
if [[ ! -f "$archive" || "$(sha256_stdin < "$archive")" != "$checksum" ]]; then
    echo "apply-patches: downloading $crate-$version"
    curl -fsSL "https://crates.io/api/v1/crates/$crate/$version/download" -o "$archive"
fi
actual="$(sha256_stdin < "$archive")"
if [[ "$actual" != "$checksum" ]]; then
    rm -f "$archive"
    echo "apply-patches: $crate-$version has sha256 $actual, expected $checksum" >&2
    exit 1
fi

# Extract beside the target and move it into place, so an interrupted run leaves no
# half-patched copy that looks finished: the marker is written last.
staging="$work/staging"
rm -rf "$staging" "$target"
mkdir -p "$staging"
tar -xzf "$archive" -C "$staging"
mv "$staging/$crate-$version" "$target"
rmdir "$staging"

for file in "${patches[@]}"; do
    echo "apply-patches: applying $(basename "$file")"
    (cd "$target" && patch -p1 < "$file")
done

printf '%s\n' "$want" > "$marker"
echo "apply-patches: $crate-$version patched at $target"
