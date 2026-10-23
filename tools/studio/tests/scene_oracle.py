"""Private M7 reader oracle for a supplied map bundle; never prints game bytes."""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[3]

from tools.analysis.dispatch_tables import group_models, primitive_table, walk_model
from tools.analysis.field import field_components


def u16(data: bytes, offset: int) -> int:
    if offset < 0 or offset + 2 > len(data):
        raise ValueError("u16 source span exceeds component output")
    return struct.unpack_from("<H", data, offset)[0]


def u32(data: bytes, offset: int) -> int:
    if offset < 0 or offset + 4 > len(data):
        raise ValueError("u32 source span exceeds component output")
    return struct.unpack_from("<I", data, offset)[0]


def span(data: bytes, offset: int, size: int) -> bytes:
    if offset < 0 or size < 0 or offset + size > len(data):
        raise ValueError("model source span exceeds component output")
    return data[offset : offset + size]


def arity(draw: tuple[str, ...]) -> int:
    if not draw:
        raise ValueError("primitive dispatch has no draw routine")
    for family in ("f", "ft", "g", "gt"):
        if draw[0].startswith(f"model_draw_{family}3_"):
            return 3
        if draw[0].startswith(f"model_draw_{family}4_"):
            return 4
    raise ValueError("unqualified primitive draw routine")


def model_json(data: bytes, model: int, base: int, model_index: int, table: Any) -> dict[str, Any]:
    if model < 0 or model + 56 > len(data):
        raise ValueError("model header outside component output")
    flags, vertex_count, face_count, group_count = struct.unpack_from("<4H", data, model)
    vertices_at = base + u32(data, model + 8)
    normals_at = base + u32(data, model + 12)
    groups_at = base + u32(data, model + 16)
    aux = base + u32(data, model + 20)
    morph = u32(data, model + 28)
    morph_at = None if morph == 0 else base + morph
    vertices_raw = span(data, vertices_at, vertex_count * 8)
    vertices = [list(values) for values in struct.iter_unpack("<4h", vertices_raw)]
    if morph_at is not None:
        span(data, morph_at, 4)

    walk = walk_model(data, model, base, table)
    if walk.errors or walk.differences:
        raise ValueError("Python model walk rejected a mesh header/packet declaration")

    record = groups_at
    faces: list[dict[str, Any]] = []
    tpage: int | None = None
    clut: int | None = None
    for _ in range(group_count):
        group_header = span(data, record, 4)
        kind = group_header[0]
        count = struct.unpack_from("<h", group_header, 2)[0]
        if count < 0 or kind >= len(table.types):
            raise ValueError("Python model walk rejected a primitive group")
        spec = table.types[kind]
        corners = arity(spec.draw)
        record += 4
        for _ in range(count):
            face_at = record
            raw = span(data, record, spec.stride)
            raw_indices = list(struct.unpack_from("<4H", raw))
            effective = raw_indices[:corners]
            effective[1] &= 0x1FFF
            controls = []
            if spec.overrides:
                while True:
                    command = span(data, aux, 4)
                    if command[3] not in table.override_codes:
                        break
                    code = command[3]
                    value = u16(command, 0)
                    if code == 0xC4:
                        tpage = value
                    elif code == 0xC8:
                        clut = value
                    else:
                        raise ValueError("Python oracle encountered an unqualified override")
                    controls.append({"source_span": [aux, aux + 4], "code": code, "value": value})
                    aux += 4
            auxiliary_at = aux
            auxiliary = list(span(data, aux, spec.aux_stride))
            textured = "ft" in spec.prepare or "gt" in spec.prepare
            diagnostic = None
            uv = None
            if spec.prepare.endswith("_envmap"):
                diagnostic = "Environment-map UVs depend on runtime normals and overrides"
            elif textured:
                offsets = [4, 6, 0] if corners == 3 else [4, 6, 8, 10]
                uv = [[auxiliary[at], auxiliary[at + 1]] for at in offsets]
                if tpage is None or clut is None:
                    diagnostic = "Texture page/CLUT is inherited; no source is invented"
            if spec.prepare.endswith("_lit") and any(
                struct.unpack("<h", struct.pack("<H", value))[0] < 0 or value >= vertex_count
                for value in raw_indices[:3]
            ):
                message = (
                    "Conditional lit preparation uses raw signed vertex indices outside the model; "
                    "only draw geometry is qualified"
                )
                diagnostic = f"{diagnostic}; {message}" if diagnostic else message
            faces.append(
                {
                    "source_span": [face_at, face_at + spec.stride],
                    "auxiliary_span": [auxiliary_at, auxiliary_at + spec.aux_stride],
                    "kind": kind,
                    "raw_indices": raw_indices,
                    "vertices": effective,
                    "auxiliary": auxiliary,
                    "controls": controls,
                    "declared_tpage": tpage,
                    "declared_clut": clut,
                    "uv": uv,
                    "diagnostic": diagnostic,
                }
            )
            record += spec.stride
            aux += spec.aux_stride

    if len(faces) != face_count:
        raise ValueError("Python model walk disagrees with declared face count")
    return {
        "source_span": [model, model + 56],
        "group_offset": base,
        "model_index": model_index,
        "flags": flags,
        "vertices_offset": vertices_at,
        "vertices": vertices,
        "normals_offset": normals_at,
        "primitive_groups_offset": groups_at,
        "morph_offset": morph_at,
        "faces": faces,
    }


def scene_oracle(bundle_path: Path) -> dict[str, Any]:
    bundle_bytes = bundle_path.read_bytes()
    components = field_components(bundle_bytes)
    component = components[2]
    data = component.decoded.data
    count = u32(data, 0)
    if count > 4096:
        raise ValueError("geometry group count exceeds the source analysis bound")
    table = primitive_table(ROOT)
    groups = []
    mesh_count = 0
    for group_index in range(count):
        base = u32(data, 4 + group_index * 4)
        models = group_models(data, base)
        flags = u32(data, base + 4)
        meshes = [
            model_json(data, model, base, model_index, table)
            for model_index, model in enumerate(models)
        ]
        mesh_count += len(meshes)
        groups.append({"source_offset": base, "flags": flags, "meshes": meshes})
    return {
        "schema_version": 1,
        "bundle_sha256": hashlib.sha256(bundle_bytes).hexdigest(),
        "component": {
            "index": component.index,
            "source_offset": component.source_offset,
            "logical_size": component.logical_size,
            "decoded_output_size": len(component.decoded.data),
            "source_bytes_read": component.decoded.source_bytes_read,
            "decoded_sha256": hashlib.sha256(component.decoded.data).hexdigest(),
        },
        "group_count": len(groups),
        "mesh_count": mesh_count,
        "geometry": {"groups": groups},
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--bundle", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    report = scene_oracle(args.bundle)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, sort_keys=True, separators=(",", ":"))
        output.write("\n")
    print(f"wrote private scene oracle: groups={report['group_count']} meshes={report['mesh_count']}")


if __name__ == "__main__":
    main()
