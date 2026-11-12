import { ChangeDetectionStrategy, Component, ElementRef, computed, effect, inject, input } from '@angular/core';
import { KomaApi } from '../../core/koma-api';
import { Direction } from '../../core/models';
import { hostSize } from './element-size';
import { PageSize, aspectOf } from './spreads';

/** `fit`: whole page on screen. `width`: page as wide as the screen, scroll down. `actual`: one image pixel per screen pixel. */
export type FitMode = 'fit' | 'width' | 'actual';

// Frame chrome in CSS pixels; keep in sync with the styles below.
const BORDER = 4;
const PADDING = 10;
const GAP = 3;
const SHADOW = 10;
const MARGIN = 6;

/**
 * Spread and single modes: one or two pages. Pages are always the original
 * image files, never resized or re-encoded, and there is no page-turn animation.
 */
@Component({
  selector: 'koma-paged-view',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="frame" [class.rtl]="direction() === 'rtl'" [class.bare]="bare()">
      @for (page of layout(); track page.index) {
        <img
          [src]="page.url"
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
      display: flex;
      width: 100%;
      height: 100%;
      overflow: auto;
    }
    .frame {
      display: flex;
      gap: 3px;
      margin: auto;
      padding: 10px;
      border: 4px solid var(--ink);
      background: var(--reader-bg);
      box-shadow: 10px 10px 0 var(--acc);
      /* Room for the offset shadow, so it is never clipped. */
      translate: -5px -5px;
    }
    /* Full screen: no frame, the pages use every pixel. */
    .frame.bare {
      gap: 0;
      padding: 0;
      border: 0;
      box-shadow: none;
      translate: none;
    }
    .frame.rtl {
      flex-direction: row-reverse;
    }
    img {
      display: block;
      flex: none;
      background: color-mix(in srgb, var(--text) 8%, var(--reader-bg));
    }
  `,
})
export class PagedView {
  private readonly api = inject(KomaApi);
  private readonly box = hostSize();

  readonly volumeId = input.required<string>();
  /** Page indexes in reading order: one page, or the two pages of a spread. */
  readonly pages = input.required<number[]>();
  readonly sizes = input.required<PageSize[]>();
  readonly direction = input<Direction>('rtl');
  readonly fit = input<FitMode>('fit');
  /** Full screen: drop the frame and fill the whole view. */
  readonly bare = input(false);

  constructor() {
    // A new page starts at its top.
    const element = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement;
    effect(() => {
      this.pages();
      element.scrollTop = 0;
    });
  }

  protected readonly layout = computed(() => {
    const pages = this.pages();
    const sizes = this.sizes();
    const { width, height } = this.box();
    const dpr = window.devicePixelRatio || 1;
    const bare = this.bare();
    const chrome = bare ? 0 : 2 * (BORDER + PADDING) + SHADOW + 2 * MARGIN;
    const gap = bare ? 0 : GAP;
    const availableW = Math.max(120, width - chrome - gap * (pages.length - 1));
    const availableH = Math.max(120, height - chrome);

    const aspects = pages.map((index) => aspectOf(sizes[index]));
    const totalAspect = aspects.reduce((sum, a) => sum + a, 0);
    const widthHeight = availableW / totalAspect;
    const fitHeight = this.fit() === 'width' ? widthHeight : Math.min(availableH, widthHeight);

    return pages.map((index, i) => {
      const natural = sizes[index];
      const actual = this.fit() === 'actual' && natural && natural[0] > 0;
      return {
        index,
        // 100%: one image pixel per screen pixel. Fit: scaled by the display only.
        width: Math.round(actual ? natural[0] / dpr : fitHeight * aspects[i]),
        height: Math.round(actual ? natural[1] / dpr : fitHeight),
        url: this.api.pageUrl(this.volumeId(), index),
      };
    });
  });
}
