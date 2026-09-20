#!/usr/bin/env bash
# Points every git worktree at one shared build directory.
#
# Each worktree otherwise builds into its own target/, and each of those is a full copy of
# every dependency's output. That is how a 349GB volume reached 100% with ten agent
# worktrees open, which broke every build running at the time and is not a failure mode
# worth meeting twice.
#
# Writes a machine local .cargo/config.toml, which is gitignored because the path in it is
# absolute. Run it once per clone, and again after moving the checkout.

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$root/.cargo"

cat > "$root/.cargo/config.toml" <<EOF
# Written by scripts/setup-worktrees.sh. Machine local: the path is absolute.
#
# One build directory for the whole repository, shared by every worktree cut from it.
# Cargo fingerprints on target and feature set, so sharing is safe: builds that differ get
# different fingerprints, builds that match are reused, and a fresh worktree compiles in
# seconds rather than minutes.
[build]
target-dir = "$root/target"
EOF

echo "ok    shared build directory: $root/target"
echo "      every worktree of this repository now builds there"
