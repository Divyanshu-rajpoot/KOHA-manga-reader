import { ChangeDetectionStrategy, Component, computed, effect, inject, signal } from '@angular/core';
import { RouterLink } from '@angular/router';
import { formatAgo, formatSize, pad2 } from '../../core/format';
import { LibraryService, isFinished, resumeVolume, seriesStats, volumeProgress } from '../../core/library.service';
import { Cover } from '../../shared/cover/cover';
import { SegProgress } from '../../shared/seg-progress/seg-progress';
import { SeriesTable } from '../../shared/series-table/series-table';
import { Carousel, CarouselItem } from './carousel';
import { ReadingCard } from './reading-card';

const UP_NEXT_COUNT = 3;

/**
 * Home. The carousel of in-progress series drives everything else: the big
 * info column, the reading card and "up next" all follow the front cover.
 */
@Component({
  selector: 'koma-home',
  imports: [RouterLink, Carousel, Cover, ReadingCard, SegProgress, SeriesTable],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './home.html',
  styleUrl: './home.css',
})
export class Home {
  protected readonly library = inject(LibraryService);

  /** Series on the carousel: those being read, most recent first. If none are, every series. */
  protected readonly featured = computed(() => {
    const all = this.library.series().map((series) => ({ series, stats: seriesStats(series) }));
    const reading = all
      .filter((entry) => entry.stats.status === 'reading')
      .sort((a, b) => b.stats.lastRead.localeCompare(a.stats.lastRead));
    return { inProgress: reading.length > 0, entries: reading.length > 0 ? reading : all };
  });

  protected readonly items = computed<CarouselItem[]>(() =>
    this.featured().entries.map(({ series }) => {
      const volume = resumeVolume(series);
      return { seriesId: series.id, title: series.title, volumeId: volume.id, badge: pad2(volume.number) };
    }),
  );

  protected readonly index = signal(0);

  /** Everything the left column and the panels show, for the front cover. */
  protected readonly current = computed(() => {
    const entry = this.featured().entries[this.index()];
    if (!entry) return null;
    const { series, stats } = entry;
    const volume = resumeVolume(series);
    const volumeIndex = series.volumes.indexOf(volume);
    const progress = volumeProgress(volume);
    const unit = volume.unit === 'chapter' ? 'CH' : 'VOL';
    const direction = series.direction === 'rtl' ? 'RIGHT-TO-LEFT' : 'LEFT-TO-RIGHT';

    const upNext = series.volumes.slice(volumeIndex + 1, volumeIndex + 1 + UP_NEXT_COUNT).map((next) => ({
      id: next.id,
      label: `№${pad2(next.number)} · ${isFinished(next) ? 'READ' : next.progress ? `${Math.round(volumeProgress(next) * 100)}%` : 'UNREAD'} · ${formatSize(next.size)}`,
    }));
    const last = series.volumes[series.volumes.length - 1];

    return {
      series,
      volume,
      number: pad2(volume.number),
      eyebrow: `${volume.progress ? 'NOW READING' : 'ON THE SHELF'} · ${series.type.toUpperCase()} · SERIES ${String(this.library.series().indexOf(series) + 1).padStart(3, '0')} / ${String(this.library.series().length).padStart(3, '0')}`,
      meta: `${stats.volumes} ${stats.volumes === 1 ? 'VOL' : 'VOLS'} / ${formatSize(stats.size).toUpperCase()} / ${direction}`,
      progress,
      pageLine: volume.progress
        ? `P.${volume.progress.page} / ${volume.pages} — ${Math.round(progress * 100)}% OF ${unit} ${volume.number}`
        : `NOT STARTED · ${volume.pages} PAGES`,
      resumeLabel: volume.progress && !isFinished(volume) ? 'RESUME' : isFinished(volume) ? 'READ AGAIN' : 'START',
      bubble: stats.lastRead ? formatAgo(stats.lastRead) : 'new',
      cardTitle: `Reading card · ${unit === 'CH' ? 'Ch.' : 'Vol.'} ${pad2(volume.number)}`,
      upNext,
      /** Empty "up next" slots become one tile inviting the next volume. */
      importNumber: upNext.length < UP_NEXT_COUNT ? pad2(Math.floor(last.numberEnd ?? last.number) + 1) : null,
    };
  });

  protected readonly tag = computed(() => {
    const { inProgress, entries } = this.featured();
    return `${inProgress ? 'IN PROGRESS' : 'LIBRARY'} · ${entries.length} SERIES`;
  });

  constructor() {
    // Keep the index valid when the library changes under the carousel.
    effect(() => {
      const count = this.items().length;
      if (this.index() >= count) this.index.set(Math.max(0, count - 1));
    });
  }
}
