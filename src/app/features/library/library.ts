import { ChangeDetectionStrategy, Component, computed, inject, signal } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, RouterLink } from '@angular/router';
import { map } from 'rxjs';
import { formatSize } from '../../core/format';
import { LibraryService, seriesStats, volumeProgress } from '../../core/library.service';
import { Series } from '../../core/models';
import { Cover } from '../../shared/cover/cover';

type Filter = 'all' | 'manga' | 'manhwa' | 'reading' | 'unread' | 'finished';
type Sort = 'recent' | 'title' | 'size' | 'volumes';
type View = 'covers' | 'details';

const FILTERS: { id: Filter; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'manga', label: 'Manga' },
  { id: 'manhwa', label: 'Manhwa' },
  { id: 'reading', label: 'Reading' },
  { id: 'unread', label: 'Unread' },
  { id: 'finished', label: 'Finished' },
];
const SORTS: { id: Sort; label: string }[] = [
  { id: 'recent', label: 'Recently read' },
  { id: 'title', label: 'Title A–Z' },
  { id: 'size', label: 'Size on disk' },
  { id: 'volumes', label: 'Most volumes' },
];
const VIEW_KEY = 'koma.library.view';
const SORT_KEY = 'koma.library.sort';

function stored<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const value = localStorage.getItem(key) as T | null;
    return value && allowed.includes(value) ? value : fallback;
  } catch {
    return fallback;
  }
}

function remember(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Not remembered this time.
  }
}

function fileName(path: string): string {
  return path.slice(Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/')) + 1);
}

function matches(series: Series, query: string): boolean {
  if (series.title.toLowerCase().includes(query)) return true;
  if (series.titleJp?.toLowerCase().includes(query)) return true;
  return series.volumes.some((v) => fileName(v.path).toLowerCase().includes(query));
}

/** Every series, as a cover grid or a catalog list. */
@Component({
  selector: 'koma-library',
  imports: [RouterLink, Cover],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './library.html',
  styleUrl: './library.css',
})
export class LibraryScreen {
  protected readonly library = inject(LibraryService);

  protected readonly filters = FILTERS;
  protected readonly sorts = SORTS;
  protected readonly filter = signal<Filter>('all');
  protected readonly sort = signal<Sort>(stored(SORT_KEY, ['recent', 'title', 'size', 'volumes'], 'recent'));
  protected readonly view = signal<View>(stored(VIEW_KEY, ['covers', 'details'], 'covers'));
  /** Search text from the nav box, kept in the URL. */
  protected readonly query = toSignal(
    inject(ActivatedRoute).queryParamMap.pipe(map((params) => (params.get('q') ?? '').trim())),
    { initialValue: '' },
  );

  protected readonly summary = computed(() => {
    const series = this.library.series().length;
    const volumes = this.library.volumeCount();
    return `${series} series · ${volumes.toLocaleString()} ${volumes === 1 ? 'volume' : 'volumes'} · ${formatSize(this.library.totalSize())} · all local`;
  });

  protected readonly cards = computed(() => {
    const filter = this.filter();
    const query = this.query().toLowerCase();
    const rows = this.library
      .series()
      .map((series) => ({ series, stats: seriesStats(series) }))
      .filter(({ series, stats }) => {
        if (query && !matches(series, query)) return false;
        if (filter === 'manga' || filter === 'manhwa') return series.type === filter;
        return filter === 'all' || stats.status === filter;
      });

    const byTitle = (a: (typeof rows)[number], b: (typeof rows)[number]) =>
      a.series.title.localeCompare(b.series.title, undefined, { numeric: true, sensitivity: 'base' });
    switch (this.sort()) {
      case 'recent':
        rows.sort((a, b) => b.stats.lastRead.localeCompare(a.stats.lastRead) || byTitle(a, b));
        break;
      case 'title':
        rows.sort(byTitle);
        break;
      case 'size':
        rows.sort((a, b) => b.stats.size - a.stats.size || byTitle(a, b));
        break;
      case 'volumes':
        rows.sort((a, b) => b.stats.volumes - a.stats.volumes || byTitle(a, b));
        break;
    }

    return rows.map(({ series, stats }) => {
      const read = series.volumes.reduce((sum, v) => sum + volumeProgress(v), 0);
      return {
        id: series.id,
        title: series.title,
        coverId: series.volumes[0].id,
        type: series.type,
        readLine: `${stats.finished} / ${stats.volumes} ${stats.volumes === 1 ? 'volume' : 'volumes'} read`,
        volumes: `${stats.volumes} ${stats.volumes === 1 ? 'vol' : 'vols'}`,
        size: formatSize(stats.size),
        percent: Math.round((read / Math.max(1, stats.volumes)) * 100),
      };
    });
  });

  protected setView(view: View): void {
    this.view.set(view);
    remember(VIEW_KEY, view);
  }

  protected setSort(sort: string): void {
    this.sort.set(sort as Sort);
    remember(SORT_KEY, sort);
  }
}
