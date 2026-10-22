#!/usr/bin/env python3
"""Build the private M5 map-23 message fixture from an explicitly supplied disc extraction.

This is an artifact generator, not a test. It requires an explicit original
extraction root, validates the pinned boot identities, and creates a fresh
ignored leaf under tmp/studio/milestone-5. It never downloads data, changes the
source extraction, or prints game-file bytes/text to stdout. The emitted recipe
is consumed by the native
`xem-package build` CLI in the pinned runtime shell.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
OUTPUT_ROOT = REPO / "tmp/studio/milestone-5"
SLOT = 652
EXPECTED_BUNDLE_SIZE = 110_052
FIELD_NUMBER = 23
COMPONENT = 7
SIZE_TABLE = 0x10C
OFFSET_TABLE = 0x130
MESSAGE_INDEX = 1
EDIT_TEXT = "MODE"

# These are the executable identities used by the runtime's known-disc table.
# Both declarations in the package are checked against the extracted originals.
DISC_IDENTITIES = (
    (
        "SLUS-00664",
        "SLUS_006.64",
        "dc0b2dd786203d4cce5927c5a3fc85a18f39a3f7406078860076ebb0bbae7119",
    ),
    (
        "SLUS-00669",
        "SLUS_006.69",
        "3246e15f4040305b280adae06bc7bb908ee882794183bec9fc23e71d85c19c35",
    ),
)


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def write_new(path: Path, data: bytes) -> None:
    with path.open("xb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def token_sequence(tokens) -> list[dict[str, object]]:
    return [
        {
            "mnemonic": token.mnemonic,
            "code": f"{token.code:04x}",
            "length": token.length,
            # Keep the decoder's operand order in an array; evidence.json is
            # sorted by key when written, so a JSON object would lose ordering.
            "operands": [
                {"name": name, "value": value}
                for name, value in token.operands
            ],
        }
        for token in tokens
    ]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        help="new leaf directory under tmp/studio/milestone-5 (default: real-message-fixture-PID)",
    )
    args = parser.parse_args()
    original_root_text = os.environ.get("XEM_STUDIO_EXTRACT_ROOT")
    if not original_root_text:
        raise SystemExit(
            "set XEM_STUDIO_EXTRACT_ROOT to the original root containing "
            ".local/discs and .local/extract"
        )
    original_root = Path(original_root_text).expanduser().resolve(strict=True)
    for number in (1, 2):
        if not (original_root / f".local/discs/disc{number}.bin").is_file():
            raise SystemExit(f"missing original disc {number} under {original_root}/.local/discs")
        if not (original_root / f".local/extract/disc{number}/manifest.json").is_file():
            raise SystemExit(
                f"missing original disc {number} extraction manifest under "
                f"{original_root}/.local/extract"
            )
    output = args.output or Path(f"real-message-fixture-{os.getpid()}")
    if not output.is_absolute():
        output = OUTPUT_ROOT / output
    output = output.resolve()
    if output.parent != OUTPUT_ROOT.resolve():
        raise SystemExit("--output must name a new leaf inside tmp/studio/milestone-5")
    if output.exists():
        raise SystemExit(f"refusing to overwrite existing fixture directory: {output}")

    sys.path.insert(0, str(REPO))
    from tools import packed_container
    from tools.analysis import text_control
    from tools.analysis.disc_index import Disc
    from tools.analysis.packed import decode_block

    discs = [Disc(number, original_root) for number in (1, 2)]
    declared_discs = []
    for disc, (serial, boot_name_expected, expected_hash) in zip(
        discs, DISC_IDENTITIES, strict=True
    ):
        boot = disc.manifest["boot"]
        actual_hash = sha256(disc.boot)
        if (
            boot["name"] != boot_name_expected
            or boot.get("sha256") != expected_hash
            or actual_hash != expected_hash
        ):
            raise SystemExit(
                f"disc {disc.number} boot identity does not match the pinned {serial} original"
            )
        declared_discs.append({"serial": serial, "sha256": actual_hash})

    disc1 = discs[0]
    map23_slot = disc1.slot(4, 0, 0xB8 + 2 * FIELD_NUMBER)
    if map23_slot != SLOT:
        raise SystemExit(f"map 23 resolves to slot {map23_slot}, expected {SLOT}")
    entry = disc1.entries.get(SLOT)
    if entry is None or entry["size"] != EXPECTED_BUNDLE_SIZE:
        raise SystemExit("map 23 source bundle does not have the expected original indexed size")
    original_bundle = disc1.data(SLOT)
    if len(original_bundle) != EXPECTED_BUNDLE_SIZE:
        raise SystemExit("map 23 source bundle read did not match its declared original size")
    original_sectors = disc1.sectors(SLOT)
    if original_sectors[: len(original_bundle)] != original_bundle:
        raise SystemExit("map 23 sector read does not begin with the indexed source file")

    size_pos = SIZE_TABLE + COMPONENT * 4
    offset_pos = OFFSET_TABLE + COMPONENT * 4
    if len(original_bundle) < max(size_pos, offset_pos) + 4:
        raise SystemExit("map 23 bundle is too short for component 7 metadata")
    logical_size = struct.unpack_from("<I", original_bundle, size_pos)[0]
    source_offset = struct.unpack_from("<I", original_bundle, offset_pos)[0]
    if logical_size == 0 or source_offset >= len(original_bundle):
        raise SystemExit("map 23 component 7 has an invalid size or offset")
    source_block = decode_block(original_sectors[source_offset:], output_limit=logical_size + 16)
    original_component = source_block.data[:logical_size]
    if len(original_component) != logical_size:
        raise SystemExit("map 23 component 7 did not decode to its declared logical size")

    offsets, _end = text_control.text_table(original_component)
    if len(offsets) != 49:
        raise SystemExit(f"map 23 component 7 has {len(offsets)} messages, expected 49")
    message_start = offsets[MESSAGE_INDEX]
    message_end = offsets[MESSAGE_INDEX + 1]
    message_capacity = message_end - message_start
    if message_capacity != 5:
        raise SystemExit(f"map 23 message {MESSAGE_INDEX} has {message_capacity} bytes, expected 5")
    font = text_control.unpack(disc1.sectors(disc1.slot(0, 1, 6)))
    threshold = text_control.font_threshold(font)
    characters = text_control.character_map(disc1, threshold).glyphs
    inverse = {character: glyph for glyph, character in characters.items()}
    try:
        encoded_text = bytes(inverse[character] for character in EDIT_TEXT)
    except KeyError as error:
        raise SystemExit(f"the original character map has no glyph for {error.args[0]!r}") from None
    if any(glyph >= threshold for glyph in encoded_text):
        raise SystemExit("the authored ASCII text resolved to a non-single-byte glyph")
    replacement = encoded_text + b"\0" + bytes(message_capacity - len(encoded_text) - 1)
    if len(replacement) != message_capacity:
        raise SystemExit(f"authored text does not fit in map 23 message {MESSAGE_INDEX}")

    original_tokens = text_control.decode_text(original_component, message_start, threshold)
    modified_component = bytearray(original_component)
    modified_component[message_start:message_end] = replacement
    modified_component = bytes(modified_component)
    changed_positions = [
        position
        for position, (before, after) in enumerate(
            zip(original_component, modified_component, strict=True)
        )
        if before != after
    ]
    allowed_changes = set(range(message_start, message_end))
    if any(position not in allowed_changes for position in changed_positions):
        raise SystemExit(f"logical component changed outside message {MESSAGE_INDEX}'s storage")
    modified_offsets, _ = text_control.text_table(modified_component)
    if modified_offsets != offsets:
        raise SystemExit(f"message table offsets changed while editing message {MESSAGE_INDEX}")
    for index in range(1, len(offsets)):
        end = offsets[index + 1] if index + 1 < len(offsets) else _end
        if (
            index != MESSAGE_INDEX
            and original_component[offsets[index] : end]
            != modified_component[offsets[index] : end]
        ):
            raise SystemExit(f"message {index} changed unexpectedly")
    if original_component[: message_start] != modified_component[: message_start]:
        raise SystemExit(f"message-table data before message {MESSAGE_INDEX} changed unexpectedly")
    modified_tokens = text_control.decode_text(modified_component, message_start, threshold)

    packed_stream = packed_container.encode(modified_component)
    result_bundle = bytearray(original_bundle)
    new_offset = (len(result_bundle) + 3) & ~3
    result_bundle.extend(bytes(new_offset - len(result_bundle)))
    result_bundle.extend(packed_stream)
    # The resident decoder speculatively reads one terminal flag after the last
    # decoded group. This byte is part of the appended stream's measured source.
    result_bundle.append(0)
    struct.pack_into("<I", result_bundle, offset_pos, new_offset)
    result_bundle = bytes(result_bundle)
    strict_result = decode_block(result_bundle[new_offset:], output_limit=logical_size + 16)
    if strict_result.data[:logical_size] != modified_component:
        raise SystemExit(
            "strict decoding of appended component 7 differs from the authored logical bytes"
        )
    if strict_result.source_bytes_read > len(packed_stream) + 1:
        raise SystemExit("strict decoding read beyond the explicitly appended terminal flag")
    if len(result_bundle) <= len(original_bundle):
        raise SystemExit("result bundle size did not grow after appending the new component stream")
    expected_table = bytearray(original_bundle[SIZE_TABLE : OFFSET_TABLE + 8 * 4])
    actual_table = bytearray(result_bundle[SIZE_TABLE : OFFSET_TABLE + 8 * 4])
    expected_table[offset_pos - SIZE_TABLE : offset_pos - SIZE_TABLE + 4] = actual_table[
        offset_pos - SIZE_TABLE : offset_pos - SIZE_TABLE + 4
    ]
    if expected_table != actual_table:
        raise SystemExit("component table changed outside component 7's offset field")

    OUTPUT_ROOT.mkdir(parents=True, exist_ok=True)
    output.mkdir()
    original_path = output / "original-map23.bin"
    result_path = output / "result-map23.bin"
    recipe_path = output / "recipe.json"
    evidence_path = output / "evidence.json"
    write_new(original_path, original_bundle)
    write_new(result_path, result_bundle)

    recipe = {
        "spec": {
            "id": "field23-message-active",
            "discs": declared_discs,
            "requires": [],
            "overrides": [],
        },
        "files": [
            {
                "disc": declared_discs[0]["serial"],
                "slot": SLOT,
                "source": original_path.name,
                "result": result_path.name,
            }
        ],
    }
    write_new(recipe_path, (json.dumps(recipe, indent=2, sort_keys=True) + "\n").encode())
    evidence = {
        "fixture": (
            f"real-disc map 23 message {MESSAGE_INDEX} delta; generated only from the explicitly "
            "selected local originals"
        ),
        "disc_targets": declared_discs,
        "file": {
            "disc": declared_discs[0]["serial"],
            "slot": SLOT,
            "original_size": len(original_bundle),
            "result_size": len(result_bundle),
            "original_sha256": sha256(original_bundle),
            "result_sha256": sha256(result_bundle),
            "only_modified_map_message_indices": [MESSAGE_INDEX],
        },
        "component7": {
            "logical_size": logical_size,
            "original_offset": source_offset,
            "result_offset": new_offset,
            "source_logical_sha256": sha256(original_component),
            "result_logical_sha256": sha256(modified_component),
            "result_compressed_sha256": sha256(packed_stream),
            "explicit_terminal_flag": 0,
            "strict_decoded_logical_sha256": sha256(strict_result.data[:logical_size]),
            "message_count": len(offsets),
            "message_index": MESSAGE_INDEX,
            "message_capacity": message_capacity,
            "authored_text": EDIT_TEXT,
        },
        "message_tokens": {
            "original": token_sequence(original_tokens),
            "modified": token_sequence(modified_tokens),
        },
        "message_bytes": {
            "original_hex": original_component[
                message_start : message_start + sum(token.length for token in original_tokens)
            ].hex(),
            "modified_hex": modified_component[
                message_start : message_start + sum(token.length for token in modified_tokens)
            ].hex(),
        },
        "recipe": recipe_path.name,
        "package_payload": (
            "delta; the recipe has a source file and does not embed an original-file member"
        ),
    }
    write_new(evidence_path, (json.dumps(evidence, indent=2, sort_keys=True) + "\n").encode())
    print(
        json.dumps(
            {
                "status": "fixture generated",
                "output": str(output.relative_to(REPO)),
                "recipe": recipe_path.name,
                "source_sha256": evidence["file"]["original_sha256"],
                "result_sha256": evidence["file"]["result_sha256"],
                "result_size": len(result_bundle),
                "disc_targets": declared_discs,
            },
            sort_keys=True,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
