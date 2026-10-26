#!/usr/bin/env bash
# Runs the editor in a window once per scenario and gathers what each run measured.
#
# Usage: run-window.sh <editor-window binary> <output directory> [real file]
#
# Each run is its own process, so the renderer's frame summary (DXC_REPORT_LATENCY, printed
# when DXC_SYNTH's `exit` ends the window) covers one scenario. The probe in the binary
# reports its own numbers as soon as the scenario is over; the renderer ends the window at
# the time given, and the process is killed if it is still there long after that.
#
# A screenshot is taken during every run, so whether the page is drawn at all can be seen.
set -uo pipefail
bin="$1"
out="$2"
real="${3:-}"
mkdir -p "$out"

shot() {
    case "$(uname -s)" in
        Darwin)
            screencapture -x "$out/$1.png" 2>>"$out/screencapture.err" || echo "screencapture failed for $1" >>"$out/screencapture.err"
            ;;
        MINGW* | MSYS* | CYGWIN*)
            powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \$b=[System.Windows.Forms.Screen]::PrimaryScreen.Bounds; \$i=New-Object System.Drawing.Bitmap \$b.Width,\$b.Height; \$g=[System.Drawing.Graphics]::FromImage(\$i); \$g.CopyFromScreen(\$b.Location,[System.Drawing.Point]::Empty,\$b.Size); \$i.Save('$(cygpath -w "$out/$1.png")')" 2>>"$out/screencapture.err" || true
            ;;
    esac
}

# run <label> <scenario> <renderer exit, s> <file> <scroll mode> <DXC_SYNTH> <screenshot at, s>
run() {
    local label="$1" scenario="$2" exit_at="$3" file="$4" mode="$5" synth="$6" shot_at="$7"
    local log="$out/$label.log"
    echo "== $label"
    EDITOR_SCENARIO="$scenario" EDITOR_FILE="$file" EDITOR_SCROLL_MODE="$mode" \
        EDITOR_SPIKE_OUT="$out/$label.md" EDITOR_EXIT_AFTER=$((exit_at + 20)) \
        DXC_SYNTH="$synth" DXC_SYNTH_EXIT_AT="$exit_at" DXC_REPORT_LATENCY=1 DXC_CURSOR_BLINK=0 \
        "$bin" >"$log" 2>&1 &
    local pid=$!
    sleep "$shot_at"
    shot "$label"
    local tries=$(((exit_at + 40) * 2))
    for _ in $(seq "$tries"); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.5
    done
    if kill -0 "$pid" 2>/dev/null; then
        echo "still running after $((exit_at + 40))s, killed" >>"$log"
        kill "$pid" 2>/dev/null
        sleep 1
        kill -9 "$pid" 2>/dev/null
    fi
    wait "$pid" 2>/dev/null
    echo "exit status $?" >>"$log"
}

run idle-top idle 30 "" top exit 8
run page-top page 30 "" top exit 8
run wheel-top wheel 40 "" top exit 8
run type-top type 90 "" top exit 12
run wheel-transform wheel 40 "" transform exit 8
run type-transform type 90 "" transform exit 12
if [[ -n "$real" ]]; then
    run wheel-real wheel 40 "$real" top exit 8
    run type-real type 90 "$real" top exit 12
fi
# The renderer types five letters into the first text field it finds (the textarea), one
# every 700 ms, through the real field and the page's `input` handler.
run realtype-top none 12 "" top "type,exit" 6

summary="$out/summary.md"
{
    echo "## Window runs ($(uname -s))"
    echo
    for log in "$out"/*.log; do
        label="$(basename "$log" .log)"
        echo "### $label"
        echo
        if [[ -f "$out/$label.md" ]]; then
            grep -E '^\|' "$out/$label.md"
            echo
            grep -vE '^\|' "$out/$label.md" | sed '/^$/d' | sed 's/^/    /'
            echo
        else
            echo "The probe did not report: see $label.log."
            echo
        fi
        echo '```'
        grep -E "compose-rust: (latency frames|parity)|editor-spike (event|window: ending)|still running|exit status|panicked|error" "$log" | head -40
        echo '```'
        echo
    done
} >"$summary"
cat "$summary"
