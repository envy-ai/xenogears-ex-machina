import type { FieldGraph } from '../../src/events/model';
export type EventView = 'list' | 'graph' | 'lanes';
export interface RenderTime { view: EventView; ms: number; actor: number; event: number; map: number; instructions: number }
export interface DiscSnapshot { id: number; disc: number | null | undefined; mapCount: number; placeholders: number }
export interface StudioTestSnapshot {
  view: EventView;
  selection: { actor: number; event: number; pc: number | null };
  map: number | null;
  instructions: number;
  discs: DiscSnapshot[];
  renderTimes: RenderTime[];
  draft: { revision:number; bindings:Record<string,number>; graph:FieldGraph; bytes:number[] } | null;
}
export interface StudioTestDriver {
  command(json: string): string;
  loadComponent(bytes: number[], declaredSize: number, map?: number): Promise<void>;
  snapshot(): StudioTestSnapshot;
}

declare global {
  interface Window { __xemStudioTest?: StudioTestDriver }
}
