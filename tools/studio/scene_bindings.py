"""Export source-proven scene references over the canonical field opcode forms.

This is not an opcode registry. Unmatched current-PC readers stay unknown;
structural edits must not guess their binding. Runtime selectors remain symbolic.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from dataclasses import asdict
from pathlib import Path

from tools.analysis import event_access, events

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "src/runtime/crates/xem-studio/data/field-scene-bindings.json"
SOURCE = "src/decomp/src/field/field_event.c"


def number(value: int) -> str:
    return rf"(?:{value}|0[xX]0*{value:x}|0[xX]0*{value:X})"


def metadata() -> dict:
    source = (ROOT / SOURCE).read_text()
    forms = [
        form for values in (*events.PRIMARY.values(), *events.EXTENDED.values()) for form in values
    ]
    actor_facts: dict[tuple[str, int], dict] = {}
    zone_facts: dict[str, dict] = {}
    macros = (
        "#define EVENT_ZONE (field_trigger_zones[EVENT_OPERAND_BYTE(1)])",
        "#define CODE_ZONE(code) (field_trigger_zones[(code)[1]])",
    )
    if any(macro not in source for macro in macros):
        raise ValueError("zone index macros changed; review source reference semantics")
    macro_hash = hashlib.sha256("\n".join(macros).encode()).hexdigest()
    for form in forms:
        body = event_access.function(source, form.handler)
        for operand in form.operands:
            if operand.kind != "actor":
                continue
            key = (form.handler, operand.offset)
            if key in actor_facts:
                for other in form.operands:
                    if (
                        other != operand
                        and other.offset
                        <= operand.offset
                        < other.offset + events.WIDTHS[other.kind]
                    ):
                        value = asdict(other)
                        if value not in actor_facts[key]["aliases"]:
                            actor_facts[key]["aliases"].append(value)
                continue
            k = number(operand.offset)
            pattern = rf"(field_event_read_actor_index(?:_or_leader)?)\(\s*{k}\s*\)"
            candidates = [(form.handler, body)]
            delegate = event_access.DELEGATES.get(form.handler)
            if delegate and f"{delegate}(" in body:
                candidates.append((delegate, event_access.function(source, delegate)))
            proofs = [
                event_access.evidence(SOURCE, name, candidate, helper)
                for name, candidate in candidates
                for helper in sorted({match.group(1) for match in re.finditer(pattern, candidate)})
            ]
            raw_pattern = rf"field_view\.components\.descriptors\[EVENT_OPERAND_BYTE\(\s*{k}\s*\)\]"
            raw = bool(re.search(raw_pattern, body))
            for variable in re.findall(
                rf"(?:\b(?:s32|u32|int)\s+)?(\w+)\s*=\s*EVENT_OPERAND_BYTE\(\s*{k}\s*\)\s*;", body
            ):
                raw |= bool(
                    re.search(
                        r"field_view\.components\.descriptors\[" + re.escape(variable) + r"\]", body
                    )
                )
            aliases = []
            for other in form.operands:
                if other == operand:
                    continue
                width = events.WIDTHS[other.kind]
                if other.offset <= operand.offset < other.offset + width:
                    aliases.append(asdict(other))
            actor_facts[key] = {
                "handler": form.handler,
                "operand": asdict(operand),
                "role": ("raw_and_actor_selector" if raw else "actor_selector")
                if proofs
                else "unknown",
                "aliases": aliases,
                "provenance": proofs,
                "reason": None
                if proofs
                else ("No exact current-PC actor reader in the handler or an approved delegation."),
            }
        if re.search(r"\b(?:EVENT_ZONE(?:_CORNER)?|CODE_ZONE(?:_CORNER)?)\b", body):
            operands = [
                operand for operand in form.operands if operand.offset == 1 and operand.kind == "u8"
            ]
            if len(operands) != 1:
                raise ValueError(f"zone handler {form.handler} lacks its proven byte selector")
            zone_facts[form.handler] = {
                "handler": form.handler,
                "operand": asdict(operands[0]),
                "role": "zone_index",
                "provenance": [
                    event_access.evidence(SOURCE, form.handler, body, "EVENT_ZONE/CODE_ZONE")
                ],
                "macro_sha256": macro_hash,
            }
    reader = event_access.function(source, "field_event_read_actor_index")
    return {
        "schema_version": 1,
        "generator": "tools/studio/scene_bindings.py",
        "sources": {
            path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest()
            for path in (SOURCE, "tools/analysis/events.py", "tools/analysis/event_access.py")
        },
        "actor_reader_sha256": hashlib.sha256(reader.encode()).hexdigest(),
        "actor_readers": {
            name: hashlib.sha256(event_access.function(source, name).encode()).hexdigest()
            for name in ("field_event_read_actor_index", "field_event_read_actor_index_or_leader")
        },
        "scope": (
            "Literal field references only; party/self remain symbolic. "
            "No runtime variable or incoming-entrance closure is inferred."
        ),
        "actors": [actor_facts[key] for key in sorted(actor_facts)],
        "zones": [zone_facts[key] for key in sorted(zone_facts)],
    }


def metadata_bytes() -> bytes:
    return (json.dumps(metadata(), sort_keys=True, indent=2) + "\n").encode("utf-8")


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--output", type=Path, default=OUTPUT)
    args = parser.parse_args(argv)
    expected = metadata_bytes()
    if args.check:
        if not args.output.is_file() or args.output.read_bytes() != expected:
            parser.exit(1, "Scene bindings stale; run python3 -m tools.studio.scene_bindings\n")
        print("Scene bindings match canonical forms and source reader proofs.")
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_bytes(expected)
        print(f"Wrote public scene binding metadata: {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
