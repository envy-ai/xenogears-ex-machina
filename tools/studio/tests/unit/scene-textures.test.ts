import { describe, expect, it } from 'vitest';
import { facePixelsKnown, SceneVram } from '../../src/scene/textures';
import type { SceneAssets } from '../../src/backend/scene-assets';

type Upload = SceneAssets['images']['uploads'][number];
function upload(x: number, y: number, width: number, height: number, words: number[]): Upload {
  return { source_span: [0, words.length * 2], pixels_offset: 0, rect: [x, y, width, height], words, kind: 'synthetic', slot: null, tim_mode: null };
}
function vram(palettes: Upload[] = [], images: Upload[] = []): SceneVram {
  const assets: Pick<SceneAssets, 'palettes' | 'images'> = {
    palettes: { uploads: palettes, shared_slots: [] },
    images: { uploads: images, shared_slots: [] },
  };
  return new SceneVram(assets);
}
function rgba(page: ReturnType<SceneVram['page']>, u: number, v = 0): number[] {
  const offset = (v * 256 + u) * 4;
  return Array.from(page.rgba.slice(offset, offset + 4));
}

describe('source PS1 texture preview addressing', () => {
  it('unpacks 4-bit indices, applies CLUT page/row addressing and keeps index zero transparent', () => {
    const texturePage = 0x11; // page x=64 words, y=256; 4-bit depth
    const clut = 1; // x=16 words, y=0
    const source = vram(
      [upload(16, 0, 4, 1, [0x0000, 0x001f, 0x03e0, 0x7c00])],
      [upload(64, 256, 1, 1, [0x3210])],
    );
    const page = source.page(texturePage, clut);

    expect(rgba(page, 0)).toEqual([0, 0, 0, 0]);
    expect(rgba(page, 1)).toEqual([255, 0, 0, 255]);
    expect(rgba(page, 2)).toEqual([0, 255, 0, 255]);
    expect(rgba(page, 3)).toEqual([0, 0, 255, 255]);
    expect(Array.from(page.known.slice(0, 4))).toEqual([1, 1, 1, 1]);
  });

  it('unpacks low/high 8-bit indices from a word and reads the selected CLUT row', () => {
    const source = vram(
      [upload(32, 2, 3, 1, [0, 0x7c00, 0x7fff])],
      [upload(0, 0, 1, 1, [0x0201])],
    );
    const page = source.page(0x80, (2 | (2 << 6)));

    expect(rgba(page, 0)).toEqual([0, 0, 255, 255]);
    expect(rgba(page, 1)).toEqual([255, 255, 255, 255]);
    expect(Array.from(page.known.slice(0, 2))).toEqual([1, 1]);
  });

  it('reads direct 16-bit colors and distinguishes transparent zero from a missing source word', () => {
    const source = vram([], [upload(0, 0, 2, 1, [0x0000, 0x83e0])]);
    const page = source.page(0x100, 0);
    expect(rgba(page, 0)).toEqual([0, 0, 0, 0]);
    expect(rgba(page, 1)).toEqual([0, 255, 0, 255]);
    expect(Array.from(page.known.slice(0, 3))).toEqual([1, 1, 0]);

    const missingClut = vram([], [upload(0, 0, 1, 1, [0x0001])]).page(0, 1);
    expect(missingClut.known[0]).toBe(0);
    expect(rgba(missingClut, 0)).toEqual([0, 0, 0, 0]);
  });

  it('applies palette uploads before images and later image uploads in source order', () => {
    const source = vram(
      [upload(0, 0, 1, 1, [0x001f])],
      [upload(0, 0, 1, 1, [0x03e0]), upload(0, 0, 1, 1, [0x7c00])],
    );
    expect(rgba(source.page(0x100, 0), 0)).toEqual([0, 0, 255, 255]);
  });

  it('requires every integer texel covered by a face UV bounding rectangle to be known', () => {
    const page = vram([], [upload(10, 10, 1, 1, [0x1234])]).page(0x100, 0);
    expect(facePixelsKnown(page, [[10, 10], [10, 10], [10, 10]])).toBe(true);
    expect(facePixelsKnown(page, [[10, 10], [11, 10], [11, 11], [10, 11]])).toBe(false);
  });

  it('rejects malformed upload rectangles instead of inventing or truncating words', () => {
    expect(() => vram([], [upload(0, 0, 2, 1, [0x1234])])).toThrow(/rectangle and words disagree/);
  });
});
