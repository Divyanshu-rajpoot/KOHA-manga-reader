import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  input,
  model,
  signal,
} from '@angular/core';
import { ThemeService } from '../../core/theme.service';
import { Cover } from '../../shared/cover/cover';

export interface CarouselItem {
  seriesId: string;
  title: string;
  volumeId: string;
  /** Volume number shown on the cover, e.g. "07". */
  badge: string;
}

const AUTO_ADVANCE_MS = 3600;
/** Cards further than this from the front are hidden. */
const VISIBLE_REACH = 2;

/** The rotating 3D cover carousel. See docs/DESIGN.md §6 for its geometry. */
@Component({
  selector: 'koma-carousel',
  imports: [Cover],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '(mouseenter)': 'hovered.set(true)',
    '(mouseleave)': 'hovered.set(false)',
    '(focusin)': 'hovered.set(true)',
    '(focusout)': 'hovered.set(false)',
  },
  template: `
    <div class="flow">
      @for (card of cards(); track card.item.seriesId) {
        <button
          type="button"
          class="card"
          [class.front]="card.offset === 0"
          [style.transform]="card.transform"
          [style.z-index]="card.z"
          [style.opacity]="card.visible ? 1 : 0"
          [style.pointer-events]="card.visible ? 'auto' : 'none'"
          [attr.tabindex]="card.visible ? 0 : -1"
          [attr.aria-label]="card.item.title"
          [attr.aria-current]="card.offset === 0 ? 'true' : null"
          (click)="index.set(card.i)"
        >
          <koma-cover [volumeId]="card.item.volumeId" [badge]="card.item.badge" [lazy]="false" />
        </button>
      }
    </div>

    @if (items().length > 1) {
      <div class="controls">
        <button type="button" aria-label="Previous series" (click)="step(-1)">&#9664;</button>
        <span class="mono" aria-live="polite">{{ index() + 1 }} / {{ items().length }}</span>
        <button type="button" aria-label="Next series" (click)="step(1)">&#9654;</button>
      </div>
    }
  `,
  styles: `
    :host {
      position: relative;
      display: block;
      height: 400px;
    }
    .flow {
      position: relative;
      height: 100%;
      display: flex;
      align-items: center;
      justify-content: center;
      perspective: 1200px;
      padding-bottom: 46px;
    }
    .card {
      position: absolute;
      width: 184px;
      padding: 0;
      border: 0;
      background: transparent;
      filter: grayscale(0.7) opacity(0.75);
      transition:
        transform 700ms var(--ease),
        opacity 700ms var(--ease),
        filter 700ms var(--ease);
    }
    .card.front {
      filter: none;
    }
    .card koma-cover {
      transition: box-shadow 700ms var(--ease);
    }
    .card.front koma-cover {
      box-shadow: 10px 10px 0 var(--acc);
    }
    .controls {
      position: absolute;
      left: 0;
      right: 0;
      bottom: 14px;
      z-index: 20;
      display: flex;
      align-items: center;
      justify-content: center;
      gap: 12px;
    }
    .controls button {
      width: 44px;
      height: 44px;
      border: var(--border-control) solid var(--ink);
      background: var(--on-ink);
      color: var(--ink);
      font-size: 13px;
    }
    .controls button:hover {
      background: var(--ink);
      color: var(--on-ink);
    }
    .controls span {
      padding: 10px 14px;
      border: var(--border-control) solid var(--ink);
      background: var(--on-ink);
      color: var(--ink);
    }
  `,
})
export class Carousel {
  private readonly theme = inject(ThemeService);

  readonly items = input.required<CarouselItem[]>();
  /** Index of the front card. */
  readonly index = model(0);

  protected readonly hovered = signal(false);

  protected readonly cards = computed(() => {
    const items = this.items();
    const count = items.length;
    const current = this.index();
    return items.map((item, i) => {
      // Shortest way round the ring from the front card.
      let offset = i - current;
      if (offset > count / 2) offset -= count;
      if (offset < -count / 2) offset += count;
      const distance = Math.abs(offset);
      return {
        i,
        item,
        offset,
        visible: distance <= VISIBLE_REACH,
        z: 10 - distance,
        transform: `translateX(${offset * 175}px) translateZ(${-distance * 170}px) rotateY(${-offset * 30}deg)`,
      };
    });
  });

  constructor() {
    // Auto-advance, paused while hovered or focused, and off under "reduce motion".
    let timer = 0;
    effect(() => {
      clearInterval(timer);
      const run = this.items().length > 1 && !this.hovered() && !this.theme.reduceMotion();
      if (run) timer = setInterval(() => this.step(1), AUTO_ADVANCE_MS);
    });
    inject(DestroyRef).onDestroy(() => clearInterval(timer));
  }

  protected step(delta: number): void {
    const count = this.items().length;
    if (count > 0) this.index.update((i) => (i + delta + count) % count);
  }
}
