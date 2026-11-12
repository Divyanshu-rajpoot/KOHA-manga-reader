import { ChangeDetectionStrategy, Component, computed, inject, input, output } from '@angular/core';
import { KomaApi } from '../../core/koma-api';
import { Direction } from '../../core/models';

/** Thumbnails shown on each side of the current page. */
const SIDE_THUMBS = 7;

/** Thumbnail strip plus the page scrubber. Both run right-to-left for manga. */
@Component({
  selector: 'koma-thumb-strip',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="thumbs" [class.rtl]="rtl()">
      @for (index of window(); track index) {
        <button
          type="button"
          [class.cur]="index === page()"
          [class.cached]="cached().has(index)"
          [attr.aria-label]="'Go to page ' + (index + 1)"
          [attr.aria-current]="index === page() ? 'page' : null"
          (click)="seek.emit(index)"
        >
          <img [src]="thumbUrl(index)" alt="" loading="lazy" draggable="false" />
        </button>
      }
    </div>

    <div class="scrub mono">
      <span>{{ rtl() ? pageCount() : 1 }}</span>
      <div
        class="track"
        role="slider"
        tabindex="0"
        aria-label="Page"
        aria-valuemin="1"
        [attr.aria-valuemax]="pageCount()"
        [attr.aria-valuenow]="page() + 1"
        (pointerdown)="startDrag($event)"
        (pointermove)="drag($event)"
      >
        <i [class.rtl]="rtl()" [style.width.%]="percent()"></i>
      </div>
      <span>{{ page() + 1 }} / {{ pageCount() }}{{ cachedLabel() }}</span>
    </div>
  `,
  styles: `
    :host {
      display: block;
      padding: 8px 22px 14px;
    }
    .thumbs {
      display: flex;
      justify-content: center;
      align-items: flex-end;
      gap: 6px;
      height: 50px;
      margin-bottom: 10px;
    }
    .thumbs.rtl {
      flex-direction: row-reverse;
    }
    button {
      width: 30px;
      height: 42px;
      padding: 0;
      border: 0;
      background: color-mix(in srgb, var(--text) 18%, transparent);
      opacity: 0.4;
      overflow: hidden;
    }
    /* Pages already in memory are brighter. */
    button.cached {
      opacity: 0.75;
    }
    button:hover {
      opacity: 1;
    }
    button.cur {
      opacity: 1;
      outline: 2px solid var(--acc);
      transform: translateY(-4px);
    }
    img {
      display: block;
      width: 100%;
      height: 100%;
      object-fit: cover;
    }
    .scrub {
      display: flex;
      align-items: center;
      gap: 12px;
      opacity: 0.85;
    }
    .track {
      position: relative;
      flex: 1;
      height: 16px;
      cursor: pointer;
      touch-action: none;
    }
    .track::before,
    .track i {
      content: '';
      position: absolute;
      top: 6px;
      height: 4px;
    }
    .track::before {
      left: 0;
      right: 0;
      background: color-mix(in srgb, var(--text) 20%, transparent);
    }
    .track i {
      left: 0;
      background: var(--acc);
    }
    .track i.rtl {
      left: auto;
      right: 0;
    }
  `,
})
export class ThumbStrip {
  private readonly api = inject(KomaApi);

  readonly volumeId = input.required<string>();
  readonly pageCount = input.required<number>();
  /** Current page index. */
  readonly page = input.required<number>();
  readonly direction = input<Direction>('rtl');
  /** Page indexes ready in the memory cache. */
  readonly cached = input<Set<number>>(new Set());
  readonly seek = output<number>();

  protected readonly rtl = computed(() => this.direction() === 'rtl');
  protected readonly percent = computed(() =>
    this.pageCount() > 1 ? (this.page() / (this.pageCount() - 1)) * 100 : 100,
  );
  protected readonly window = computed(() => {
    const count = this.pageCount();
    const size = Math.min(count, SIDE_THUMBS * 2 + 1);
    const start = Math.max(0, Math.min(this.page() - SIDE_THUMBS, count - size));
    return Array.from({ length: size }, (_, i) => start + i);
  });
  /** " · pages 136–150 cached": the unbroken cached run around the current page. */
  protected readonly cachedLabel = computed(() => {
    const cached = this.cached();
    const page = this.page();
    if (!cached.has(page)) return '';
    let from = page;
    let to = page;
    while (cached.has(from - 1)) from--;
    while (cached.has(to + 1)) to++;
    return from === to ? '' : ` · pages ${from + 1}–${to + 1} cached`;
  });

  protected thumbUrl(index: number): string {
    return this.api.thumbUrl(this.volumeId(), index);
  }

  protected startDrag(event: PointerEvent): void {
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
    this.seekTo(event);
  }

  protected drag(event: PointerEvent): void {
    if (event.buttons === 1) this.seekTo(event);
  }

  private seekTo(event: PointerEvent): void {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    const fraction = this.rtl() ? 1 - x : x;
    const index = Math.round(fraction * (this.pageCount() - 1));
    if (index !== this.page()) this.seek.emit(index);
  }
}
