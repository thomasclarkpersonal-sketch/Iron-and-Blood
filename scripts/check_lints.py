#!/usr/bin/env python3
"""Checks that every workspace crate applies the workspace lints.

A crate either inherits them (`[lints] workspace = true`) or copies the table. A crate
copies it when it needs the one permitted exception below, which Cargo can't express
as an override. A copied table must equal [workspace.lints] apart from that exception,
so a lint added to the workspace can't silently skip a crate.

Permitted exception: `unsafe_code` may be "deny" instead of "forbid", only for the
crates in UNSAFE_ALLOWED, whose generated or framework code needs `unsafe`
(pax_protocol: flatc output; pax_godot: godot-rust's entry point). They must allow
it only where needed (D22, D12).
"""

import sys
import tomllib
from pathlib import Path

root = Path(__file__).resolve().parent.parent
workspace = tomllib.loads((root / "Cargo.toml").read_text())
expected = workspace["workspace"]["lints"]
relaxed = {**expected, "rust": {**expected.get("rust", {}), "unsafe_code": "deny"}}
# The only crates allowed the relaxed table (D12, D22). Adding one needs a decision.
UNSAFE_ALLOWED = {"crates/pax_protocol", "crates/pax_godot"}

failures = []
for member in workspace["workspace"]["members"]:
    manifest = root / member / "Cargo.toml"
    lints = tomllib.loads(manifest.read_text()).get("lints")
    if lints is None:
        failures.append(f"{member}: no [lints] table (add `[lints] workspace = true`)")
    elif lints == {"workspace": True}:
        continue
    elif lints == relaxed and member not in UNSAFE_ALLOWED:
        failures.append(f"{member}: relaxes unsafe_code, which only {sorted(UNSAFE_ALLOWED)} may do (D12, D22)")
    elif lints not in (expected, relaxed):
        failures.append(
            f"{member}: copied [lints] differ from [workspace.lints] beyond the permitted "
            f"unsafe_code = \"deny\" exception.\n  crate:     {lints}\n  workspace: {expected}"
        )

if failures:
    print("\n".join(failures), file=sys.stderr)
    sys.exit(1)
print(f"lints: {len(workspace['workspace']['members'])} crates apply the workspace lints")
