#!/usr/bin/env bash
# Bring the fork up to the latest upstream release and install it everywhere.
#
#   scripts/fork-update.sh                 merge, test, install here, push, install on remotes
#   scripts/fork-update.sh --check         only report what an update would do
#   scripts/fork-update.sh --tag v0.9.6    merge this upstream tag instead of the latest
#   scripts/fork-update.sh --skip-tests    skip cargo nextest
#   scripts/fork-update.sh --no-push       leave origin as it is
#   scripts/fork-update.sh --no-remotes    install only on this machine
#
# On a fork build, `herdr update` runs this script.
#
# Remotes are ssh hosts listed one per line in ~/.config/herdr/fork-remotes (# comments
# allowed); each gets scripts/fork-install.sh --remote <host> --handoff. A host whose
# ~/.local/bin/herdr.fork-commit already names the current commit is skipped, and an
# unreachable host is reported and skipped, so rerunning picks up where it left off.
#
# A merge with conflicts is aborted and the conflicting files listed: merge the tag by
# hand on the fork branch, commit, and rerun; the merged tag is then built and installed.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
branch="${HERDR_FORK_BRANCH:-javan}"
remotes_file="${HERDR_FORK_REMOTES_FILE:-$HOME/.config/herdr/fork-remotes}"
export PATH="$HOME/.cargo/bin:$PATH"
export ZIG="${ZIG:-$HOME/.local/share/zig-0.16.0/zig}"

check=false
tag=""
tests=true
push=true
use_remotes=true
while (($#)); do
  case "$1" in
    --check) check=true ;;
    --tag)
      tag="${2:?--tag needs an upstream tag, e.g. v0.9.6}"
      shift
      ;;
    --skip-tests) tests=false ;;
    --no-push) push=false ;;
    --no-remotes) use_remotes=false ;;
    *) echo "usage: $0 [--check] [--tag <vX.Y.Z>] [--skip-tests] [--no-push] [--no-remotes]" >&2; exit 2 ;;
  esac
  shift
done

say() { printf '[fork-update] %s\n' "$*"; }
die() { printf '[fork-update] %s\n' "$*" >&2; exit 1; }

cd "$repo"

current="$(git branch --show-current)"
[[ "$current" == "$branch" ]] || die "$repo is on '${current:-detached HEAD}', not the fork branch '$branch'"
git diff --quiet HEAD || die "$repo has uncommitted changes; commit or set them aside first"

say "fetching upstream and origin"
git fetch --quiet --tags upstream
git fetch --quiet origin

# Take commits pushed from another checkout first, so the push below cannot be rejected.
if ! git merge-base --is-ancestor "origin/$branch" HEAD; then
  if git merge-base --is-ancestor HEAD "origin/$branch"; then
    if $check; then
      say "origin/$branch has commits this checkout lacks; an update fast-forwards to them first"
    else
      say "fast-forwarding to origin/$branch"
      git merge --quiet --ff-only "origin/$branch"
    fi
  else
    die "$branch and origin/$branch have diverged; reconcile them, then rerun"
  fi
fi

if [[ -z "$tag" ]]; then
  tag="$(git tag -l 'v*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n 1 || true)"
  [[ -n "$tag" ]] || die "found no upstream release tag"
fi
git rev-parse --quiet --verify "$tag^{commit}" >/dev/null || die "no such tag: $tag"

merged=false
if git merge-base --is-ancestor "$tag" HEAD; then
  say "$branch already includes upstream $tag"
elif $check; then
  say "upstream $tag is not merged yet; an update merges it"
else
  say "merging upstream $tag into $branch"
  if ! git merge --quiet --no-ff --no-edit -m "chore: merge upstream $tag into fork" "$tag"; then
    git rev-parse --quiet --verify MERGE_HEAD >/dev/null || die "merging $tag failed before it started; see git's message above"
    conflicts="$(git diff --name-only --diff-filter=U)"
    git merge --abort
    printf '[fork-update] merging %s conflicts in:\n%s\n' "$tag" "$conflicts" >&2
    printf '[fork-update] the merge was aborted; run `git merge %s` in %s, resolve, commit, and rerun\n' "$tag" "$repo" >&2
    exit 3
  fi
  merged=true
fi

commit="$(git rev-parse --short=12 HEAD)"
stamp="$HOME/.local/bin/herdr.fork-commit"
installed="$(cat "$stamp" 2>/dev/null || true)"

hosts=()
if $use_remotes && [[ -f "$remotes_file" ]]; then
  while IFS= read -r line; do
    line="${line%%#*}"
    line="${line//[[:space:]]/}"
    [[ -n "$line" ]] && hosts+=("$line")
  done <"$remotes_file"
fi

remote_commit() {
  ssh -o BatchMode=yes -o ConnectTimeout=10 "$1" 'cat ~/.local/bin/herdr.fork-commit 2>/dev/null || true'
}

if $check; then
  say "this machine: installed ${installed:-unknown}, checkout at $commit"
  for host in ${hosts[@]+"${hosts[@]}"}; do
    if got="$(remote_commit "$host")"; then
      say "$host: installed ${got:-unknown}"
    else
      say "$host: unreachable"
    fi
  done
  ahead="$(git rev-list --count "origin/$branch..HEAD")"
  say "$ahead commit(s) not pushed to origin/$branch"
  exit 0
fi

if $tests && { $merged || [[ "$installed" != "$commit" ]]; }; then
  say "running tests"
  # tests must not reach the herdr session this script may be running in
  if ! env -u HERDR_ENV -u HERDR_SOCKET_PATH -u HERDR_CLIENT_SOCKET_PATH \
    -u HERDR_PANE_ID -u HERDR_TAB_ID -u HERDR_WORKSPACE_ID \
    cargo nextest run --locked --status-level fail --final-status-level fail \
    --failure-output final --success-output never; then
    if $merged; then
      die "tests failed after merging $tag; the merge is committed locally and not pushed (undo it with \`git reset --hard ORIG_HEAD\`)"
    fi
    die "tests failed"
  fi
fi

if [[ "$installed" == "$commit" ]]; then
  say "this machine already runs $commit"
else
  "$repo/scripts/fork-install.sh" --handoff
fi

if $push && [[ -n "$(git rev-list "origin/$branch..HEAD")" ]]; then
  say "pushing $branch to origin"
  git push --quiet origin "$branch"
fi

failed=()
skipped=()
for host in ${hosts[@]+"${hosts[@]}"}; do
  if ! got="$(remote_commit "$host")"; then
    say "$host is unreachable; skipping it (rerun to install there later)"
    skipped+=("$host")
  elif [[ "$got" == "$commit" ]]; then
    say "$host already runs $commit"
  elif ! "$repo/scripts/fork-install.sh" --remote "$host" --handoff; then
    failed+=("$host")
  fi
done

if ((${#skipped[@]})); then say "not updated (unreachable): ${skipped[*]}"; fi
if ((${#failed[@]})); then die "install failed on: ${failed[*]}"; fi
say "fork is at $commit"
