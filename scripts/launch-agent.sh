#!/usr/bin/env bash
# Usage: ./scripts/launch-agent.sh <name> <branch> <prompt-file>
#
# Runs one headless Claude in its own worktree, detached, with a log you can tail.
#
#   name        directory and log name under the runs directory
#   branch      created from origin/develop if it does not exist, reused if it does
#   prompt-file a file holding the task
#
# The runs directory is a sibling of this checkout called agent-runs. Set DXC_AGENT_RUNS
# to put it somewhere else.
#
# Watch one with:   tail -f <runs>/<name>.log
# Stop one with:    kill "$(cat <runs>/<name>.pid)"
#
# Why this exists: an in-session subagent dies whenever the session that started it ends,
# and that has cost this project several hours of finished but unreported work. A detached
# `claude -p` outlives the session, so the only thing that stops it is finishing.
#
# Note what the permission flag means. `--permission-mode bypassPermissions` lets the run
# edit files, run cargo and push its branch without asking, which is the whole point of an
# unattended run, and also means nothing stands between it and the repository. It works on
# its own branch in its own worktree and never on develop, release or main, which is the
# containment this relies on.
#
# This script does not write a cargo config. It used to point every worktree at the main
# checkout's build directory to save disk, and that is how one checkout's build came to
# run, and overwrite, another checkout's files. scripts/setup-worktrees.sh owns that
# question now and this defers to it, so there is one answer rather than two.
set -euo pipefail

if [ "$#" -ne 3 ]; then
    echo "usage: $0 <name> <branch> <prompt-file>" >&2
    exit 2
fi

name="$1"
branch="$2"
prompt_file="$3"

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runs="${DXC_AGENT_RUNS:-$(dirname "$repo")/agent-runs}"
tree="$runs/$name"
log="$runs/$name.log"

[ -f "$prompt_file" ] || { echo "error: no prompt file at $prompt_file" >&2; exit 1; }
prompt="$(cat "$prompt_file")"
mkdir -p "$runs"

cd "$repo"
git fetch -q origin

if [ ! -d "$tree" ]; then
    if git show-ref -q --verify "refs/heads/$branch"; then
        git worktree add -q "$tree" "$branch"
    else
        git worktree add -q -b "$branch" "$tree" origin/develop
    fi
fi

# Every worktree builds into its own target/. Two checkouts sharing one build directory
# run each other's binaries, because cargo leaves the package path out of the unit hash
# for a path package and identical sources in two places become one cache entry.
#
# --all, so this runs against the new worktree as well as the rest. The branch it was cut
# from may predate this script, and a worktree left over from the shared build directory
# still carries the config that pointed it elsewhere.
"$repo/scripts/setup-worktrees.sh" --all

cd "$tree"
: > "$log"
# The working directory is named in the prompt as well as entered here. Two runs started
# with a launcher like this one did their work in the main checkout instead of the
# worktree they were given: they created the branch there, switched it twice while
# another worker was committing, and left that worker's two commits on two different
# unrelated branches. cd alone was not enough, so the path is stated where the run can
# read it.
prompt="Your working directory is $tree, and you are already in it. Every command you run, every file you edit and every git operation happens there. Never run a command against $repo, never cd out of your worktree, and never switch the branch of any checkout but your own: another worker is committing in that one. If a path you want is not under $tree, you are in the wrong place.

$prompt"
# stream-json, not plain text. With plain `-p` the output arrives in one lump when the
# run finishes, so the log sits at zero bytes for an hour and there is no way to tell a
# working run from a wedged one. Each line here is one event as it happens.
nohup env PATH="$HOME/.cargo/bin:$PATH" \
    claude -p "$prompt" \
        --permission-mode bypassPermissions \
        --verbose \
        --output-format stream-json \
    >> "$log" 2>&1 &

echo $! > "$runs/$name.pid"
echo "$name started, pid $(cat "$runs/$name.pid")"
echo "  worktree $tree"
echo "  branch   $branch"
echo "  log      $log"
