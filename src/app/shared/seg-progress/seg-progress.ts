import { ChangeDetectionStrategy, Component, computed, input } from '@angular/core';

const BLOCKS = 20;

/** 20-block progress bar: filled blocks are ink, the last filled block is the accent. */
@Component({
  selector: 'koma-seg-progress',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    role: 'progressbar',
    'aria-valuemin': '0',
    'aria-valuemax': '100',
    '[attr.aria-valuenow]': 'percent()',
  },
  template: `
    @for (state of blocks(); track $index) {
      <i [class.on]="state !== 'off'" [class.last]="state === 'last'"></i>
    }
  `,
  styles: `
    :host {
      display: grid;
      grid-template-columns: repeat(20, 1fr);
      gap: 3px;
      max-width: 440px;
    }
    i {
      height: 12px;
      background: color-mix(in srgb, var(--ink) 14%, transparent);
    }
    i.on {
      background: var(--ink);
    }
    i.last {
      background: var(--acc);
    }
  `,
})
export class SegProgress {
  /** Progress from 0 to 1. */
  readonly value = input(0);

  protected readonly percent = computed(() => Math.round(Math.min(1, Math.max(0, this.value())) * 100));
  protected readonly blocks = computed(() => {
    const filled = Math.round((this.percent() / 100) * BLOCKS);
    return Array.from({ length: BLOCKS }, (_, i) =>
      i >= filled ? 'off' : i === filled - 1 ? 'last' : 'on',
    );
  });
}
