import { ChangeDetectionStrategy, Component, computed, inject, input, output, signal } from '@angular/core';
import { formatSize, formatVolume, parseVolumeNumber } from '../../core/format';
import { KomaApi } from '../../core/koma-api';
import { Candidate, ImportStep, Unit } from '../../core/models';

export type RowStatus = 'ready' | 'needs' | 'duplicate' | 'error' | 'queued' | 'working' | 'done' | 'failed';

/** One file in the import queue: the scan result plus the user's edits and progress. */
export interface QueueRow {
  candidate: Candidate;
  series: string;
  number: number | null;
  numberEnd: number | null;
  unit: Unit;
  status: RowStatus;
  step: ImportStep | null;
  /** 0 to 1. */
  fraction: number;
  thumbsDone: number;
  message: string | null;
  volumeId: string | null;
}

export interface RowEdit {
  series: string;
  number: number | null;
  numberEnd: number | null;
  unit: Unit;
}

@Component({
  selector: 'koma-queue-row',
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './queue-row.html',
  styleUrl: './queue-row.css',
  host: { '[class.flagged]': 'flagged()' },
})
export class QueueRowView {
  private readonly api = inject(KomaApi);

  readonly row = input.required<QueueRow>();
  /** Titles of series already in the library, offered as suggestions. */
  readonly seriesTitles = input<string[]>([]);
  readonly edited = output<RowEdit>();
  readonly removed = output<void>();

  protected readonly editing = signal(false);
  protected readonly size = computed(() => formatSize(this.row().candidate.size));
  protected readonly listId = computed(() => 'series-' + this.row().candidate.path.replace(/\W/g, ''));
  protected readonly flagged = computed(() => ['needs', 'error', 'failed'].includes(this.row().status));
  protected readonly locked = computed(() => ['queued', 'working', 'done'].includes(this.row().status));
  protected readonly canEdit = computed(() => ['ready', 'needs'].includes(this.row().status));
  protected readonly showEditor = computed(
    () => this.canEdit() && (this.editing() || this.row().status === 'needs'),
  );
  protected readonly coverUrl = computed(() => {
    const id = this.row().volumeId;
    return id ? this.api.coverUrl(id) : null;
  });
  protected readonly numberText = computed(() => {
    const row = this.row();
    if (row.number === null) return '';
    return row.numberEnd === null ? String(row.number) : `${row.number}-${row.numberEnd}`;
  });
  protected readonly matchLine = computed(() => {
    const row = this.row();
    switch (row.status) {
      case 'error':
      case 'failed':
        return row.message ?? 'Could not read this file';
      case 'duplicate':
        return 'Already in your library';
      case 'needs':
        return 'No match: choose a series and number';
      default:
        return `${row.series} → ${formatVolume(row.unit, row.number ?? 0, row.numberEnd)}`;
    }
  });
  protected readonly indexTime = computed(() => {
    const ms = this.row().candidate.indexMs;
    return ms >= 1000 ? `${(ms / 1000).toFixed(2)} s` : `${Math.max(1, Math.round(ms))} ms`;
  });
  protected readonly coverDone = computed(() => {
    const step = this.row().step;
    return step === 'cover' || step === 'saved' || step === 'thumbs' || step === 'done';
  });

  protected setSeries(series: string): void {
    this.edited.emit({ ...this.current(), series });
  }

  protected setNumber(text: string): void {
    const parsed = parseVolumeNumber(text);
    this.edited.emit({
      ...this.current(),
      number: parsed?.number ?? null,
      numberEnd: parsed?.numberEnd ?? null,
    });
  }

  protected setUnit(unit: string): void {
    this.edited.emit({ ...this.current(), unit: unit === 'chapter' ? 'chapter' : 'volume' });
  }

  private current(): RowEdit {
    const { series, number, numberEnd, unit } = this.row();
    return { series, number, numberEnd, unit };
  }
}
