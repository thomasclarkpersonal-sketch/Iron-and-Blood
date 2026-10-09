#!/usr/bin/env bash
# Installs the sdlc-overnight workflow template into a git repository.
#
#   install.sh <target-repo>           # workflow + starter files the repo doesn't have yet
#   install.sh --user <target-repo>    # workflow into ~/.claude/workflows (every repo), starter files into the target
#   install.sh --agents <target-repo>  # also add an AGENTS.md starter if the repo has none
#   install.sh --check <target-repo>   # exit 1 if the installed workflow differs from this template's
#
# The workflow script is always replaced: it is the template's file, and every
# per-project choice lives in .claude/sdlc-overnight.json. Every other file is
# a starting point for you to edit, so it is copied only if it doesn't exist.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
user=false agents=false check=false
while [[ $# -gt 0 && "$1" == --* ]]; do
  case "$1" in
    --user) user=true ;;
    --agents) agents=true ;;
    --check) check=true ;;
    *) echo "unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
[[ $# -eq 1 ]] || { sed -n '2,10p' "${BASH_SOURCE[0]}"; exit 2; }
target="$(cd "$1" && git rev-parse --show-toplevel)"

if $user; then workflow_dir="$HOME/.claude/workflows"; else workflow_dir="$target/.claude/workflows"; fi
workflow="$workflow_dir/sdlc-overnight.js"

if $check; then
  if cmp -s "$here/workflow/sdlc-overnight.js" "$workflow"; then
    echo "OK: $workflow matches the template."
    exit 0
  fi
  echo "$workflow differs from $here/workflow/sdlc-overnight.js; run install.sh again to update it." >&2
  exit 1
fi

mkdir -p "$workflow_dir"
cp "$here/workflow/sdlc-overnight.js" "$workflow"
echo "installed  $workflow"

starter() { # starter <source> <destination>: copy unless the destination exists
  if [[ -e "$2" ]]; then
    echo "kept       $2 (already there)"
  else
    mkdir -p "$(dirname "$2")"
    cp "$1" "$2"
    echo "created    $2"
  fi
}
starter "$here/config/sdlc-overnight.example.json" "$target/.claude/sdlc-overnight.json"
starter "$here/commands/critic.md" "$target/.claude/commands/critic.md"
starter "$here/commands/critic-followup.md" "$target/.claude/commands/critic-followup.md"
starter "$here/docs/SDLC_WORKFLOW.md" "$target/docs/SDLC_WORKFLOW.md"
if $agents; then starter "$here/docs/AGENTS.template.md" "$target/AGENTS.md"; fi

# The run directory holds the log and the report; it must never be committed.
run_dir="$(sed -n 's/^[[:space:]]*"runDir"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$target/.claude/sdlc-overnight.json" | head -n1)"
run_root="${run_dir%%/*}"
run_root="${run_root:-.sdlc-runs}"
if git -C "$target" check-ignore -q "$run_root/probe"; then
  echo "ignored    $run_root/ (already in .gitignore)"
else
  printf '\n# sdlc-overnight run logs and reports\n/%s/\n' "$run_root" >> "$target/.gitignore"
  echo "ignored    $run_root/ (added to .gitignore)"
fi

cat <<EOF

Next:
  1. Edit $target/.claude/sdlc-overnight.json: at least "project" and "gate.full"
     (every key is explained in $here/config-reference.md).
  2. Read the copied critic commands and docs/SDLC_WORKFLOW.md, and adapt them.
  3. Optional: node $here/test/run-tests.mjs "" $target/.claude/sdlc-overnight.json
  4. In Claude Code, in $target: run the sdlc-overnight workflow with {"request": "...", "slug": "...", "planOnly": true}
EOF
