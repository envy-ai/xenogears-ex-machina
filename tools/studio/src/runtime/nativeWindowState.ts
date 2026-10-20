// Copied from rpg_creator src/runtime/nativeWindowState.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
export type NativeWindowStateEvent =
  'maximize' | 'minimize' | 'restore' | 'enter-fullscreen';

export interface NativeWindowStateSource {
  on(event: NativeWindowStateEvent, listener: () => void): void;
  removeListener(event: NativeWindowStateEvent, listener: () => void): void;
}

/** NW.js restore also means returning from minimize or fullscreen. */
export class NativeWindowStateTracker {
  maximized: boolean;
  private maximizedBeforeMinimize: boolean | null = null;
  private maximizedBeforeFullscreen: boolean | null = null;

  get minimized() {
    return this.maximizedBeforeMinimize !== null;
  }

  constructor(
    private readonly source: NativeWindowStateSource,
    private readonly onChange: () => void,
    initialMaximized = false
  ) {
    this.maximized = initialMaximized;
    source.on('maximize', this.handleMaximize);
    source.on('minimize', this.handleMinimize);
    source.on('restore', this.handleRestore);
    source.on('enter-fullscreen', this.handleEnterFullscreen);
  }

  private readonly handleMaximize = () => {
    this.maximized = true;
    this.onChange();
  };

  private readonly handleMinimize = () => {
    if (this.maximizedBeforeMinimize === null) {
      this.maximizedBeforeMinimize = this.maximized;
    }
    this.onChange();
  };

  private readonly handleEnterFullscreen = () => {
    if (this.maximizedBeforeFullscreen === null) {
      this.maximizedBeforeFullscreen = this.maximized;
    }
    this.onChange();
  };

  private readonly handleRestore = () => {
    if (this.maximizedBeforeMinimize !== null) {
      this.maximized = this.maximizedBeforeMinimize;
      this.maximizedBeforeMinimize = null;
    } else if (this.maximizedBeforeFullscreen !== null) {
      this.maximized = this.maximizedBeforeFullscreen;
      this.maximizedBeforeFullscreen = null;
    } else {
      this.maximized = false;
    }
    this.onChange();
  };

  dispose() {
    this.source.removeListener('maximize', this.handleMaximize);
    this.source.removeListener('minimize', this.handleMinimize);
    this.source.removeListener('restore', this.handleRestore);
    this.source.removeListener('enter-fullscreen', this.handleEnterFullscreen);
  }
}
