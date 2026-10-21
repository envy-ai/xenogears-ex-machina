"""Conservative operand access proofs over the authoritative field forms.

This is source analysis, not an opcode registry. A fact is established only by
an exact reader/writer call in the recovered handler (or an explicitly named
delegation). Unmatched expressions stay unknown. These are possible accesses,
not execution order or a claim that a conditional handler path always runs.
"""

from __future__ import annotations

import hashlib
import re
from dataclasses import asdict
from pathlib import Path

from tools.analysis import events

ROOT = Path(__file__).resolve().parents[2]
SOURCE = "src/decomp/src/field/field_event.c"
READER_SOURCE = "src/decomp/src/field/field_effect.c"
SCREEN_SOURCE = "src/decomp/src/field/field_screen.c"
KINDS = {"var", "iv", "sel", "bit", "data"}
# Source-confirmed calls into shared operand readers/writers. Matching still
# requires exact operand/selector offsets; unresolved PC rebasing stays unknown.
DELEGATES = {
    **{f"field_event_store_flags{i}": "field_event_store_flags" for i in range(4)},
    "field_event_set_template_pairs0": "field_event_set_template_pairs",
    "field_event_set_template_pairs4": "field_event_set_template_pairs",
    "field_event_select_music": "field_event_select_music_track",
    "field_event_select_music_keep": "field_event_select_music_track",
    "field_event_change_map_transition": "field_event_change_map_entry",
    "field_event_walk": "field_event_walk_straight",
    "field_event_walk_limited": "field_event_walk_straight",
    "field_event_walk_player_ahead": "field_event_walk_player",
    "field_event_walk_player_ahead_ea": "field_event_walk_player",
    "field_event_move_by": "field_event_move",
    "field_event_move_by_limited": "field_event_move",
    "field_event_move_to": "field_event_move",
    "field_event_move_to_limited": "field_event_move",
    "field_event_move_to_actor": "field_event_move",
    "field_event_move_to_actor_limited": "field_event_move",
    "field_event_move_angle": "field_event_move",
    "field_event_turn_move_by": "field_event_turn_move",
    "field_event_turn_move_by_limited": "field_event_turn_move",
    "field_event_turn_move_to": "field_event_turn_move",
    "field_event_turn_move_to_limited": "field_event_turn_move",
    "field_event_turn_move_angle": "field_event_turn_move",
}
# The packed bit operand names a BYTE reference by >>4, then the bank helper
# indexes that reference by >>1. It does not directly encode a bank slot.
BIT_HANDLERS = {
    "field_event_branch_unless_bit": "read",
    "field_event_ext_set_variable_bit": "read_write",
    "field_event_ext_clear_variable_bit": "read_write",
}


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def function(source: str, name: str) -> str:
    match = re.search(r"^\w+[ \t*]+" + re.escape(name) + r"\([^;{}]*\)\s*\{", source, re.MULTILINE)
    if match is None:
        raise ValueError(f"missing recovered function {name}")
    end, depth = match.end(), 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[match.start() : end]


def evidence(path: str, name: str, body: str, helper: str) -> dict:
    return {
        "source": path,
        "function": name,
        "function_sha256": digest(body.encode()),
        "helper": helper,
    }


def operand_facts() -> list[dict]:
    source = (ROOT / SOURCE).read_text()
    reader_source = (ROOT / READER_SOURCE).read_text()
    forms = [f for fs in (*events.PRIMARY.values(), *events.EXTENDED.values()) for f in fs]
    descriptors = {
        (form.handler, operand.kind, operand.offset, operand.flags, operand.bit)
        for form in forms
        for operand in form.operands
        if operand.kind in KINDS
    }
    facts = []
    for handler, kind, offset, flags, bit in sorted(descriptors):
        body = function(source, handler)
        owner, proof_body, proof_path = handler, body, SOURCE
        if handler in DELEGATES:
            owner = DELEGATES[handler]
            if f"{owner}(" not in body:
                raise ValueError(f"missing proven delegation {handler} -> {owner}")
            proof_body = function(source, owner)
        elif handler == "field_event_screen_distortion":
            owner, proof_path = "field_distortion_start", SCREEN_SOURCE
            if "field_distortion_start(0);" not in body:
                raise ValueError("screen distortion delegation changed")
            proof_body = function((ROOT / proof_path).read_text(), owner)
        proofs = []
        direction = "unknown"
        reason = "No proven operand access in the recovered handler."
        if kind in ("iv", "sel"):
            helper = (
                "field_event_read_imm_or_var"
                if kind == "iv"
                else f"field_event_read_selected_operand_{bit:02x}"
            )
            # Check numeric literals, including hexadecimal source spelling.
            number = r"(0x[0-9A-Fa-f]+|\d+)"
            pattern = re.escape(helper) + r"\(\s*" + number + r"\s*"
            calls = []
            # Keep direct wrapper reads as well as the delegated body's reads.
            candidates = (
                [(handler, body, SOURCE)]
                if owner == handler
                else [(handler, body, SOURCE), (owner, proof_body, proof_path)]
            )
            for candidate_owner, candidate, candidate_path in candidates:
                for match in re.finditer(pattern, candidate):
                    if int(match.group(1), 0) != offset:
                        continue
                    if kind == "sel":
                        selectors = [
                            r",\s*EVENT_OPERAND_BYTE\(" + number + r"\)\)",
                            r",\s*field_event_bytecode\[field_current_event_actor->pc\s*\+\s*"
                            + number
                            + r"\]\)",
                        ]
                        if (
                            "pc = field_current_event_actor->pc;" in candidate
                            and "code = pc + field_event_bytecode;" in candidate
                        ):
                            selectors.append(r",\s*code\[" + number + r"\]\)")
                        if not any(
                            (flag_match := re.match(selector, candidate[match.end() :]))
                            and int(flag_match.group(1), 0) == flags
                            for selector in selectors
                        ):
                            continue
                    calls.append((candidate_owner, candidate, candidate_path))
            if calls:
                helper_path = READER_SOURCE if kind == "iv" else SOURCE
                helper_body = function(reader_source if kind == "iv" else source, helper)
                if "field_event_read_variable(" not in helper_body:
                    raise ValueError(f"reader no longer reads variables: {helper}")
                direction, reason = "read", None
                proven_owner, proven_body, proven_path = calls[0]
                proofs.append(evidence(proven_path, proven_owner, proven_body, helper))
                proofs.append(
                    evidence(helper_path, helper, helper_body, "field_event_read_variable")
                )
            elif kind == "sel" and handler == "field_event_branch_if_false":
                # 02 inlines the two mode-controlled reads rather than using
                # selected_operand_80/40; its descriptor carries those modes.
                needle = f"field_event_read_variable(field_event_read_u16({offset})"
                if needle not in body:
                    raise ValueError("branch inline operand proof changed")
                direction, reason = "read", None
                proofs.append(evidence(SOURCE, handler, body, "field_event_read_variable"))
        elif kind == "var":
            raw = rf"field_event_read_u16\(\s*(?:{offset}|0x{offset:X}|0x{offset:x})\s*\)"
            aliases = re.findall(r"\b(\w+)\s*=\s*" + raw + r"\s*(?:&\s*0xFFFF)?\s*;", proof_body)
            references = [raw + r"\s*(?:&\s*0xFFFF)?"]
            references.extend(re.escape(alias) + r"\s*(?:&\s*0xFFFF)?" for alias in aliases)
            roles = []
            for role in ("read", "write"):
                helper = f"field_event_{role}_variable"
                if any(
                    re.search(re.escape(helper) + r"\(\s*" + ref + r"\s*[,)]", proof_body)
                    for ref in references
                ):
                    roles.append(role)
                    proofs.append(evidence(SOURCE, owner, proof_body, helper))
            if roles:
                direction, reason = "_".join(roles), None
        elif kind == "bit" and handler in BIT_HANDLERS:
            if (
                "variable = reference >> 4" not in body
                or "field_event_read_variable(variable)" not in body
            ):
                raise ValueError(f"packed bit proof changed: {handler}")
            direction, reason = BIT_HANDLERS[handler], None
            for role in direction.split("_"):
                helper = f"field_event_{role}_variable"
                if helper + "(" not in body:
                    raise ValueError(f"packed bit proof missing {helper}")
                proofs.append(evidence(SOURCE, handler, body, helper))
        elif kind == "data":
            reason = "Bytecode data-array address; not a direct variable reference."
        if direction == "unknown":
            if handler == "field_event_change_map_transition":
                reason = (
                    "The delegated map-entry handler can advance the PC before these readers; "
                    "the movie-pending path retains a different base. No fixed operand proof."
                )
            elif handler == "field_event_walk_limited" and kind == "iv":
                reason = (
                    "The handler reads offset 11 on the preceding setup path; this step's "
                    "offset-2 descriptor requires a cross-instruction access relation."
                )
            proofs.append(evidence(SOURCE, handler, body, "unresolved"))
        if (
            owner != handler
            and direction != "unknown"
            and any(p["function"] == owner for p in proofs)
        ):
            proofs.insert(0, evidence(SOURCE, handler, body, owner))
        facts.append(
            {
                "handler": handler,
                "operand": asdict(events.Operand(kind, offset, flags, bit)),
                "direction": direction,
                "reason": reason,
                "provenance": proofs,
            }
        )
    return facts
