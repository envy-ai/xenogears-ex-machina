/** Read-only decoded original assets; Rust is the only format reader. */
import { readCommandResult, type Backend } from './service';
import type { FieldGraph } from '../events/model';
import type { SceneDescriptor, SceneEntrance, SceneInput, SceneCollision, SceneCorners } from './scenes';
export interface SceneFaceAsset {
  source_span:[number,number]; auxiliary_span:[number,number]; kind:number;
  raw_indices:[number,number,number,number]; vertices:number[]; auxiliary:number[];
  controls:{source_span:[number,number];code:number;value:number}[];
  declared_tpage:number|null; declared_clut:number|null; uv:[number,number][]|null; diagnostic:string|null;
}
export interface SceneMeshAsset {
  source_span:[number,number]; group_offset:number; model_index:number; flags:number;
  vertices_offset:number; vertices:[number,number,number,number][]; normals_offset:number;
  primitive_groups_offset:number; morph_offset:number|null; faces:SceneFaceAsset[];
}
export interface SceneUploads {
  uploads:{source_span:[number,number];pixels_offset:number;rect:[number,number,number,number];words:number[];kind:string;slot:number|null;tim_mode:number|null}[];
  shared_slots:number[];
}
export interface SceneMapStream {
  uploads:SceneUploads['uploads']; source_bytes_read:number; image_count:number; stop_reason:string;
  source?:{file:number;slot:number;lba:number;indexed_size:number;source_sha256:string};
}
export interface SceneSpritePart {
  source_span:[number,number]; cell_span:[number,number];
  position:[number,number]; size:[number,number]; uv:[number,number]; uv_size:[number,number];
  tpage:number; clut:number; palette:number; group:number; flip_x:boolean; flip_y:boolean; blend:number;
}
export interface SceneSpriteFrame {
  frame:number; animation:number; facing:number; flip_x:boolean; flip_y:boolean;
  source_span:[number,number]; parts:SceneSpritePart[]; uploads:SceneUploads['uploads'];
  clear:{source_span:[number,number];rect:[number,number,number,number]}|null;
}
export interface SceneSpriteResource {
  slot:number; source_span:[number,number]; palette_count:number;
  palette_banks:{bank:number;source_span:[number,number];words:number[]}[];
  first_frame:SceneSpriteFrame|null; diagnostic:string|null;
}
export interface SceneAssets {
  schema_version:1; source_sha256:string;
  header:{descriptors:SceneDescriptor[];view:{words:number[]};slots:{x:number;y:number;reserved:number;shared:number}[];offsets:number[];logical_sizes:number[];source_sha256:string};
  geometry:{groups:{source_offset:number;flags:number;meshes:SceneMeshAsset[]}[]};
  collision:SceneCollision; zones:{zones:{corners:SceneCorners}[];tail:number[]};
  palettes:SceneUploads; images:SceneUploads; sprites:{resources:SceneSpriteResource[]}; map_stream?:SceneMapStream; events:FieldGraph;
  entrances:SceneEntrance[]; diagnostics:string[];
}
export function decodeSceneAssets(backend:Backend,input:SceneInput,entrances:number[]):SceneAssets {
  return readCommandResult<SceneAssets>(backend.command(JSON.stringify({op:'field.scene.decode',args:{input,entrances}})));
}
