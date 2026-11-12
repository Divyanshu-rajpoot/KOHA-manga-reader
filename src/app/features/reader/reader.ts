import { Location } from '@angular/common';
import {
  ChangeDetectionStrategy,
  Component,
  DestroyRef,
  computed,
  effect,
  inject,
  signal,
  untracked,
  viewChild,
} from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, Router } from '@angular/router';
import { map } from 'rxjs';
import { formatVolume } from '../../core/format';
import { KomaApi } from '../../core/koma-api';
import { LibraryService } from '../../core/library.service';
import { Direction } from '../../core/models';
import { WindowService } from '../../core/window.service';
import { FitMode, PagedView } from './paged-view';
import { PageSize, buildSpreads } from './spreads';
import { StripView } from './strip-view';
import { ThumbStrip } from './thumb-strip';

export type ReaderMode = 'spread' | 'single' | 'strip';

const MODES: { id: ReaderMode; label: string; key: string }[] = [
  { id: 'spread', label: 'Spread', key: '1' },
  { id: 'single', label: 'Single', key: '2' },
  { id: 'strip', label: 'Long strip', key: '3' },
];
const SAVE_DELAY_MS = 700;
const CACHE_POLL_MS = 600;
const WHEEL_THROTTLE_MS = 140;
/** Fraction of the screen that Space scrolls in long-strip mode. */
const STRIP_STEP = 0.85;
const FULLSCREEN_KEY = 'koma.reader.fullscreen';
/** Distance from the top or bottom edge that reveals the bars in full screen. */
const EDGE_ZONE_PX = 90;

@Component({
  selector: 'koma-reader',
  imports: [PagedView, StripView, ThumbStrip],
  changeDetection: ChangeDetectionStrategy.OnPush,
  templateUrl: './reader.html',
  styleUrl: './reader.css',
  host: {
    '(window:keydown)': 'onKey($event)',
    '(mousemove)': 'onPointerMove($event)',
    '[class.immersive]': 'win.fullscreen()',
    '[class.show-top]': "edge() === 'top' || menuOpen()",
    '[class.show-bottom]': "edge() === 'bottom'",
  },
})
export class Reader {
  private readonly api = inject(KomaApi);
  private readonly library = inject(LibraryService);
  private readonly router = inject(Router);
  private readonly location = inject(Location);
  protected readonly win = inject(WindowService);
  private readonly strip = viewChild(StripView);

  protected readonly modes = MODES;
  protected readonly volumeId = toSignal(
    inject(ActivatedRoute).paramMap.pipe(map((params) => params.get('volumeId') ?? '')),
    { initialValue: '' },
  );
  protected readonly found = computed(() => this.library.findVolume(this.volumeId()));
  protected readonly missing = computed(() => this.library.loaded() && !this.found());

  protected readonly sizes = signal<PageSize[]>([]);
  protected readonly error = signal<string | null>(null);
  protected readonly mode = signal<ReaderMode>('spread');
  protected readonly fit = signal<FitMode>('fit');
  protected readonly direction = signal<Direction>('rtl');
  /** Index of the current page (the first page of the current spread). */
  protected readonly page = signal(0);
  /** The reader went past the last page: show the end card. */
  protected readonly ended = signal(false);
  protected readonly menuOpen = signal(false);
  /** Which screen edge the pointer is near, in full screen. */
  protected readonly edge = signal<'top' | 'bottom' | null>(null);
  protected readonly cached = signal(new Set<number>());

  protected readonly pageCount = computed(() => this.sizes().length);
  protected readonly spreads = computed(() =>
    this.mode() === 'spread' ? buildSpreads(this.sizes()) : this.sizes().map((_, i) => [i]),
  );
  protected readonly spreadIndex = computed(() => {
    const page = this.page();
    const at = this.spreads().findIndex((spread) => spread.includes(page));
    return Math.max(0, at);
  });
  protected readonly currentPages = computed(() => this.spreads()[this.spreadIndex()] ?? []);
  /** The furthest page on screen, 1-based: what is saved as progress. */
  protected readonly pageNumber = computed(() => Math.max(this.page(), ...this.currentPages()) + 1);
  protected readonly title = computed(() => {
    const found = this.found();
    if (!found) return '';
    const { series, volume } = found;
    return `${series.title} · ${formatVolume(volume.unit, volume.number, volume.numberEnd)}`;
  });
  protected readonly pageLabel = computed(() => {
    const pages = this.currentPages();
    const shown =
      this.mode() === 'spread' && pages.length === 2 ? `${pages[0] + 1}–${pages[1] + 1}` : `${this.page() + 1}`;
    const direction = this.mode() === 'strip' ? 'long strip' : this.direction() === 'rtl' ? 'right to left' : 'left to right';
    return `Page ${shown} / ${this.pageCount()} · ${direction}`;
  });
  protected readonly nextVolume = computed(() => {
    const found = this.found();
    return found ? (found.series.volumes[found.index + 1] ?? null) : null;
  });
  protected readonly nextVolumeLabel = computed(() => {
    const next = this.nextVolume();
    return next ? formatVolume(next.unit, next.number, next.numberEnd) : '';
  });

  private loadedFor = '';
  private saveTimer = 0;
  private cacheTimer = 0;
  private lastSaved = 0;
  private lastWheel = 0;

  constructor() {
    // Set up once per volume, as soon as the library knows it.
    effect(() => {
      const id = this.volumeId();
      const found = this.found();
      if (!found || this.loadedFor === id) return;
      this.loadedFor = id;
      untracked(() => void this.open(id));
    });

    // Save progress shortly after the page settles, and refresh the cached-page markers.
    effect(() => {
      const number = this.pageNumber();
      if (this.pageCount() === 0) return;
      clearTimeout(this.saveTimer);
      this.saveTimer = setTimeout(() => this.save(number), SAVE_DELAY_MS);
      clearTimeout(this.cacheTimer);
      this.cacheTimer = setTimeout(() => void this.refreshCached(), CACHE_POLL_MS);
    });

    // Reopen in full screen if that is how the reader was last used.
    try {
      if (localStorage.getItem(FULLSCREEN_KEY) === '1') void this.win.setFullscreen(true);
    } catch {
      // Start windowed.
    }

    inject(DestroyRef).onDestroy(() => {
      clearTimeout(this.saveTimer);
      clearTimeout(this.cacheTimer);
      if (this.pageCount() > 0) this.save(this.pageNumber());
      void this.win.setFullscreen(false);
    });
  }

  private async open(id: string): Promise<void> {
    const found = this.library.findVolume(id);
    if (!found) return;
    const { series, volume } = found;
    this.sizes.set([]);
    this.error.set(null);
    this.ended.set(false);
    this.cached.set(new Set());
    this.direction.set(series.direction);
    this.mode.set(this.storedMode(series.id) ?? (series.type === 'manhwa' ? 'strip' : 'spread'));
    const start = Math.min(volume.pages, Math.max(1, volume.progress?.page ?? 1)) - 1;
    this.lastSaved = volume.progress?.page ?? 0;
    try {
      const sizes = await this.api.getPageSizes(id);
      if (this.volumeId() !== id) return;
      this.page.set(Math.min(start, Math.max(0, sizes.length - 1)));
      this.sizes.set(sizes);
    } catch (err) {
      this.error.set(String(err));
    }
  }

  private save(pageNumber: number): void {
    const id = this.loadedFor;
    if (!id || pageNumber === this.lastSaved) return;
    this.lastSaved = pageNumber;
    void this.api.setProgress(id, pageNumber).catch(() => (this.lastSaved = 0));
  }

  private async refreshCached(): Promise<void> {
    const id = this.volumeId();
    const pages = await this.api.getCachedPages(id).catch(() => []);
    if (this.volumeId() === id) this.cached.set(new Set(pages));
  }

  // --- navigation ---

  protected next(): void {
    if (this.ended()) {
      this.openNextVolume();
    } else if (this.mode() === 'strip') {
      const strip = this.strip();
      if (strip?.atEnd()) this.ended.set(true);
      else strip?.scrollByScreens(STRIP_STEP);
    } else {
      this.step(1);
    }
  }

  protected previous(): void {
    if (this.ended()) this.ended.set(false);
    else if (this.mode() === 'strip') this.strip()?.scrollByScreens(-STRIP_STEP);
    else this.step(-1);
  }

  private step(delta: number): void {
    const target = this.spreadIndex() + delta;
    if (target < 0) return;
    if (target >= this.spreads().length) {
      this.ended.set(true);
      return;
    }
    this.page.set(this.spreads()[target][0]);
  }

  protected goTo(index: number): void {
    const target = Math.min(this.pageCount() - 1, Math.max(0, index));
    this.ended.set(false);
    this.page.set(target);
  }

  protected openNextVolume(): void {
    const next = this.nextVolume();
    if (next) void this.router.navigate(['/read', next.id], { replaceUrl: true });
  }

  protected back(): void {
    if (window.history.length > 1) this.location.back();
    else void this.router.navigateByUrl('/');
  }

  // --- settings ---

  protected setMode(mode: ReaderMode): void {
    this.mode.set(mode);
    this.ended.set(false);
    const seriesId = this.found()?.series.id;
    try {
      if (seriesId) localStorage.setItem(`koma.mode.${seriesId}`, mode);
    } catch {
      // Not remembered this time.
    }
  }

  private storedMode(seriesId: string): ReaderMode | null {
    try {
      const stored = localStorage.getItem(`koma.mode.${seriesId}`);
      return MODES.some((m) => m.id === stored) ? (stored as ReaderMode) : null;
    } catch {
      return null;
    }
  }

  protected toggleDirection(): void {
    this.direction.update((d) => (d === 'rtl' ? 'ltr' : 'rtl'));
    this.menuOpen.set(false);
  }

  protected toggleFullscreen(): void {
    const on = !this.win.fullscreen();
    void this.win.setFullscreen(on);
    this.menuOpen.set(false);
    try {
      localStorage.setItem(FULLSCREEN_KEY, on ? '1' : '0');
    } catch {
      // Not remembered this time.
    }
  }

  /** In full screen the bars are hidden until the pointer nears the top or bottom edge. */
  protected onPointerMove(event: MouseEvent): void {
    if (!this.win.fullscreen()) return;
    const edge = event.clientY < EDGE_ZONE_PX ? 'top' : event.clientY > window.innerHeight - EDGE_ZONE_PX ? 'bottom' : null;
    if (edge !== this.edge()) this.edge.set(edge);
  }

  // --- input ---

  /** Clicking the left or right third of the stage turns the page. */
  protected onStageClick(event: MouseEvent): void {
    this.menuOpen.set(false);
    if (this.mode() === 'strip' || this.fit() !== 'fit' || this.ended()) return;
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    const x = (event.clientX - rect.left) / rect.width;
    if (x < 1 / 3) this.towards('left');
    else if (x > 2 / 3) this.towards('right');
  }

  protected onWheel(event: WheelEvent): void {
    // When the page is larger than the view, the wheel scrolls it instead.
    if (this.mode() === 'strip' || this.fit() !== 'fit' || event.ctrlKey) return;
    const now = performance.now();
    if (now - this.lastWheel < WHEEL_THROTTLE_MS || event.deltaY === 0) return;
    this.lastWheel = now;
    if (event.deltaY > 0) this.next();
    else this.previous();
  }

  /** In right-to-left reading, moving left goes forward. */
  private towards(side: 'left' | 'right'): void {
    const forward = (side === 'left') === (this.direction() === 'rtl');
    if (forward) this.next();
    else this.previous();
  }

  protected onKey(event: KeyboardEvent): void {
    if (event.ctrlKey || event.altKey || event.metaKey) return;
    // Space and Enter on a focused button should press that button, not turn the page.
    const onButton = event.target instanceof HTMLButtonElement;
    if (onButton && (event.key === ' ' || event.key === 'Enter')) return;
    const strip = this.mode() === 'strip';
    switch (event.key) {
      case 'ArrowLeft':
        if (!strip) this.towards('left');
        break;
      case 'ArrowRight':
        if (!strip) this.towards('right');
        break;
      case 'ArrowDown':
        if (strip) this.strip()?.scrollByScreens(0.25);
        else this.next();
        break;
      case 'ArrowUp':
        if (strip) this.strip()?.scrollByScreens(-0.25);
        else this.previous();
        break;
      case ' ':
        if (event.shiftKey) this.previous();
        else this.next();
        break;
      case 'PageDown':
      case 'Enter':
        this.next();
        break;
      case 'PageUp':
      case 'Backspace':
        this.previous();
        break;
      case 'Home':
        this.goTo(0);
        break;
      case 'End':
        this.goTo(this.pageCount() - 1);
        break;
      case '1':
      case '2':
      case '3':
        this.setMode(MODES[Number(event.key) - 1].id);
        break;
      case 'f':
      case 'F':
        this.fit.update((fit) => (fit === 'fit' ? 'width' : fit === 'width' ? 'actual' : 'fit'));
        break;
      case 'F11':
        this.toggleFullscreen();
        break;
      case 'Escape':
        if (this.menuOpen()) this.menuOpen.set(false);
        else if (this.win.fullscreen()) this.toggleFullscreen();
        else this.back();
        break;
      default:
        return;
    }
    event.preventDefault();
  }
}
