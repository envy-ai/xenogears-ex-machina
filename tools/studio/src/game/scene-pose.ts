/** Read-only field actor observation through the existing paused schema protocol. */
import { assertLiveComponent, readLivePc, type LoadedField } from './pc';
import { ExactInteger, record, type GameValue } from './protocol';
import type { GameIdentity } from './schema';
import type { SceneVector } from '../backend/scenes';

export interface ScenePoseSource {
  entity: string;
  actor: number;
  identity: GameIdentity;
  loaded: LoadedField;
}
export interface ScenePose {
  entity: string;
  actor: number;
  map: number;
  disc: number;
  identity: GameIdentity;
  vblank: string;
  raw_position: SceneVector;
  position: SceneVector;
  heading_raw: number;
}
function signed(value: GameValue, bits: 16 | 32, label: string): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < -(2 ** (bits - 1)) || value >= 2 ** (bits - 1)) {
    throw new Error(`Unavailable: invalid signed${bits} ${label}`);
  }
  return value;
}
function vblank(value: GameValue): string {
  if (value instanceof ExactInteger && /^(?:0|[1-9]\d*)$/.test(value.decimal) &&
      (value.decimal.length < 20 || (value.decimal.length === 20 && value.decimal <= '18446744073709551615'))) return value.decimal;
  if (typeof value === 'number' && Number.isSafeInteger(value) && value >= 0) return String(value);
  throw new Error('Unavailable: invalid runtime vblank');
}
/** The caller holds GameController.paused and validates its UI owner after await. */
export async function readScenePose(
  request: (command: { [key: string]: GameValue }) => Promise<GameValue>,
  source: ScenePoseSource,
): Promise<ScenePose> {
  if (!/^[a-f0-9]{64}$/.test(source.identity.module) || !source.identity.disc || !source.entity) {
    throw new Error('Unavailable: Scene observation requires an identified source/module');
  }
  const status = record(await request({ cmd: 'status' }), 'Scene pose status');
  const identity = record(status.identity, 'Scene pose identity');
  if (identity.module !== source.identity.module || identity.disc !== source.identity.disc ||
      !Array.isArray(identity.packages) || identity.packages.length !== source.identity.packages.length ||
      identity.packages.some((value, index) => value !== source.identity.packages[index])) {
    throw new Error('Unavailable: runtime module, disc family or package order differs from the qualified Scene source');
  }
  const observedVblank = vblank(status.vblanks);
  const live = await readLivePc(request, source.loaded, source.actor);
  assertLiveComponent(live, source.loaded);
  const base = `field_view.components.descriptors[${source.actor}].actor`;
  const raw: number[] = [];
  for (let axis = 0; axis < 3; axis++) {
    raw.push(signed(await request({ cmd: 'inspect', path: `${base}->position[${axis}]`, depth: 0 }), 32, `actor position ${axis}`));
  }
  const heading = signed(await request({ cmd: 'inspect', path: `${base}->heading`, depth: 0 }), 16, 'actor heading');
  // actors.h declares signed 16.16 source-space XYZ; heading is a signed word.
  // No descriptor word is inferred from these transient runtime values.
  return {
    entity: source.entity, actor: source.actor, disc: live.disc, map: live.map,
    identity: { module: source.identity.module, disc: source.identity.disc, packages: [...source.identity.packages] },
    vblank: observedVblank, raw_position: raw as SceneVector,
    position: raw.map(value => value / 65536) as SceneVector, heading_raw: heading,
  };
}
