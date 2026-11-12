import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { formatSize } from '../../core/format';
import { KomaApi } from '../../core/koma-api';
import { CacheInfo, Settings } from '../../core/models';
import { SettingsService } from '../../core/settings.service';

const CACHE_SIZES_MB = [256, 512, 1024, 2048, 4096, 8192];
const MAX_AHEAD = 16;
const MAX_BEHIND = 8;

/** Appearance, reading, cache and folder settings. Every change is saved at once. */
@Component({
  selector: 'koma-settings',
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './settings.html',
  styleUrl: './settings.css',
})
export class SettingsScreen {
  private readonly api = inject(KomaApi);
  private readonly service = inject(SettingsService);

  protected readonly inTauri = this.api.inTauri;
  protected readonly settings = this.service.settings;
  protected readonly cacheSizes = CACHE_SIZES_MB;
  protected readonly maxAhead = MAX_AHEAD;
  protected readonly maxBehind = MAX_BEHIND;

  protected readonly cache = signal<CacheInfo | null>(null);
  protected readonly dataFolder = signal('');
  protected readonly clearing = signal(false);
  protected readonly error = signal<string | null>(null);

  protected readonly thumbSize = computed(() => {
    const cache = this.cache();
    return cache ? (cache.thumbBytes === 0 ? 'Empty' : formatSize(cache.thumbBytes)) : '…';
  });
  protected readonly pageSize = computed(() => {
    const cache = this.cache();
    return cache ? (cache.pageBytes === 0 ? 'Empty' : formatSize(cache.pageBytes)) : '…';
  });

  constructor() {
    if (!this.inTauri) return;
    void this.refreshCache();
    void this.api.getAppInfo().then((info) => this.dataFolder.set(info.dataFolder));
  }

  protected set(patch: Partial<Settings>): void {
    this.error.set(null);
    void this.service.update(patch).catch((err) => this.error.set(String(err)));
  }

  protected cacheLabel(mb: number): string {
    return mb >= 1024 ? `${mb / 1024} GB` : `${mb} MB`;
  }

  protected step(key: 'preloadAhead' | 'preloadBehind', delta: number): void {
    const max = key === 'preloadAhead' ? MAX_AHEAD : MAX_BEHIND;
    const next = Math.min(max, Math.max(0, this.settings()[key] + delta));
    if (next !== this.settings()[key]) this.set({ [key]: next });
  }

  protected async chooseLibraryFolder(): Promise<void> {
    const folder = await this.api.pickFolder();
    if (folder) this.set({ libraryFolder: folder });
  }

  protected async addWatchFolder(): Promise<void> {
    const folder = await this.api.pickFolder();
    if (!folder || this.settings().watchFolders.includes(folder)) return;
    this.set({ watchFolders: [...this.settings().watchFolders, folder] });
  }

  protected removeWatchFolder(folder: string): void {
    this.set({ watchFolders: this.settings().watchFolders.filter((f) => f !== folder) });
  }

  protected async clearThumbs(): Promise<void> {
    this.clearing.set(true);
    this.error.set(null);
    try {
      await this.api.clearThumbCache();
    } catch (err) {
      this.error.set(String(err));
    } finally {
      this.clearing.set(false);
      await this.refreshCache();
    }
  }

  private async refreshCache(): Promise<void> {
    this.cache.set(await this.api.getCacheInfo().catch(() => null));
  }
}
