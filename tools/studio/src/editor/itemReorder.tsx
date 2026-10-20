// Copied from rpg_creator src/editor/itemReorder.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import {
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type DragEvent as ReactDragEvent,
  type HTMLAttributes
} from 'react';
import { GripVertical, Move } from 'lucide-react';

export type ReorderDropPosition = 'before' | 'after';

export interface ItemReorderPreview {
  sourceId: string;
  targetId: string;
  position: ReorderDropPosition;
}

export function reorderDestinationIndex(
  sourceIndex: number,
  targetIndex: number,
  position: ReorderDropPosition
): number {
  const insertionIndex = targetIndex + (position === 'after' ? 1 : 0);
  return Math.max(0, insertionIndex - (sourceIndex < insertionIndex ? 1 : 0));
}

interface ItemReorderOptions {
  enabled?: boolean;
  orientation?: 'horizontal' | 'vertical';
  canDrop?: (sourceId: string, targetId: string) => boolean;
  onReorder: (
    sourceId: string,
    targetId: string,
    position: ReorderDropPosition
  ) => void;
}

export type ReorderTargetProps = Pick<
  HTMLAttributes<HTMLElement>,
  'onDragOverCapture' | 'onDropCapture'
> & {
  'data-reorder-id': string;
  'data-reorder-dragging'?: 'true';
  'data-reorder-target'?: ReorderDropPosition;
  'data-reorder-orientation'?: 'horizontal' | 'vertical';
};

export type ReorderHandleProps = Pick<
  HTMLAttributes<HTMLElement>,
  'draggable' | 'onDragStart' | 'onDragEnd'
>;

export type OutlineReorderHandleProps = Pick<
  ButtonHTMLAttributes<HTMLButtonElement>,
  | 'onClick'
  | 'onKeyDown'
  | 'onPointerCancel'
  | 'onPointerDown'
  | 'onPointerMove'
  | 'onPointerUp'
>;

export function useItemReorder({
  enabled = true,
  orientation = 'vertical',
  canDrop = () => true,
  onReorder
}: ItemReorderOptions) {
  const sourceIdRef = useRef<string | null>(null);
  const [sourceId, setSourceId] = useState<string | null>(null);
  const [target, setTarget] = useState<{
    id: string;
    position: ReorderDropPosition;
  } | null>(null);
  const targetRef = useRef<{
    id: string;
    position: ReorderDropPosition;
  } | null>(null);

  function reset() {
    sourceIdRef.current = null;
    targetRef.current = null;
    setSourceId(null);
    setTarget(null);
  }

  function targetPosition(
    event: ReactDragEvent<HTMLElement>
  ): ReorderDropPosition {
    const anchor = event.currentTarget.querySelector<HTMLElement>(
      ':scope > .command-heading'
    );
    const bounds = (anchor ?? event.currentTarget).getBoundingClientRect();
    if (orientation === 'horizontal') {
      return event.clientX < bounds.left + bounds.width / 2 ? 'before' : 'after';
    }
    return event.clientY < bounds.top + bounds.height / 2 ? 'before' : 'after';
  }

  function targetProps(id: string): ReorderTargetProps {
    return {
      'data-reorder-id': id,
      'data-reorder-dragging': sourceId === id ? 'true' : undefined,
      'data-reorder-target': target?.id === id ? target.position : undefined,
      'data-reorder-orientation': orientation,
      onDragOverCapture: (event) => {
        const source = sourceIdRef.current;
        if (!enabled || !source) return;
        if (source === id) {
          if (!targetRef.current) return;
          event.preventDefault();
          event.dataTransfer.dropEffect = 'move';
          return;
        }
        if (!canDrop(source, id)) return;
        event.preventDefault();
        event.dataTransfer.dropEffect = 'move';
        const position = targetPosition(event);
        targetRef.current = { id, position };
        setTarget((current) =>
          current?.id === id && current.position === position
            ? current
            : { id, position }
        );
      },
      onDropCapture: (event) => {
        const source = sourceIdRef.current;
        const placement = targetRef.current;
        const canUsePlacement =
          source !== null &&
          placement !== null &&
          canDrop(source, placement.id) &&
          (id === source || placement.id === id);
        if (!enabled || !source || !placement || !canUsePlacement) {
          reset();
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        onReorder(source, placement.id, placement.position);
        reset();
      }
    };
  }

  function handleProps(id: string): ReorderHandleProps {
    return {
      draggable: enabled,
      onDragStart: (event) => {
        if (!enabled) {
          event.preventDefault();
          return;
        }
        sourceIdRef.current = id;
        setSourceId(id);
        event.dataTransfer.effectAllowed = 'move';
        event.dataTransfer.setData('text/plain', id);
      },
      onDragEnd: reset
    };
  }

  const preview: ItemReorderPreview | null =
    sourceId && target
      ? { sourceId, targetId: target.id, position: target.position }
      : null;

  return { handleProps, preview, targetProps };
}

export function previewItemReorder<T extends { id: string }>(
  items: readonly T[],
  preview: ItemReorderPreview | null
): readonly T[] {
  if (!preview) return items;
  const sourceIndex = items.findIndex((item) => item.id === preview.sourceId);
  const targetIndex = items.findIndex((item) => item.id === preview.targetId);
  if (sourceIndex < 0 || targetIndex < 0) return items;
  const destinationIndex = reorderDestinationIndex(
    sourceIndex,
    targetIndex,
    preview.position
  );
  if (destinationIndex === sourceIndex) return items;
  const reordered = [...items];
  const [source] = reordered.splice(sourceIndex, 1);
  reordered.splice(destinationIndex, 0, source);
  return reordered;
}

export function ReorderGrip({
  label,
  props,
  enabled = true
}: {
  label: string;
  props: ReorderHandleProps;
  enabled?: boolean;
}) {
  return (
    <span
      className="item-reorder-grip"
      role="img"
      aria-label={`Drag ${label} to reorder`}
      title={enabled ? `Drag ${label} to reorder` : 'Clear filtering to reorder'}
      data-reorder-enabled={enabled ? 'true' : 'false'}
      {...props}
    >
      <GripVertical size={14} aria-hidden="true" />
    </span>
  );
}

export function OutlineReorderGrip({
  label,
  props,
  enabled = true
}: {
  label: string;
  props: OutlineReorderHandleProps;
  enabled?: boolean;
}) {
  const description = enabled
    ? `Move ${label} in any direction; drag or use arrow keys to reorder or change nesting`
    : 'Clear filtering to reorder or change nesting';
  return (
    <button
      type="button"
      className="editor-button item-reorder-grip outline-reorder-grip"
      aria-label={description}
      title={description}
      data-reorder-enabled={enabled ? 'true' : 'false'}
      disabled={!enabled}
      {...props}
    >
      <Move size={14} aria-hidden="true" />
    </button>
  );
}
