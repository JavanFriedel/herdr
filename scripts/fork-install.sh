#!/usr/bin/env bash
# Build this fork and install it over ~/.local/bin/herdr, here or on another machine.
#
#   scripts/fork-install.sh                         build + install locally
#   scripts/fork-install.sh --handoff               ...then hand live panes to the new server
#   scripts/fork-install.sh --remote laptop         build here, install on ssh host "laptop"
#   scripts/fork-install.sh --restore [--remote h]  put the original upstream binary back
#
# Flags combine, e.g. `--restore --remote laptop --handoff`. Remote installs copy the
# local build, so the remote must be the same platform with a compatible glibc.
#
# The first install keeps the upstream binary as herdr.upstream; every install keeps
# the binary it replaced as herdr.prev, and records the built commit in
# herdr.fork-commit. On a fork build, `herdr update` runs scripts/fork-update.sh
# instead of installing the official release. Restart the herdr client (quit and
# run herdr) to load client-side changes; detaching alone keeps the old client code.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:$PATH"
export ZIG="${ZIG:-$HOME/.local/share/zig-0.16.0/zig}"

handoff=false
restore=false
remote=""
while (($#)); do
  case "$1" in
    --handoff) handoff=true ;;
    --restore) restore=true ;;
    --remote)
      remote="${2:?--remote needs an ssh host}"
      shift
      ;;
    *) echo "usage: $0 [--restore] [--handoff] [--remote <ssh-host>]" >&2; exit 2 ;;
  esac
  shift
done

# Runs a script from stdin on the target machine; paths resolve against the target's $HOME.
on_target() {
  if [[ -n "$remote" ]]; then
    ssh "$remote" "bash -s"
  else
    bash -s
  fi
}
where="${remote:-local}"

if ! $restore; then
  cd "$repo"
  commit="$(git rev-parse --short=12 HEAD)"
  if ! git diff --quiet HEAD; then
    commit="$commit-dirty"
  fi
  HERDR_BUILD_COMMIT="$commit" cargo build --release --locked
  built="$repo/target/release/herdr"

  on_target <<'EOF'
set -euo pipefail
target="$HOME/.local/bin/herdr"
mkdir -p "$(dirname "$target")"
if [[ -x "$target" ]]; then
  [[ -e "$target.upstream" ]] || cp "$target" "$target.upstream"
  cp "$target" "$target.prev"
fi
EOF
  if [[ -n "$remote" ]]; then
    scp -q "$built" "$remote:.local/bin/herdr.new"
  else
    cp "$built" "$HOME/.local/bin/herdr.new"
  fi
  {
    printf 'commit=%q\n' "$commit"
    cat <<'EOF'
set -euo pipefail
target="$HOME/.local/bin/herdr"
chmod +x "$target.new"
mv -f "$target.new" "$target"
# scripts/fork-update.sh skips hosts whose recorded commit is already current
printf '%s\n' "$commit" > "$target.fork-commit"
echo "installed $("$target" --version) -> $target"
EOF
  } | on_target
  echo "[$where] fork build $commit"
else
  on_target <<'EOF'
set -euo pipefail
target="$HOME/.local/bin/herdr"
[[ -x "$target.upstream" ]] || { echo "no $target.upstream to restore" >&2; exit 1; }
cp "$target.upstream" "$target.new"
mv -f "$target.new" "$target"
rm -f "$target.fork-commit"
echo "restored upstream $("$target" --version) -> $target"
EOF
fi

if $handoff; then
  on_target <<'EOF'
set -euo pipefail
target="$HOME/.local/bin/herdr"
# the default session's server; a stopped one starts on the new binary next time
if ! pgrep -u "$(id -u)" -f '(^|/)herdr server( |$)' >/dev/null; then
  echo "no running herdr server to hand off"
  exit 0
fi
"$target" server live-handoff --import-exe "$target"
echo "server handed off"
EOF
fi
echo "[$where] quit and rerun herdr on that machine to load the new client"
