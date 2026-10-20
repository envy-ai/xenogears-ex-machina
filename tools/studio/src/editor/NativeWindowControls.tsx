// Copied from rpg_creator src/editor/NativeWindowControls.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import { useEffect, useState } from 'react';
import { Copy, Minus, Square, X } from 'lucide-react';
import {
  NativeWindowStateTracker,
  type NativeWindowStateEvent
} from '../runtime/nativeWindowState';
import type { NativeWindowGeometry } from '../runtime/nativeWindowGeometry';

type NativeControlWindowEvent = NativeWindowStateEvent | 'resize' | 'move';

interface NwWindowLike {
  x: number;
  y: number;
  isFullscreen: boolean;
  minimize: () => void;
  maximize: () => void;
  restore: () => void;
  close: (force?: boolean) => void;
  on: (event: NativeControlWindowEvent, listener: () => void) => void;
  removeListener: (
    event: NativeControlWindowEvent,
    listener: () => void
  ) => void;
}

interface NwRuntimeLike {
  Window?: {
    get?: () => NwWindowLike;
  };
}

export interface NativeWindowControlBridge {
  maximized: boolean;
  normalGeometry: NativeWindowGeometry;
  minimize: () => void;
  toggleMaximized: () => void;
  close: () => void;
}

function currentNwWindow(): NwWindowLike | null {
  const runtime = (globalThis as typeof globalThis & { nw?: NwRuntimeLike }).nw;
  return runtime?.Window?.get?.() ?? null;
}

export function useNativeWindowControls(): NativeWindowControlBridge | null {
  const [nativeWindow] = useState(currentNwWindow);
  const [maximized, setMaximized] = useState(false);
  const [normalGeometry, setNormalGeometry] = useState<NativeWindowGeometry | null>(
    () => nativeWindow ? {
      x: nativeWindow.x, y: nativeWindow.y,
      width: window.innerWidth, height: window.innerHeight
    } : null
  );

  useEffect(() => {
    if (!nativeWindow) return;
    let frame: number | null = null;
    const captureNormalGeometry = () => {
      if (frame !== null) window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        frame = null;
        if (state.maximized || state.minimized || nativeWindow.isFullscreen ||
            document.fullscreenElement !== null) return;
        const geometry = {
          x: nativeWindow.x, y: nativeWindow.y,
          width: window.innerWidth, height: window.innerHeight
        };
        setNormalGeometry((current) => current &&
          current.x === geometry.x && current.y === geometry.y &&
          current.width === geometry.width && current.height === geometry.height
            ? current : geometry);
      });
    };
    const state = new NativeWindowStateTracker(nativeWindow, () => {
      setMaximized(state.maximized);
      captureNormalGeometry();
    });
    nativeWindow.on('resize', captureNormalGeometry);
    nativeWindow.on('move', captureNormalGeometry);
    window.addEventListener('resize', captureNormalGeometry);
    return () => {
      if (frame !== null) window.cancelAnimationFrame(frame);
      state.dispose();
      nativeWindow.removeListener('resize', captureNormalGeometry);
      nativeWindow.removeListener('move', captureNormalGeometry);
      window.removeEventListener('resize', captureNormalGeometry);
    };
  }, [nativeWindow]);

  if (!nativeWindow) return null;
  return {
    maximized,
    normalGeometry: normalGeometry!,
    minimize: () => nativeWindow.minimize(),
    toggleMaximized: () => {
      if (maximized) nativeWindow.restore();
      else nativeWindow.maximize();
    },
    close: () => nativeWindow.close()
  };
}

export function NativeWindowControls({
  bridge,
  placement
}: {
  bridge: NativeWindowControlBridge | null;
  placement: 'editor' | 'runtime';
}) {
  if (!bridge) return null;
  return (
    <div
      className="native-window-controls"
      data-placement={placement}
      role="group"
      aria-label="Window controls"
    >
      <button
        type="button"
        className="editor-button native-window-minimize"
        aria-label="Minimize window"
        title="Minimize"
        onClick={bridge.minimize}
      >
        <Minus size={15} strokeWidth={1.8} />
      </button>
      <button
        type="button"
        className="editor-button native-window-maximize"
        aria-label={bridge.maximized ? 'Restore window' : 'Maximize window'}
        title={bridge.maximized ? 'Restore' : 'Maximize'}
        onClick={bridge.toggleMaximized}
      >
        {bridge.maximized ? (
          <Copy size={13} strokeWidth={1.6} />
        ) : (
          <Square size={12} strokeWidth={1.7} />
        )}
      </button>
      <button
        type="button"
        className="editor-button native-window-close"
        aria-label="Close window"
        title="Close"
        onClick={bridge.close}
      >
        <X size={16} strokeWidth={1.8} />
      </button>
    </div>
  );
}
