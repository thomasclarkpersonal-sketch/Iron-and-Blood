#!/usr/bin/env bash
# Renders the Mermaid diagrams in the docs with mermaid-cli in Docker, and fails on any
# diagram that doesn't parse. GitHub shows a broken diagram as an error box, so render
# before pushing a change to one.
#
#   scripts/render-mermaid.sh                     # every tracked .md with a diagram
#   scripts/render-mermaid.sh docs/ARCHITECTURE.md
#   scripts/render-mermaid.sh --format png docs/DATA_MODEL_M5_M6.md
#
# Output goes to out/mermaid/ (git-ignored): one SVG (or PNG) per diagram, plus a copy of
# each Markdown file that links its images, for reading the docs as rendered.
#
# The image is pinned by digest, like flatc in gen-protocol.sh, so a diagram that renders
# today renders the same way tomorrow. mermaid-cli 11.4.2 bundles Mermaid 11.4.1; GitHub
# runs its own Mermaid version, which can differ in rare syntax.
set -euo pipefail

IMAGE="ghcr.io/mermaid-js/mermaid-cli/mermaid-cli@sha256:99c983b3ab4e14033f2880bc1b9de17e5090b4515dabd63fe9cf8c0ae6130956" # 11.4.2

format=svg
if [[ "${1:-}" == "--format" ]]; then
  format="$2"
  shift 2
fi
case "$format" in svg | png | pdf) ;; *) echo "--format must be svg, png or pdf" >&2; exit 2 ;; esac

root="$(git rev-parse --show-toplevel)"
cd "$root"

if [[ $# -gt 0 ]]; then
  files=("$@")
else
  mapfile -t files < <(git ls-files -z '*.md' | xargs -0 grep -l '^```mermaid' || true)
fi
if [[ ${#files[@]} -eq 0 ]]; then
  echo "No Markdown files with Mermaid diagrams."
  exit 0
fi

mkdir -p out/mermaid
failed=()
for file in "${files[@]}"; do
  dir="out/mermaid/$(dirname "$file")"
  mkdir -p "$dir"
  # The repository is mounted read-only; only out/mermaid is writable. The container runs
  # as the caller, so the output belongs to them.
  if ! docker run --rm --user "$(id -u):$(id -g)" \
    -v "$root:/data:ro" -v "$root/out/mermaid:/data/out/mermaid" \
    "$IMAGE" --quiet -e "$format" -i "$file" -o "$dir/$(basename "$file")"; then
    failed+=("$file")
  fi
done

if [[ ${#failed[@]} -gt 0 ]]; then
  echo "::error::Mermaid diagrams failed to render in: ${failed[*]}" >&2
  exit 1
fi
echo "Rendered ${#files[@]} file(s) to out/mermaid/ as $format."
