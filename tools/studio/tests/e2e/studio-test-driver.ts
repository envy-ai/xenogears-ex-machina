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
}
export interface StudioTestDriver {
  loadComponent(bytes: number[], declaredSize: number, map?: number): Promise<void>;
  snapshot(): StudioTestSnapshot;
}

declare global {
  interface Window { __xemStudioTest?: StudioTestDriver }
}
