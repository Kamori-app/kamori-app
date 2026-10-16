#!/usr/bin/env python3
"""Read-only structural checks for the bounded E2EE V&V document set."""
from pathlib import Path
import hashlib
import re
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parents[4]
SPEC = ROOT / "docs/superpowers/specs/2026-10-16-e2ee-trust-boundaries-design.md"
REVIEWS = ROOT / "docs/security/design-reviews"
FILES = [
    SPEC,
    REVIEWS / "2026-10-16-e2ee-validation-verification.md",
    REVIEWS / "2026-10-16-e2ee-security-validation.md",
    REVIEWS / "2026-10-16-e2ee-lifecycle-verification.md",
    REVIEWS / "2026-10-16-e2ee-traceability.md",
    Path(__file__).resolve().parent / "README.md",
]
errors = []
checked_links = 0
for path in FILES:
    body = path.read_text(encoding="utf-8")
    for number, line in enumerate(body.splitlines(), 1):
        if line.rstrip() != line:
            errors.append(f"{path.relative_to(ROOT)}:{number}: trailing whitespace")
    if re.search(r"\b(?:TODO|TBD|FIXME)\b", body):
        errors.append(f"{path.relative_to(ROOT)}: unresolved placeholder")
    if not body.endswith("\n"):
        errors.append(f"{path.relative_to(ROOT)}: missing final newline")
    if sum(line.startswith("```") for line in body.splitlines()) % 2:
        errors.append(f"{path.relative_to(ROOT)}: unmatched fenced block")
    for target in re.findall(r"!?(?:\[[^\]\n]+\])\(([^)]+)\)", body):
        target = target.strip().strip("<>")
        if target.startswith(("https://", "http://", "mailto:", "#")):
            continue
        target = unquote(target.split("#", 1)[0])
        checked_links += 1
        if not (path.parent / target).is_file():
            errors.append(f"{path.relative_to(ROOT)}: missing link {target}")

spec = SPEC.read_text(encoding="utf-8")
acceptance = re.findall(r"^- \*\*(P[0-9]{2}):\*\*", spec, re.MULTILINE)
expected = [f"P{number:02}" for number in range(1, 17)]
if acceptance != expected:
    errors.append(f"acceptance IDs: expected {expected}, got {acceptance}")
for marker in ("VV-S1", "VV-S2", "VV-L1", "VV-L2", "VV-L3",
               "VV-T1", "VV-T5", "VV-R1"):
    if marker not in spec:
        errors.append(f"missing normative disposition {marker}")
if errors:
    raise SystemExit("\n".join(errors))
print(f"PASS: {len(FILES)} UTF-8 Markdown files; {checked_links} local file links")
print("PASS: whitespace, fenced blocks, placeholders and P01-P16 identifiers")
print("PASS: normative finding markers present")
print(f"SPEC SHA256: {hashlib.sha256(SPEC.read_bytes()).hexdigest()}")
print("Scope: structural document checks, not protocol or product verification")
