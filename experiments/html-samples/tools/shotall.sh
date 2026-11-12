#!/usr/bin/env bash
# Usage: experiments/html-samples/tools/shotall.sh <outdir> <sample>...
#
# Starts each sample's debug binary, waits for its first frames, captures its own main
# window (by window id, so nothing else on the screen is photographed) and stops it by
# the process id it started. Build the binaries first with
# `cargo build --bin sample-html-<name>`, and the window lister with
# `swiftc -O tools/winid.swift -o <outdir>/winid`.
set -uo pipefail
out=$1; shift
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
mkdir -p "$out"
for s in "$@"; do
  "$repo/target/debug/sample-html-$s" > "$out/$s.log" 2>&1 &
  pid=$!
  sleep 7
  # The largest on-screen window of that process is the page's window.
  id=$("$out/winid" "$pid" | awk '{print $1, $2*$3}' | sort -k2 -n -r | head -1 | cut -d' ' -f1)
  if [ -n "$id" ]; then screencapture -o -x -l "$id" "$out/$s.png" && echo "$s ok"; else echo "$s: no window"; fi
  kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
done
