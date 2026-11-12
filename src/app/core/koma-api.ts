import { Injectable } from '@angular/core';
import { invoke } from '@tauri-apps/api/core';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { open } from '@tauri-apps/plugin-dialog';
import {
  AppInfo,
  CacheInfo,
  Candidate,
  Direction,
  FileInfo,
  ImportItem,
  ImportProgress,
  Library,
  SeriesType,
  Settings,
  Storage,
} from './models';

export type DropEvent = { type: 'hover' } | { type: 'leave' } | { type: 'drop'; paths: string[] };

/** Wrappers over the Rust commands, events and the `koma://` image protocol. */
@Injectable({ providedIn: 'root' })
export class KomaApi {
  readonly inTauri = '__TAURI_INTERNALS__' in window;

  // WebView2 exposes custom schemes as http://{scheme}.localhost/.
  private readonly base = navigator.userAgent.includes('Windows') ? 'http://koma.localhost' : 'koma:/';

  // --- images: bytes come straight from Rust, never through JSON ---

  /** With `width`, Rust resizes the page to that many pixels wide. */
  pageUrl(volumeId: string, index: number, width?: number): string {
    const query = width ? `?w=${width}` : '';
    return `${this.base}/page/${volumeId}/${index}${query}`;
  }

  coverUrl(volumeId: string): string {
    return `${this.base}/cover/${volumeId}`;
  }

  thumbUrl(volumeId: string, index: number): string {
    return `${this.base}/thumb/${volumeId}/${index}`;
  }

  // --- library ---

  getLibrary(): Promise<Library> {
    return invoke<Library>('get_library');
  }

  getAppInfo(): Promise<AppInfo> {
    return invoke<AppInfo>('get_app_info');
  }

  onLibraryChanged(handler: () => void): Promise<UnlistenFn> {
    return listen('library-changed', handler);
  }

  /** Returns the id of the series that now holds the volumes (it changes on a merge). */
  updateSeries(
    seriesId: string,
    edit: { title: string; titleJp: string | null; seriesType: SeriesType; direction: Direction },
  ): Promise<string> {
    return invoke<string>('update_series', { seriesId, ...edit });
  }

  /** Removes from the library only. The file on disk is not touched. */
  removeVolume(volumeId: string): Promise<void> {
    return invoke<void>('remove_volume', { volumeId });
  }

  removeSeries(seriesId: string): Promise<void> {
    return invoke<void>('remove_series', { seriesId });
  }

  setRead(volumeId: string, read: boolean): Promise<void> {
    return invoke<void>('set_read', { volumeId, read, now: new Date().toISOString() });
  }

  getSeriesFiles(seriesId: string): Promise<FileInfo[]> {
    return invoke<FileInfo[]>('get_series_files', { seriesId });
  }

  // --- settings and caches ---

  getSettings(): Promise<Settings> {
    return invoke<Settings>('get_settings');
  }

  /** Returns the settings as stored (out-of-range values are corrected). */
  saveSettings(settings: Settings): Promise<Settings> {
    return invoke<Settings>('save_settings', { settings });
  }

  getCacheInfo(): Promise<CacheInfo> {
    return invoke<CacheInfo>('get_cache_info');
  }

  clearThumbCache(): Promise<void> {
    return invoke<void>('clear_thumb_cache');
  }

  /** New CBZ and PDF files found in the watched folders (not in the library yet). */
  onWatchFound(handler: (paths: string[]) => void): Promise<UnlistenFn> {
    return listen<string[]>('watch-found', (event) => handler(event.payload));
  }

  checkWatchFolders(): Promise<string[]> {
    return invoke<string[]>('check_watch_folders');
  }

  async pickFolder(): Promise<string | null> {
    return open({ multiple: false, directory: true });
  }

  // --- reader ---

  /** `[width, height]` of every page of a volume. */
  getPageSizes(volumeId: string): Promise<[number, number][]> {
    return invoke<[number, number][]>('get_page_sizes', { volumeId });
  }

  /** Page indexes ready in the memory cache. */
  getCachedPages(volumeId: string): Promise<number[]> {
    return invoke<number[]>('get_cached_pages', { volumeId });
  }

  /** Saves the page being read (1-based). */
  setProgress(volumeId: string, page: number): Promise<void> {
    const now = new Date();
    const pad = (n: number) => String(n).padStart(2, '0');
    const today = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
    return invoke<void>('set_progress', { volumeId, page, now: now.toISOString(), today });
  }

  // --- import ---

  /** File picker for CBZ, ZIP and PDF files. Returns [] if cancelled. */
  async pickFiles(): Promise<string[]> {
    const picked = await open({
      multiple: true,
      directory: false,
      filters: [{ name: 'Comics (CBZ, ZIP, PDF)', extensions: ['cbz', 'zip', 'pdf'] }],
    });
    return picked ?? [];
  }

  /** Folder picker. Returns [] if cancelled. */
  async pickFolders(): Promise<string[]> {
    const picked = await open({ multiple: true, directory: true });
    return picked ?? [];
  }

  scanImport(paths: string[]): Promise<Candidate[]> {
    return invoke<Candidate[]>('scan_import', { paths });
  }

  runImport(items: ImportItem[], storage: Storage): Promise<void> {
    return invoke<void>('run_import', { items, storage });
  }

  onImportProgress(handler: (progress: ImportProgress) => void): Promise<UnlistenFn> {
    return listen<ImportProgress>('import-progress', (event) => handler(event.payload));
  }

  onImportFinished(handler: () => void): Promise<UnlistenFn> {
    return listen('import-finished', handler);
  }

  /** Files or folders dragged onto the window from Explorer. */
  onFileDrop(handler: (event: DropEvent) => void): Promise<UnlistenFn> {
    return getCurrentWebview().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === 'drop') handler({ type: 'drop', paths: payload.paths });
      else if (payload.type === 'leave') handler({ type: 'leave' });
      else handler({ type: 'hover' });
    });
  }
}
