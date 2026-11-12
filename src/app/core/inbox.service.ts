import { Injectable, inject, signal } from '@angular/core';
import { Router } from '@angular/router';
import { KomaApi } from './koma-api';

/**
 * Files waiting to be looked at by the Import screen: dropped anywhere on the
 * window, or found in a watched folder.
 */
@Injectable({ providedIn: 'root' })
export class ImportInbox {
  private readonly api = inject(KomaApi);
  private readonly router = inject(Router);

  /** Paths the Import screen should scan next. It empties this when it takes them. */
  readonly pending = signal<string[]>([]);
  /** New CBZ and PDF files sitting in the watched folders. */
  readonly watchFound = signal<string[]>([]);
  /** Files are being dragged over the window. */
  readonly dragging = signal(false);

  constructor() {
    if (!this.api.inTauri) return;
    void this.api.onWatchFound((paths) => this.watchFound.set(paths));
    void this.api.onFileDrop((event) => {
      this.dragging.set(event.type === 'hover');
      if (event.type === 'drop' && event.paths.length > 0) this.send(event.paths);
    });
  }

  /** Hands paths to the Import screen and opens it. */
  send(paths: string[]): void {
    this.pending.update((current) => [...current, ...paths]);
    void this.router.navigateByUrl('/import');
  }

  /** Takes the waiting paths, leaving the inbox empty. */
  take(): string[] {
    const paths = this.pending();
    if (paths.length > 0) this.pending.set([]);
    return paths;
  }

  async recheckWatchFolders(): Promise<void> {
    if (this.api.inTauri) this.watchFound.set(await this.api.checkWatchFolders());
  }
}
