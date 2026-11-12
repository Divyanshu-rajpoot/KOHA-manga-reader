import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  signal,
  untracked,
} from '@angular/core';
import { formatSize } from '../../core/format';
import { ImportInbox } from '../../core/inbox.service';
import { KomaApi } from '../../core/koma-api';
import { LibraryService } from '../../core/library.service';
import { RouterLink } from '@angular/router';
import { Candidate, ImportItem, ImportProgress, Storage } from '../../core/models';
import { QueueRow, QueueRowView, RowEdit, RowStatus } from './queue-row';

function initialStatus(candidate: Candidate): RowStatus {
  if (candidate.error) return 'error';
  if (candidate.duplicate) return 'duplicate';
  return candidate.matched ? 'ready' : 'needs';
}

function toRow(candidate: Candidate): QueueRow {
  return {
    candidate,
    series: candidate.series,
    number: candidate.number,
    numberEnd: candidate.numberEnd,
    unit: candidate.unit,
    status: initialStatus(candidate),
    step: null,
    fraction: 0,
    thumbsDone: 0,
    message: candidate.error,
    volumeId: null,
  };
}

@Component({
  selector: 'koma-import',
  imports: [QueueRowView, RouterLink],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './import.html',
  styleUrl: './import.css',
})
export class Import {
  private readonly api = inject(KomaApi);
  protected readonly library = inject(LibraryService);
  protected readonly inTauri = this.api.inTauri;

  protected readonly rows = signal<QueueRow[]>([]);
  protected readonly storage = signal<Storage>('linked');
  protected readonly scanning = signal(false);
  protected readonly importing = signal(false);
  private readonly inbox = inject(ImportInbox);
  protected readonly dragOver = this.inbox.dragging;
  /** New files in the watched folders that are not in the queue yet. */
  protected readonly watchNew = computed(() => {
    const queued = new Set(this.rows().map((r) => r.candidate.path));
    return this.inbox.watchFound().filter((path) => !queued.has(path));
  });
  protected readonly error = signal<string | null>(null);
  protected readonly libraryFolder = signal('');

  protected readonly seriesTitles = computed(() => this.library.series().map((s) => s.title));
  protected readonly readyCount = computed(() => this.rows().filter((r) => r.status === 'ready').length);
  protected readonly finishedCount = computed(
    () => this.rows().filter((r) => r.status === 'done' || r.status === 'duplicate').length,
  );
  protected readonly queueSummary = computed(() => {
    const rows = this.rows();
    const size = rows.reduce((sum, r) => sum + r.candidate.size, 0);
    return `${rows.length} ${rows.length === 1 ? 'file' : 'files'} · ${formatSize(size)}`;
  });
  protected readonly librarySize = computed(() => formatSize(this.library.totalSize()));

  constructor() {
    // Files dropped anywhere on the window arrive through the inbox.
    effect(() => {
      if (this.inbox.pending().length > 0) untracked(() => void this.addPaths(this.inbox.take()));
    });
    if (!this.inTauri) return;
    void this.api.getAppInfo().then((info) => this.libraryFolder.set(info.libraryFolder));

    const unlisten = [
      this.api.onImportProgress((progress) => this.onProgress(progress)),
      this.api.onImportFinished(() => {
        this.importing.set(false);
        void this.inbox.recheckWatchFolders();
      }),
    ];
    inject(DestroyRef).onDestroy(() => unlisten.forEach((p) => void p.then((off) => off())));
  }

  protected async chooseFiles(): Promise<void> {
    await this.addPaths(await this.api.pickFiles());
  }

  protected async chooseFolder(): Promise<void> {
    await this.addPaths(await this.api.pickFolders());
  }

  protected async reviewWatched(): Promise<void> {
    await this.addPaths(this.watchNew());
  }

  private async addPaths(paths: string[]): Promise<void> {
    if (paths.length === 0) return;
    this.error.set(null);
    this.scanning.set(true);
    try {
      const candidates = await this.api.scanImport(paths);
      if (candidates.length === 0) {
        this.error.set('No CBZ, ZIP or PDF files were found there.');
        return;
      }
      this.rows.update((rows) => {
        // A file being imported keeps its row; anything else is replaced by the fresh scan.
        const busy = new Set(
          rows.filter((r) => r.status === 'queued' || r.status === 'working').map((r) => r.candidate.path),
        );
        const fresh = candidates.filter((c) => !busy.has(c.path));
        const replaced = new Set(fresh.map((c) => c.path));
        return [...rows.filter((r) => !replaced.has(r.candidate.path)), ...fresh.map(toRow)];
      });
    } catch (err) {
      this.error.set(String(err));
    } finally {
      this.scanning.set(false);
    }
  }

  protected edit(path: string, edit: RowEdit): void {
    this.patch(path, (row) => ({
      ...row,
      ...edit,
      status: edit.series.trim() !== '' && edit.number !== null ? 'ready' : 'needs',
    }));
  }

  protected remove(path: string): void {
    this.rows.update((rows) => rows.filter((r) => r.candidate.path !== path));
  }

  protected clearFinished(): void {
    this.rows.update((rows) => rows.filter((r) => r.status !== 'done' && r.status !== 'duplicate'));
  }

  protected async addAll(): Promise<void> {
    const ready = this.rows().filter((r) => r.status === 'ready');
    if (ready.length === 0 || this.importing()) return;
    const items: ImportItem[] = ready.map((r) => ({
      path: r.candidate.path,
      series: r.series.trim(),
      number: r.number!,
      numberEnd: r.numberEnd,
      unit: r.unit,
      seriesType: r.candidate.seriesType,
      direction: r.candidate.direction,
    }));

    this.error.set(null);
    this.importing.set(true);
    const queued = new Set(items.map((i) => i.path));
    this.rows.update((rows) =>
      rows.map((r) => (queued.has(r.candidate.path) ? { ...r, status: 'queued' as const } : r)),
    );
    try {
      await this.api.runImport(items, this.storage());
    } catch (err) {
      this.importing.set(false);
      this.error.set(String(err));
      this.rows.update((rows) =>
        rows.map((r) => (r.status === 'queued' ? { ...r, status: 'ready' as const } : r)),
      );
    }
  }

  private onProgress(p: ImportProgress): void {
    const copying = this.storage() === 'copied';
    // Share of the bar each stage ends at.
    const afterCopy = copying ? 0.5 : 0;
    const afterCover = afterCopy + 0.15;
    const part = p.total > 0 ? p.done / p.total : 0;

    this.patch(p.path, (row) => {
      const next: QueueRow = { ...row, status: 'working', step: p.step, volumeId: p.volumeId ?? row.volumeId };
      switch (p.step) {
        case 'copy':
          return { ...next, fraction: afterCopy * part };
        case 'index':
          return { ...next, fraction: afterCopy + 0.05 };
        case 'cover':
        case 'saved':
          return { ...next, fraction: afterCover };
        case 'thumbs':
          return { ...next, thumbsDone: p.done, fraction: afterCover + (1 - afterCover) * part };
        case 'done':
          return { ...next, status: 'done', fraction: 1 };
        case 'error':
          return { ...next, status: 'failed', message: p.message };
      }
    });
  }

  private patch(path: string, change: (row: QueueRow) => QueueRow): void {
    this.rows.update((rows) => rows.map((r) => (r.candidate.path === path ? change(r) : r)));
  }

  protected coverUrl(volumeId: string): string {
    return this.api.coverUrl(volumeId);
  }
}
