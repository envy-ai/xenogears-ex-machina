import * as THREE from 'three';
import { describe, expect, it } from 'vitest';
import type { SceneSnapshot, SceneSpriteBinding } from '../../src/backend/scenes';
import type { SceneSpritePart, SceneSpriteResource, SceneAssets } from '../../src/backend/scene-assets';
import { actorHeaderPosition, actorHeaderWords, makeSceneModel } from '../../src/scene/model';
import { makeSourceSprite, projectSpritePart, sourceSpriteImages } from '../../src/scene/sprites';
import { SceneVram } from '../../src/scene/textures';

function part(overrides: Partial<SceneSpritePart> = {}): SceneSpritePart {
  return {
    source_span: [20, 23], cell_span: [40, 45], position: [-2, 3], size: [4, 2],
    uv: [0, 0], uv_size: [4, 2], tpage: 0, clut: (480 << 6) | 16,
    palette: 0, group: 0, flip_x: false, flip_y: false, blend: 0, ...overrides,
  };
}

function resource(partial = part()): SceneSpriteResource {
  return {
    slot: 0, source_span: [8, 100], palette_count: 1,
    palette_banks: [
      { bank: 0, source_span: [80, 112], words: [0, 0x001f, 0x03e0, 0x7c00, 0x03ff, 0x7fe0, 0x7c1f, 0x7fff, ...Array(8).fill(0)] },
      { bank: 1, source_span: [112, 144], words: [0, 0x7c00, 0x03e0, 0x001f, 0x03ff, 0x7fe0, 0x7c1f, 0x7fff, ...Array(8).fill(0)] },
    ],
    first_frame: {
      frame: 1, animation: 0, facing: 0, flip_x: false, flip_y: false,
      source_span: [30, 50], clear: null, parts: [partial],
      uploads: [{ source_span: [60, 70], pixels_offset: 62, rect: [0, 0, 1, 2], words: [0x3210, 0x7654], kind: 'sprite_cell', slot: 0, tim_mode: null }],
    },
    diagnostic: null,
  };
}

function vram(): SceneVram {
  const empty: SceneAssets['palettes'] = { uploads: [], shared_slots: [] };
  const assets: Pick<SceneAssets, 'palettes' | 'images'> = {
    palettes: empty,
    images: { uploads: [], shared_slots: [] },
  };
  return new SceneVram(assets);
}

function rgbaRows(bytes: Uint8Array, width: number): number[][] {
  if (bytes.length % (width * 4) !== 0) throw new Error('RGBA fixture is not a whole number of sprite rows');
  return Array.from({ length: bytes.length / 4 }, (_, i) => Array.from(bytes.slice(i * 4, i * 4 + 4)));
}

describe('source-qualified sprite projection', () => {
  it('sign-extends only actor header positions, preserves raw words, and gives qualified script placement precedence', () => {
    const rawActorPosition: [number, number, number] = [65_228, 32_768, 19];
    expect(actorHeaderPosition(rawActorPosition)).toEqual([-308, -32_768, 19]);
    expect(actorHeaderWords([-308, -32_768, 19])).toEqual(rawActorPosition);
    expect(Number.isNaN(actorHeaderWords([-32_769, 32_768, 0])[0])).toBe(true);
    expect(Number.isNaN(actorHeaderWords([-32_769, 32_768, 0])[1])).toBe(true);

    const state = {
      descriptors: [
        { id: 'scene:1:actor', value: { flags: 0x40, rotation: [0, 0, 0], position: rawActorPosition, model: 0 } },
        { id: 'descriptor:1', value: { flags: 0, rotation: [0, 0, 0], position: rawActorPosition, model: 0 } },
      ],
      actors: [{ id: 'scene:1:actor' }], sprite_bindings: [],
      collision_entities: { layers: [] }, zones: [], entrances: [],
    } as unknown as SceneSnapshot;
    const assets = {
      geometry: { groups: [{ meshes: [] }] },
      palettes: { uploads: [], shared_slots: [] }, images: { uploads: [], shared_slots: [] },
      sprites: { resources: [] },
    } as unknown as SceneAssets;
    const visible = { geometry: true, actors: true, collision: true, zones: true, entrances: true };
    const model = makeSceneModel(assets, state, visible, null);
    expect(model.objects.get('scene:1:actor')!.position.toArray()).toEqual([-308, 32_768, 19]);
    expect(model.objects.get('descriptor:1')!.position.toArray()).toEqual([65_228, -32_768, 19]);
    const scriptPosition: [number, number, number] = [321, -7, 9];
    const scripted = makeSceneModel(assets, state, visible, null, new Map([['scene:1:actor', scriptPosition]]));
    expect(scripted.objects.get('scene:1:actor')!.position.toArray()).toEqual([321, 7, 9]);
    expect(scripted.objects.get('descriptor:1')!.position.toArray()).toEqual([65_228, -32_768, 19]);
    expect(state.descriptors[0].value.position).toEqual(rawActorPosition);
    model.dispose();
    scripted.dispose();
  });

  it('projects signed part origins and combines part/frame flips without changing bounds', () => {
    const source = part({ flip_x: true, flip_y: false });
    expect(projectSpritePart(source, false, false)).toEqual({ left: -2, top: 3, width: 4, height: 2, flipX: true, flipY: false });
    expect(projectSpritePart(source, true, false)).toEqual({ left: -2, top: 3, width: 4, height: 2, flipX: false, flipY: false });
    expect(projectSpritePart(source, false, true)).toEqual({ left: -2, top: -5, width: 4, height: 2, flipX: true, flipY: true });
    expect(projectSpritePart(source, true, true)).toEqual({ left: -2, top: -5, width: 4, height: 2, flipX: false, flipY: true });
  });

  it('uses the selected source palette bank and preserves transparent/known source pixels', () => {
    const image = sourceSpriteImages(resource(part({ uv_size: [4, 2] })), 0, vram())[0];
    expect(image.projection).toEqual({ left: -2, top: 3, width: 4, height: 2, flipX: false, flipY: false });
    expect(rgbaRows(image.rgba, image.width)).toEqual([
      [0, 0, 0, 0], [255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255],
      [255, 255, 0, 255], [0, 255, 255, 255], [255, 0, 255, 255], [255, 255, 255, 255],
    ]);
    const alternate = sourceSpriteImages(resource(part({ uv_size: [4, 2] })), 1, vram())[0];
    expect(rgbaRows(alternate.rgba, alternate.width)).toEqual([
      [0, 0, 0, 0], [0, 0, 255, 255], [0, 255, 0, 255], [255, 0, 0, 255],
      [255, 255, 0, 255], [0, 255, 255, 255], [255, 0, 255, 255], [255, 255, 255, 255],
    ]);
  });

  it('applies flips to exact pixels, including the source part flip and whole-frame flips', () => {
    const source = resource(part({ uv_size: [4, 2], flip_x: true, flip_y: true }));
    source.first_frame!.flip_x = true;
    source.first_frame!.flip_y = true;
    const image = sourceSpriteImages(source, 0, vram())[0];
    expect(image.projection.flipX).toBe(false);
    expect(image.projection.flipY).toBe(false);
    expect(rgbaRows(image.rgba, image.width).slice(0, 4)).toEqual([
      [0, 0, 0, 0], [255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255],
    ]);

    const flipped = resource(part({ uv_size: [4, 2] }));
    flipped.first_frame!.flip_x = true;
    const flippedImage = sourceSpriteImages(flipped, 0, vram())[0];
    expect(rgbaRows(flippedImage.rgba, flippedImage.width).slice(0, 4)).toEqual([
      [0, 0, 255, 255], [0, 255, 0, 255], [255, 0, 0, 255], [0, 0, 0, 0],
    ]);
  });

  it('applies explicit source ClearImage as known zero before cell uploads', () => {
    const cleared = resource(part({ uv_size: [1, 1] }));
    cleared.first_frame!.clear = { source_span: [50, 52], rect: [0, 0, 1, 1] };
    cleared.first_frame!.uploads = [{ source_span: [60, 62], pixels_offset: 60, rect: [4, 0, 1, 1], words: [0x001f], kind: 'sprite_cell', slot: 0, tim_mode: null }];
    const baseAssets: Pick<SceneAssets, 'palettes' | 'images'> = {
      palettes: { uploads: [], shared_slots: [] },
      images: { uploads: [{ source_span: [0, 2], pixels_offset: 0, rect: [0, 0, 1, 1], words: [0x1111], kind: 'map_image_strip', slot: null, tim_mode: null }], shared_slots: [] },
    };
    const image = sourceSpriteImages(cleared, 0, new SceneVram(baseAssets))[0];
    expect(Array.from(image.rgba)).toEqual([0, 0, 0, 0]);
  });

  it('creates camera-facing Three sprites anchored to the stable authored actor identity', () => {
    const binding: SceneSpriteBinding = { actor: 'scene:7:actor', node: 'node:source', pc: 15, resource: 0, bank: 1, diagnostic: null };
    const model = makeSourceSprite(binding, resource(), vram());
    expect(model.group.children).toHaveLength(1);
    const child = model.group.children[0];
    expect(child).toBeInstanceOf(THREE.Sprite);
    const sprite = child as THREE.Sprite;
    expect(sprite.material).toBeInstanceOf(THREE.SpriteMaterial);
    expect(sprite.center.toArray()).toEqual([0.5, 2.5]);
    expect(sprite.scale.toArray()).toEqual([24, 12, 1]);
    expect(model.group.userData.sprite).toEqual({ actor: binding.actor, resource: 0, bank: 1, frame: 1, animation: 0, facing: 0, parts: 1, pc: 15, node: 'node:source' });
    expect(typeof sprite.raycast).toBe('function');
    model.textures.forEach(texture => texture.dispose());
    model.materials.forEach(material => material.dispose());

    const multipart = resource();
    multipart.first_frame!.parts = [part(), part({ position: [1, -1] })];
    const composed = makeSourceSprite(binding, multipart, vram());
    expect(composed.group.children).toHaveLength(2);
    expect(composed.group.children.every(child => child instanceof THREE.Sprite)).toBe(true);
    expect(composed.group.children.map(child => child.renderOrder)).toEqual([2, 1]);
    composed.textures.forEach(texture => texture.dispose());
    composed.materials.forEach(material => material.dispose());
  });

  it('maps source top-row UVs to the top of Three sprites independently of authored flips', () => {
    const binding: SceneSpriteBinding = { actor: 'scene:7:actor', node: null, pc: 15, resource: 0, bank: 0, diagnostic: null };
    const model = makeSourceSprite(binding, resource(), vram());
    const sprite = model.group.children[0] as THREE.Sprite;
    const uv = (u: number, v: number) => new THREE.Vector2(u, v).applyMatrix3(sprite.material.map!.matrix);

    expect(uv(0.25, 1).y).toBe(0);
    expect(uv(0.25, 0).y).toBe(1);
    expect(uv(0.25, 1).x).toBe(0.25);
    model.textures.forEach(texture => texture.dispose());
    model.materials.forEach(material => material.dispose());
  });

  it('keeps unsupported blends, unresolved frames, missing pixels and missing banks explicit', () => {
    const blended = resource(part({ blend: 1 }));
    expect(() => sourceSpriteImages(blended, 0, vram())).toThrow(/semitransparent blending/);
    const unresolved = resource(); unresolved.first_frame = null; unresolved.diagnostic = 'source VM state is unresolved';
    expect(() => sourceSpriteImages(unresolved, 0, vram())).toThrow(/source VM state is unresolved/);
    expect(() => sourceSpriteImages(resource(), 3, vram())).toThrow(/no palette bank3/);
    const missing = resource(part({ tpage: 0x100 }));
    expect(() => sourceSpriteImages(missing, 0, vram())).toThrow(/missing pixels are not synthesized/);
  });
});
