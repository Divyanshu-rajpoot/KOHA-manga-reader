import { Injectable, computed, inject, signal } from '@angular/core';
import { KomaApi } from './koma-api';
import { Library, Series, Volume } from './models';

export type SeriesStatus = 'reading' | 'finished' | 'unread';

export interface SeriesStats {
  volumes: number;
  finished: number;
  pages: number;
  size: number;
  /** ISO time the series was last read, or '' if never. */
  lastRead: string;
  status: SeriesStatus;
}

export function isFinished(volume: Volume): boolean {
  return !!volume.progress && volume.progress.page >= volume.pages;
}

/** 0 to 1. */
export function volumeProgress(volume: Volume): number {
  if (!volume.progress || volume.pages === 0) return 0;
  return Math.min(1, volume.progress.page / volume.pages);
}

export function seriesStats(series: Series): SeriesStats {
  const volumes = series.volumes;
  const finished = volumes.filter(isFinished).length;
  const started = volumes.some((v) => v.progress);
  return {
    volumes: volumes.length,
    finished,
    pages: volumes.reduce((sum, v) => sum + v.pages, 0),
    size: volumes.reduce((sum, v) => sum + v.size, 0),
    lastRead: volumes.reduce((latest, v) => (v.progress && v.progress.updatedAt > latest ? v.progress.updatedAt : latest), ''),
    status: finished === volumes.length ? 'finished' : started ? 'reading' : 'unread',
  };
}

/**
 * The volume to open for a series: the one read most recently, or, if that one
 * is finished, the next unfinished volume after it. Never-read series start at
 * the first volume.
 */
export function resumeVolume(series: Series): Volume {
  const volumes = series.volumes;
  let recent = -1;
  volumes.forEach((v, i) => {
    if (v.progress && (recent < 0 || v.progress.updatedAt > volumes[recent].progress!.updatedAt)) recent = i;
  });
  if (recent < 0) return volumes[0];
  if (!isFinished(volumes[recent])) return volumes[recent];
  return volumes.slice(recent + 1).find((v) => !isFinished(v)) ?? volumes[recent];
}

/** The library, kept in sync with the Rust side. */
@Injectable({ providedIn: 'root' })
export class LibraryService {
  private readonly api = inject(KomaApi);

  readonly library = signal<Library>({ version: 1, series: [] });
  /** False until the first load from disk has finished. */
  readonly loaded = signal(false);
  readonly series = computed(() => this.library().series);
  readonly volumeCount = computed(() => this.series().reduce((sum, s) => sum + s.volumes.length, 0));
  readonly totalSize = computed(() =>
    this.series().reduce((sum, s) => sum + s.volumes.reduce((v, volume) => v + volume.size, 0), 0),
  );

  constructor() {
    if (!this.api.inTauri) {
      this.loaded.set(true);
      return;
    }
    void this.refresh();
    void this.api.onLibraryChanged(() => void this.refresh());
  }

  async refresh(): Promise<void> {
    this.library.set(await this.api.getLibrary());
    this.loaded.set(true);
  }

  findSeries(seriesId: string): Series | null {
    return this.series().find((s) => s.id === seriesId) ?? null;
  }

  /** Finds a volume and the series it belongs to. */
  findVolume(volumeId: string): { series: Series; volume: Volume; index: number } | null {
    for (const series of this.series()) {
      const index = series.volumes.findIndex((v) => v.id === volumeId);
      if (index >= 0) return { series, volume: series.volumes[index], index };
    }
    return null;
  }
}
