#!/usr/bin/env bash
# Usage: ./scripts/launch-agy.sh <name> <branch> <prompt-file> [model]
#
# Runs one headless agy (Gemini) in its own worktree, detached, with a log you can tail.
#
# The third of the launchers, beside launch-agent.sh for Claude and launch-codex.sh.
# Everything about the worktree, the branch and the build directory is the same; only the
# program differs, so a task can go to whichever has capacity.
#
#   name        directory and log name under the runs directory
#   branch      created from origin/develop if it does not exist, reused if it does
#   prompt-file a file holding the task
#   model       optional, from `agy models`. Defaults to claude-opus-4-6-thinking, because
#               this account's quota is what these runs are here to spend rather than the
#               Claude subscription's. `agy models` also lists the Gemini and GPT-OSS ones.
#
# The runs directory is a sibling of this checkout called agent-runs. Set DXC_AGENT_RUNS
# to put it somewhere else.
#
# Watch one with:   tail -f <runs>/<name>.log
# Stop one with:    kill "$(cat <runs>/<name>.pid)"
#
# Why this exists: an in-session subagent dies whenever the session that started it ends,
# and that has cost this project several hours of finished but unreported work. A detached
# a detached run outlives the session, so the only thing that stops it is finishing.
#
# Note what the permission flag means. `--dangerously-skip-permissions` lets the run
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

if [ "$#" -lt 3 ] || [ "$#" -gt 4 ]; then
    echo "usage: $0 <name> <branch> <prompt-file> [model]" >&2
    exit 2
fi

name="$1"
branch="$2"
prompt_file="$3"
model="${4:-claude-opus-4-6-thinking}"

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
# stream-json, not plain text. With plain `-p` the output arrives in one lump when the
# run finishes, so the log sits at zero bytes for an hour and there is no way to tell a
# working run from a wedged one. Each line here is one event as it happens.
nohup env PATH="$HOME/.cargo/bin:$PATH" \
    agy --print "$prompt" \
        --model "$model" \
        --dangerously-skip-permissions \
        --output-format stream-json \
        --print-timeout 0 \
    < /dev/null >> "$log" 2>&1 &

echo $! > "$runs/$name.pid"
echo "$name started, pid $(cat "$runs/$name.pid")"
echo "  worktree $tree"
echo "  branch   $branch"
echo "  model    $model"
echo "  log      $log"
