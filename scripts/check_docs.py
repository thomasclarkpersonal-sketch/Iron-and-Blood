#!/usr/bin/env python3
"""Checks that the documentation still describes the repository (docs/README.md).

The critic guards DECISIONS.md; nothing else stops the descriptive docs from drifting
away from the code. This script checks the facts that drift silently:

1. Every relative Markdown link resolves to a file, and every `#anchor` to a heading
   of the linked file (GitHub's anchor rules).
2. Every decision cited as `D<n>` in the docs or the Rust sources has an entry in
   DECISIONS.md, so a citation can't point at a decision that was never written.
3. Lists that name parts of the code name all of them:
   - every crate under `crates/` in README.md, ARCHITECTURE.md and BACKEND_SCHEMA.md;
   - every engine system module in D4's neighbours: ARCHITECTURE.md's game loop and
     BACKEND_SCHEMA.md's workspace tree;
   - every `pax_cli` command in ONBOARDING.md's code map and BACKEND_SCHEMA.md's tree.

It needs nothing beyond the standard library and git.
"""

import functools
import re
import subprocess
import sys
import unicodedata
from pathlib import Path

root = Path(__file__).resolve().parent.parent
failures = []


def tracked(*patterns):
    out = subprocess.run(
        ["git", "ls-files", "--", *patterns], cwd=root, capture_output=True, text=True, check=True
    ).stdout
    return [root / line for line in out.splitlines() if (root / line).exists()]


def prose(text):
    """The text with fenced code blocks and inline code removed (links there are examples)."""
    text = re.sub(r"^```.*?^```", "", text, flags=re.S | re.M)
    return re.sub(r"`[^`\n]*`", "", text)


def slug(heading):
    """GitHub's anchor for a heading: lower case, punctuation dropped, spaces to hyphens."""
    heading = heading.strip().lower()
    kept = "".join(
        ch for ch in heading
        if ch.isalnum() or ch in " -_" or unicodedata.category(ch).startswith("M")
    )
    return kept.replace(" ", "-")


@functools.cache
def anchors(path):
    seen, result = {}, set()
    for line in re.sub(r"^```.*?^```", "", path.read_text(), flags=re.S | re.M).splitlines():
        m = re.match(r"#{1,6}\s+(.*?)\s*#*\s*$", line)
        if not m:
            continue
        base = slug(m.group(1))
        n = seen.get(base, 0)
        seen[base] = n + 1
        result.add(base if n == 0 else f"{base}-{n}")
    return result


# 1. Links and anchors.
LINK = re.compile(r"!?\[[^\]]*\]\(\s*<?([^)\s>]+)>?(?:\s+\"[^\"]*\")?\s*\)")
markdown = tracked("*.md")
for doc in markdown:
    for target in LINK.findall(prose(doc.read_text())):
        if re.match(r"[a-z][a-z0-9+.-]*:", target):  # http:, https:, mailto:
            continue
        path_part, _, fragment = target.partition("#")
        dest = (doc.parent / path_part).resolve() if path_part else doc
        where = doc.relative_to(root)
        if not dest.exists():
            failures.append(f"{where}: link to missing file {target}")
        elif fragment and dest.suffix == ".md" and fragment not in anchors(dest):
            failures.append(f"{where}: link to missing heading {target}")

# 2. Decision citations.
def rust_comment(line):
    """The comment on a line of Rust: after a `//` that isn't inside a string literal."""
    for m in re.finditer(r"//", line):
        if line[: m.start()].count('"') % 2 == 0:
            return line[m.end():]
    return ""


decisions_text = (root / "docs/DECISIONS.md").read_text()
decided = {int(n) for n in re.findall(r"^## D(\d+)\.", decisions_text, flags=re.M)}
for source in markdown + tracked("*.rs"):
    if "generated" in source.parts:
        continue
    text = source.read_text()
    if source.suffix == ".rs":  # only comments cite decisions; code may name a `D3`
        text = "\n".join(rust_comment(line) for line in text.splitlines())
    cited = {int(n) for n in re.findall(r"\bD(\d{1,3})\b", text)}
    for n in sorted(cited - decided):
        failures.append(f"{source.relative_to(root)}: cites D{n}, which DECISIONS.md has no entry for")

# 3. Lists that name parts of the code.
def require(doc, names, what):
    text = (root / doc).read_text()
    for name in sorted(names):
        if not re.search(rf"\b{re.escape(name)}\b", text):
            failures.append(f"{doc}: doesn't mention {what} `{name}`")


crates = {p.name for p in (root / "crates").iterdir() if (p / "Cargo.toml").exists()}
for doc in ("README.md", "docs/ARCHITECTURE.md", "docs/BACKEND_SCHEMA.md"):
    require(doc, crates, "the crate")

systems = {
    p.stem for p in (root / "crates/pax_engine/src/systems").glob("*.rs") if p.stem != "mod"
}
require("docs/ARCHITECTURE.md", {f"systems/{s}.rs" for s in systems}, "the system module")
# The workspace tree is BACKEND_SCHEMA's first fenced block; prose elsewhere may
# mention `systems/` too.
schema = (root / "docs/BACKEND_SCHEMA.md").read_text()
tree_block = re.search(r"^```text\n(.*?)^```", schema, flags=re.S | re.M)
tree = tree_block.group(1) if tree_block else ""
if not tree:
    failures.append("docs/BACKEND_SCHEMA.md: no ```text workspace tree found (update check_docs.py)")
systems_line = re.search(r"systems/\s+(.*)", tree)
listed = set(re.findall(r"\w+", systems_line.group(1))) if systems_line else set()
for s in sorted(systems - listed):
    failures.append(f"docs/BACKEND_SCHEMA.md: the workspace tree's systems/ line doesn't list `{s}`")

cli = (root / "crates/pax_cli/src/main.rs").read_text()
commands = set(re.findall(r'^\s*"([a-z]+)" => Cmd::', cli, flags=re.M))
if not commands:
    failures.append("crates/pax_cli/src/main.rs: found no commands (update check_docs.py)")
require("docs/ONBOARDING.md", commands, "the pax_cli command")
cli_line = re.search(r"pax_cli/\s+(.*)", tree)
listed = set(re.findall(r"\w+", cli_line.group(1))) if cli_line else set()
for c in sorted(commands - listed):
    failures.append(f"docs/BACKEND_SCHEMA.md: the workspace tree's pax_cli/ line doesn't list `{c}`")

if failures:
    print("Documentation is out of step with the repository (docs/README.md):")
    for f in failures:
        print(f"  {f}")
    sys.exit(1)
print(f"Docs OK: {len(markdown)} Markdown files, {len(decided)} decisions.")
