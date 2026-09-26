#!/usr/bin/env bash
# The X11 window's loop.
#
# What cannot be seen from this machine, which has no display server in reach: that the loop
# runs until the window closes and does per turn what the loops on the other desktops do.
# The size a frame is drawn at and the refusal of a second frame while one is running are
# checked by WindowFramesTest; what is left is the shape of the loop and the symbols it
# calls, which is what this reads.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
window_source="$repo_root/dioxus-compose-renderer/desktop/c/x11_window.c"
loop_file="$repo_root/dioxus-compose-renderer/desktop/src/X11Window.kt"
frames_file="$repo_root/dioxus-compose-renderer/desktop/src/WindowFrames.kt"
model_file="$repo_root/dioxus-compose-renderer/desktop/src/AppKitWindow.kt"
for file in "$window_source" "$loop_file" "$frames_file" "$model_file"; do
    [[ -f "$file" ]] || { echo "missing $file"; exit 1; }
done
red=0

fail() {
    echo "fail: $1"
    red=1
}

# `! grep ...` cannot fail a script, because the shell ignores a failing command preceded by
# the `!` reserved word. Say it the long way.
absent() {
    local pattern="$1" file="$2" why="$3"
    if grep -Eq -- "$pattern" "$file"; then
        fail "$(basename "$file") still matches '$pattern': $why"
    fi
}

# The loop runs until the window closes. A count of frames is a window that goes away while
# someone is using it, which is what this replaced.
grep -Fq 'while (!isWindowClosed())' "$loop_file" ||
    fail "X11Window.kt does not run until the window closes"
absent 'SPIKE_FRAMES' "$loop_file" "a loop that stops after so many frames is not a loop"
absent 'Thread\.sleep' "$loop_file" \
    "the waiting belongs in the window's own turn, where an event that arrives ends it"

# What a turn of the loop does, taken from the loop the other desktop already has. Each of
# these is a defect that has been found on that one: a window that hears nothing, a list
# whose rows are fetched on a thread with no Host, a screen redrawn sixty times a second
# while an input method is trying to reach the process, a tree nobody was told about.
for call in 'pumpWindowEvents(' 'work.runPending()' 'drainWindowEvents()' \
            'scene.hasInvalidations()' 'semantics.pushIfChanged(afterDrawing'; do
    grep -Fq "$call" "$model_file" ||
        fail "AppKitWindow.kt no longer does '$call', so this test is comparing against nothing"
    grep -Fq "$call" "$loop_file" || fail "the X11 loop does not do '$call' per frame"
done

# Every symbol the X11 path calls has somewhere to land. A shared library links with symbols
# it does not have, so a missing one is not a build failure: it is a window that dies the
# first time the pointer crosses into a text field.
for symbol in dxc_native_window_open dxc_native_window_size dxc_native_frame_begin \
              dxc_native_frame_end dxc_native_poll_event dxc_native_pump \
              dxc_native_window_closed dxc_native_set_cursor \
              dxc_native_set_accessibility; do
    grep -Eq "^(void|int32_t) $symbol\(" "$window_source" ||
        fail "x11_window.c does not define $symbol"
    grep -rqF "@CFunction(\"$symbol\")" "$repo_root/dioxus-compose-renderer/desktop/src" ||
        fail "nothing declares $symbol, so this list has outlived the code"
done

# Where the headers are installed, the window is compiled. Nobody working on this can run it,
# so a type error in it would otherwise be found by whoever builds the renderer.
if cc -fsyntax-only -include X11/Xlib.h -include GL/glx.h -x c /dev/null >/dev/null 2>&1; then
    if ! cc -fsyntax-only -Wall -Wextra "$window_source"; then
        fail "x11_window.c does not compile"
    fi
else
    echo "note: no X11 and GLX headers here, so the window was not compiled"
fi

(( red == 0 )) || exit 1
echo "ok: the X11 loop runs until the window closes and draws when something changed"
