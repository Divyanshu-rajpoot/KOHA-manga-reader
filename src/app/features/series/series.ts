import { ChangeDetectionStrategy, Component, computed, effect, inject, signal, untracked } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router, RouterLink } from '@angular/router';
import { map } from 'rxjs';
import { formatSize, formatVolume, pad2 } from '../../core/format';
import { KomaApi } from '../../core/koma-api';
import { LibraryService, isFinished, resumeVolume, seriesStats, volumeProgress } from '../../core/library.service';
import { Direction, FileInfo, SeriesType } from '../../core/models';
import { Cover } from '../../shared/cover/cover';

interface Draft {
  title: string;
  titleJp: string;
  type: SeriesType;
  direction: Direction;
}

function splitPath(path: string): { folder: string; file: string } {
  const at = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
  return { folder: path.slice(0, Math.max(0, at)), file: path.slice(at + 1) };
}

/** One series: its details, volumes and the files behind them. */
@Component({
  selector: 'koma-series',
  imports: [RouterLink, Cover],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './series.html',
  styleUrl: './series.css',
})
export class SeriesScreen {
  private readonly api = inject(KomaApi);
  private readonly router = inject(Router);
  protected readonly library = inject(LibraryService);

  private readonly seriesId = toSignal(
    inject(ActivatedRoute).paramMap.pipe(map((params) => params.get('seriesId') ?? '')),
    { initialValue: '' },
  );
  protected readonly series = computed(() => this.library.findSeries(this.seriesId()));

  /** The edit form, or null when it is closed. */
  protected readonly draft = signal<Draft | null>(null);
  protected readonly confirmRemove = signal(false);
  protected readonly error = signal<string | null>(null);
  private readonly files = signal(new Map<string, FileInfo>());

  protected readonly view = computed(() => {
    const series = this.series();
    if (!series) return null;
    const stats = seriesStats(series);
    const resume = resumeVolume(series);
    const label = formatVolume(resume.unit, resume.number, resume.numberEnd);
    const first = series.volumes[0];
    const last = series.volumes[series.volumes.length - 1];
    const chapters = last.unit === 'chapter';
    return {
      series,
      resume,
      badge: pad2(resume.number),
      source:
        first.storage === 'copied'
          ? `In the KOMA library · ${splitPath(first.path).folder}`
          : `Linked from ${splitPath(first.path).folder}`,
      chips: [
        series.type === 'manga' ? 'Manga' : 'Manhwa',
        series.direction === 'rtl' ? 'Right to left' : 'Left to right',
        first.storage === 'copied' ? 'Copied' : 'Linked',
      ],
      stats: [
        { value: String(stats.volumes), label: chapters ? 'files' : stats.volumes === 1 ? 'volume' : 'volumes' },
        { value: String(stats.finished), label: 'finished' },
        { value: stats.pages.toLocaleString(), label: 'pages' },
        { value: formatSize(stats.size), label: 'on disk' },
      ],
      resumeLabel: isFinished(resume)
        ? `Read ${label} again`
        : resume.progress
          ? `Resume ${label} · p.${resume.progress.page}`
          : `Start ${label}`,
      heading: chapters ? 'Chapters' : 'Volumes',
      addLabel: `${chapters ? 'Ch.' : 'Vol.'} ${pad2(Math.floor(last.numberEnd ?? last.number) + 1)}`,
    };
  });

  protected readonly volumes = computed(() => {
    const files = this.files();
    return (this.series()?.volumes ?? []).map((volume) => {
      const progress = volumeProgress(volume);
      const finished = isFinished(volume);
      const info = files.get(volume.id);
      return {
        id: volume.id,
        badge: pad2(volume.number),
        label: formatVolume(volume.unit, volume.number, volume.numberEnd),
        status: finished ? 'Read' : volume.progress ? `${Math.round(progress * 100)}%` : 'New',
        percent: Math.round(progress * 100),
        finished,
        file: splitPath(volume.path).file,
        path: volume.path,
        size: formatSize(volume.size),
        pages: volume.pages,
        archive: info ? (info.missing ? 'FILE MISSING' : info.archive) : '…',
        opened: info && !info.missing ? `${info.openMs < 10 ? info.openMs.toFixed(1) : Math.round(info.openMs)} ms` : '—',
        missing: !!info?.missing,
      };
    });
  });

  /** Changes whenever the set of files does, so the files table is re-read. */
  private readonly fileKey = computed(() => (this.series()?.volumes ?? []).map((v) => v.id).join('|'));

  constructor() {
    effect(() => {
      const key = this.fileKey();
      const id = untracked(() => this.seriesId());
      if (!key || !this.api.inTauri) return;
      void this.api
        .getSeriesFiles(id)
        .then((infos) => {
          if (this.fileKey() === key) this.files.set(new Map(infos.map((info) => [info.volumeId, info])));
        })
        .catch(() => undefined);
    });

    // Leaving for another series closes the form.
    effect(() => {
      this.seriesId();
      untracked(() => {
        this.draft.set(null);
        this.confirmRemove.set(false);
        this.error.set(null);
      });
    });
  }

  // --- editing ---

  protected startEdit(): void {
    const series = this.series();
    if (!series) return;
    this.confirmRemove.set(false);
    this.draft.set({
      title: series.title,
      titleJp: series.titleJp ?? '',
      type: series.type,
      direction: series.direction,
    });
  }

  protected patchDraft(patch: Partial<Draft>): void {
    this.draft.update((draft) => (draft ? { ...draft, ...patch } : draft));
  }

  protected async saveEdit(): Promise<void> {
    const draft = this.draft();
    const id = this.seriesId();
    if (!draft || draft.title.trim() === '') return;
    this.error.set(null);
    try {
      const newId = await this.api.updateSeries(id, {
        title: draft.title,
        titleJp: draft.titleJp.trim() || null,
        seriesType: draft.type,
        direction: draft.direction,
      });
      this.draft.set(null);
      await this.library.refresh();
      // A title that matches another series merges into it.
      if (newId !== id) void this.router.navigate(['/series', newId], { replaceUrl: true });
    } catch (err) {
      this.error.set(String(err));
    }
  }

  protected async removeSeries(): Promise<void> {
    this.error.set(null);
    try {
      await this.api.removeSeries(this.seriesId());
      await this.library.refresh();
      void this.router.navigate(['/library'], { replaceUrl: true });
    } catch (err) {
      this.error.set(String(err));
    }
  }

  // --- volumes ---

  protected setRead(volumeId: string, read: boolean): void {
    this.error.set(null);
    void this.api.setRead(volumeId, read).catch((err) => this.error.set(String(err)));
  }

  protected async removeVolume(volumeId: string): Promise<void> {
    const wasLast = this.series()?.volumes.length === 1;
    this.error.set(null);
    try {
      await this.api.removeVolume(volumeId);
      await this.library.refresh();
      if (wasLast) void this.router.navigate(['/library'], { replaceUrl: true });
    } catch (err) {
      this.error.set(String(err));
    }
  }
}
