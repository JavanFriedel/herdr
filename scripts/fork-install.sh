#!/usr/bin/env bash
# Build this fork and install it over ~/.local/bin/herdr.
#
#   scripts/fork-install.sh             build + install; restart the server yourself
#   scripts/fork-install.sh --handoff   build + install, then hand live panes to the new server
#   scripts/fork-install.sh --restore   put the original upstream binary back
#
# The first install keeps the upstream binary as herdr.upstream; every install keeps
# the binary it replaced as herdr.prev. Do not run `herdr update` on this install: it
# replaces the fork build with the official release.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${HERDR_FORK_BIN_DIR:-$HOME/.local/bin}"
target="$bin_dir/herdr"
export PATH="$HOME/.cargo/bin:$PATH"
export ZIG="${ZIG:-$HOME/.local/share/zig-0.16.0/zig}"

handoff=false
case "${1:-}" in
  --handoff) handoff=true ;;
  --restore)
    [[ -x "$target.upstream" ]] || { echo "no $target.upstream to restore" >&2; exit 1; }
    cp "$target.upstream" "$target.new" && mv -f "$target.new" "$target"
    echo "restored upstream: $("$target" --version)"
    exit 0
    ;;
  "") ;;
  *) echo "usage: $0 [--handoff|--restore]" >&2; exit 2 ;;
esac

cd "$repo"
commit="$(git rev-parse --short=12 HEAD)"
if ! git diff --quiet HEAD; then
  commit="$commit-dirty"
fi
HERDR_BUILD_COMMIT="$commit" cargo build --release --locked

mkdir -p "$bin_dir"
if [[ -x "$target" ]]; then
  [[ -e "$target.upstream" ]] || cp "$target" "$target.upstream"
  cp "$target" "$target.prev"
fi
cp target/release/herdr "$target.new"
mv -f "$target.new" "$target"
echo "installed $("$target" --version) ($commit) -> $target"

if $handoff; then
  "$target" server live-handoff --import-exe "$target"
  echo "server handed off; detach (prefix+q) and run herdr to load the new client"
else
  echo "run '$target server live-handoff --import-exe $target' or restart the server to use it,"
  echo "then detach (prefix+q) and run herdr to load the new client"
fi
