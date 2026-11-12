import {
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  computed,
  effect,
  inject,
  input,
  output,
  signal,
} from '@angular/core';
import { KomaApi } from '../../core/koma-api';
import { hostSize } from './element-size';
import { FitMode } from './paged-view';
import { PageSize, aspectOf } from './spreads';

/** Widest a strip page is shown in Fit mode, in CSS pixels. */
const MAX_FIT_WIDTH = 860;
const MAX_FIT_WIDTH_FULLSCREEN = 1100;
const SIDE_MARGIN = 24;
/** Pages within this many screens above and below the view are kept rendered. */
const BUFFER_SCREENS = 1.5;
/** The "current page" is the one crossing this fraction of the view height. */
const READING_LINE = 0.35;

interface StripLayout {
  /** Top of each page in CSS pixels. */
  tops: number[];
  widths: number[];
  heights: number[];
  total: number;
}

/**
 * Long-strip mode (manhwa): every page stacked in one scroll. Page sizes are
 * known up front, so the full height is laid out immediately and only the
 * pages near the view are actually rendered.
 */
@Component({
  selector: 'koma-strip-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(scroll)': 'onScroll()', tabindex: '-1' },
  template: `
    <div class="rail" [style.height.px]="layout().total">
      @for (page of visible(); track page.index) {
        <img
          [src]="page.url"
          [style.top.px]="page.top"
          [style.left.px]="page.left"
          [style.width.px]="page.width"
          [style.height.px]="page.height"
          [alt]="'Page ' + (page.index + 1)"
          draggable="false"
        />
      }
    </div>
  `,
  styles: `
    :host {
      display: block;
      width: 100%;
      height: 100%;
      overflow-y: auto;
      overflow-x: hidden;
      outline: none;
    }
    .rail {
      position: relative;
    }
    img {
      position: absolute;
      display: block;
      background: color-mix(in srgb, var(--text) 8%, var(--reader-bg));
    }
  `,
})
export class StripView {
  private readonly api = inject(KomaApi);
  private readonly element = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
  private readonly box = hostSize();
  private readonly scrollTop = signal(0);
  private frame = 0;
  private lastPage = -1;
  private lastLayout: StripLayout | null = null;

  readonly volumeId = input.required<string>();
  readonly sizes = input.required<PageSize[]>();
  readonly fit = input<FitMode>('fit');
  /** Full screen: wider pages, no side margins. */
  readonly bare = input(false);
  /** The current page index. Changing it from outside scrolls to that page. */
  readonly page = input(0);
  readonly pageChange = output<number>();

  protected readonly layout = computed<StripLayout>(() => {
    const dpr = window.devicePixelRatio || 1;
    const bare = this.bare();
    const room = Math.max(160, this.box().width - (bare ? 0 : 2 * SIDE_MARGIN));
    const fitWidth =
      this.fit() === 'width' ? room : Math.min(room, bare ? MAX_FIT_WIDTH_FULLSCREEN : MAX_FIT_WIDTH);
    const tops: number[] = [];
    const widths: number[] = [];
    const heights: number[] = [];
    let total = 0;
    for (const size of this.sizes()) {
      const width =
        this.fit() === 'actual' && size[0] > 0 ? Math.min(room, size[0] / dpr) : fitWidth;
      const height = Math.round(width / aspectOf(size));
      tops.push(total);
      widths.push(Math.round(width));
      heights.push(height);
      total += height;
    }
    return { tops, widths, heights, total };
  });

  protected readonly visible = computed(() => {
    const { tops, widths, heights } = this.layout();
    const { width: boxWidth, height: boxHeight } = this.box();
    const from = this.scrollTop() - boxHeight * BUFFER_SCREENS;
    const to = this.scrollTop() + boxHeight * (1 + BUFFER_SCREENS);
    const items = [];
    for (let index = this.firstPageAtOrBelow(from); index < tops.length && tops[index] < to; index++) {
      items.push({
        index,
        top: tops[index],
        left: Math.round((boxWidth - widths[index]) / 2),
        width: widths[index],
        height: heights[index],
        // Always the original image file: never resized or re-encoded.
        url: this.api.pageUrl(this.volumeId(), index),
      });
    }
    return items;
  });

  constructor() {
    // Scroll to a page chosen from outside, and keep the place when the layout changes.
    effect(() => {
      const layout = this.layout();
      const page = this.page();
      if (page === this.lastPage && layout === this.lastLayout) return;
      this.lastPage = page;
      this.lastLayout = layout;
      // Wait a frame so the rail has its new height before scrolling.
      requestAnimationFrame(() => {
        this.element.scrollTop = layout.tops[page] ?? 0;
        this.scrollTop.set(this.element.scrollTop);
      });
    });
  }

  /** Scrolls by a fraction of the view height (negative scrolls up). */
  scrollByScreens(screens: number): void {
    this.element.scrollBy({ top: this.box().height * screens, behavior: 'instant' });
  }

  /** True when the view is scrolled to the very end. */
  atEnd(): boolean {
    return this.element.scrollTop + this.element.clientHeight >= this.element.scrollHeight - 2;
  }

  protected onScroll(): void {
    if (this.frame) return;
    this.frame = requestAnimationFrame(() => {
      this.frame = 0;
      const top = this.element.scrollTop;
      this.scrollTop.set(top);
      const { tops } = this.layout();
      if (tops.length === 0) return;
      const line = top + this.box().height * READING_LINE;
      const current = Math.min(tops.length - 1, Math.max(0, this.firstPageAtOrBelow(line)));
      if (current !== this.lastPage) {
        this.lastPage = current;
        this.pageChange.emit(current);
      }
    });
  }

  /** Index of the page that contains `y`, by binary search. */
  private firstPageAtOrBelow(y: number): number {
    const { tops } = this.layout();
    let low = 0;
    let high = tops.length - 1;
    while (low < high) {
      const mid = (low + high + 1) >> 1;
      if (tops[mid] <= y) low = mid;
      else high = mid - 1;
    }
    return low;
  }
}
