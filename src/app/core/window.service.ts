import { Injectable, signal } from '@angular/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

/** Thin wrapper over the Tauri window API. Does nothing when run in a plain browser (`ng serve`). */
@Injectable({ providedIn: 'root' })
export class WindowService {
  readonly inTauri = '__TAURI_INTERNALS__' in window;
  readonly maximized = signal(false);
  readonly fullscreen = signal(false);

  constructor() {
    if (!this.inTauri) return;
    const win = getCurrentWindow();
    const sync = () => win.isMaximized().then((m) => this.maximized.set(m));
    void sync();
    void win.onResized(() => void sync());
  }

  minimize(): void {
    if (this.inTauri) void getCurrentWindow().minimize();
  }

  toggleMaximize(): void {
    if (this.inTauri) void getCurrentWindow().toggleMaximize();
  }

  async setFullscreen(on: boolean): Promise<void> {
    if (!this.inTauri || this.fullscreen() === on) return;
    this.fullscreen.set(on);
    await getCurrentWindow().setFullscreen(on);
  }

  close(): void {
    if (this.inTauri) void getCurrentWindow().close();
  }
}
