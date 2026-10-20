// Copied from rpg_creator src/editor/editorCommands.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
export type EditorContextTarget =
  | { type: 'mapsRoot' }
  | { type: 'map'; mapId: string }
  | { type: 'layer'; mapId: string; layerId: string }
  | { type: 'tile'; tileId: string; tilesetId: string }
  | { type: 'mapCell'; mapId: string; x: number; y: number }
  | { type: 'mapEvent'; mapId: string; eventId: string; x: number; y: number }
  | {
      type: 'mapEventShape';
      mapId: string;
      eventId: string;
      pageId: string;
      role: 'solid' | 'touch' | 'action' | 'region';
      regionId?: string;
    }
  | { type: 'mapStart'; mapId: string; x: number; y: number }
  | { type: 'vehicle'; mapId: string; vehicleId: string; x: number; y: number }
  | {
      type: 'mapCollisionVertex';
      mapId: string;
      layerId: string;
      polygonId: string;
      pointIndex: number;
    }
  | {
      type: 'mapCollisionPolygon';
      mapId: string;
      layerId: string;
      polygonId: string;
    }
  | {
      type: 'mapImageInstance';
      mapId: string;
      layerId: string;
      imageId: string;
    }
  | {
      type: 'imageCollisionVertex';
      assetId: string;
      polygonId: string;
      pointIndex: number;
    }
  | {
      type: 'imageCollisionPolygon';
      assetId: string;
      polygonId: string;
    }
  | { type: 'eventPage'; eventId: string; pageId: string }
  | { type: 'eventCommand'; path: string }
  | { type: 'databaseRecord'; collection: string; recordId: string }
  | { type: 'asset'; assetKind: string; assetId: string }
  | { type: 'hd2dMaterial'; materialId: string }
  | {
      type: 'hd2dEventVolume';
      mapId: string;
      eventId: string;
      pageId: string;
      volumeId: string;
    }
  | { type: 'hd2dImage'; mapId: string; imageId: string }
  | { type: 'hd2dDecalDefinition'; definitionId: string }
  | { type: 'hd2dDecal'; mapId: string; decalId: string }
  | { type: 'hd2dLight'; mapId: string; lightId: string }
  | { type: 'hd2dFaceSelection'; mapId: string; faceIds: string[] }
  | {
      type: 'animationCell';
      animationId: string;
      frameId: string;
      cellId: string;
    }
  | {
      type: 'animationTimelineItem';
      animationId: string;
      itemKind: 'visual' | 'sound' | 'flash';
      itemId: string;
    }
  | { type: 'panel'; panelId: string }
  | { type: 'toolstrip'; toolstripId: string };

export interface EditorCommand {
  id: string;
  label: string;
  group?: string;
  checked?: boolean;
  dangerous?: boolean;
  swatchColor?: string;
  disabledReason?: string;
  children?: EditorCommand[];
  execute: () => void;
}

export interface EditorContextMenuRequest {
  target: EditorContextTarget;
  label: string;
  x: number;
  y: number;
  commands: EditorCommand[];
  restoreFocus?: HTMLElement | null;
}

export function contextMenuPointForElement(element: HTMLElement): {
  x: number;
  y: number;
} {
  const bounds = element.getBoundingClientRect();
  return {
    x: Math.min(window.innerWidth - 8, bounds.right),
    y: Math.min(window.innerHeight - 8, bounds.bottom)
  };
}

export function isContextMenuKey(event: {
  key: string;
  shiftKey: boolean;
}): boolean {
  return event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10');
}
