import { ChangeDetectionStrategy, Component, ElementRef, computed, inject, viewChild } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { ActivatedRoute, NavigationEnd, Router, RouterLink, RouterLinkActive } from '@angular/router';
import { filter, map } from 'rxjs';
import { ImportInbox } from '../../core/inbox.service';
import { Settings } from '../../core/models';
import { SettingsService } from '../../core/settings.service';

const THEME_ORDER: Settings['theme'][] = ['system', 'light', 'dark'];

@Component({
  selector: 'koma-nav',
  imports: [RouterLink, RouterLinkActive],
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: { '(window:keydown)': 'onKey($event)' },
  template: `
    <a class="logo" routerLink="/" aria-label="KOMA home">
      <span class="mark" aria-hidden="true">コ</span>KOMA
    </a>

    <nav class="links" aria-label="Main">
      <a routerLink="/" routerLinkActive="cur" [routerLinkActiveOptions]="{ exact: true }">Home</a>
      <a routerLink="/library" routerLinkActive="cur" [class.cur]="onSeries()">Library</a>
      <a routerLink="/import" routerLinkActive="cur">
        Import
        @if (inbox.watchFound().length > 0) {
          <span class="count mono" [attr.aria-label]="inbox.watchFound().length + ' new files found'">
            {{ inbox.watchFound().length }}
          </span>
        }
      </a>
      <a routerLink="/settings" routerLinkActive="cur">Settings</a>
    </nav>

    <label class="search">
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <circle cx="7" cy="7" r="4.5" />
        <path d="M10.5 10.5L14 14" />
      </svg>
      <input
        #search
        type="search"
        placeholder="Search series, volumes, files"
        aria-label="Search the library"
        [value]="query()"
        (input)="onSearch(search.value)"
        (keydown.escape)="search.value = ''; onSearch(''); search.blur()"
      />
      <span class="kbd">Ctrl K</span>
    </label>

    <button
      type="button"
      class="theme mono"
      (click)="cycleTheme()"
      [attr.aria-label]="'Theme: ' + themeLabel() + '. Click to change.'"
    >
      <span class="swatch" aria-hidden="true"></span>{{ themeLabel() }}
    </button>

    <a class="btn acc" routerLink="/import">
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 2v12M2 8h12" /></svg>
      Add files
    </a>
  `,
  styles: `
    :host {
      display: flex;
      align-items: center;
      gap: 24px;
      padding: 14px var(--gutter);
      flex: none;
    }
    a {
      color: inherit;
      text-decoration: none;
    }
    .logo {
      display: flex;
      align-items: center;
      gap: 8px;
      font-family: var(--font-display);
      font-size: 24px;
      letter-spacing: 0.5px;
    }
    .mark {
      width: 28px;
      height: 28px;
      display: grid;
      place-items: center;
      background: var(--acc);
      color: var(--on-acc);
      font-family: var(--font-jp);
      font-weight: 700;
      font-size: 14px;
    }
    .links {
      display: flex;
      gap: 4px;
      margin-left: 12px;
    }
    .links a {
      display: flex;
      align-items: center;
      min-height: 40px;
      padding: 0 14px;
      color: var(--muted);
      transition:
        background var(--t-ui) var(--ease),
        color var(--t-ui) var(--ease);
    }
    .links a:hover {
      color: var(--text);
    }
    .links a.cur {
      background: var(--ink);
      color: var(--on-ink);
      font-weight: 700;
    }
    .search {
      margin-left: auto;
      width: 300px;
      height: 40px;
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 0 12px;
      border: var(--border-control) solid var(--ink);
      background: transparent;
      color: var(--muted);
      font-size: 13px;
      cursor: text;
    }
    .search:focus-within {
      box-shadow: 5px 5px 0 var(--acc);
    }
    .search input {
      flex: 1;
      min-width: 0;
      border: 0;
      outline: 0;
      background: transparent;
      color: var(--text);
      font: inherit;
    }
    .search input::placeholder {
      color: var(--muted);
    }
    .search input::-webkit-search-cancel-button {
      display: none;
    }
    .count {
      margin-left: 8px;
      padding: 0 5px;
      background: var(--acc);
      color: var(--on-acc);
      font-size: 11px;
    }
    .kbd {
      margin-left: auto;
      padding: 1px 6px;
      border: 1px solid var(--line);
      font-size: 11px;
    }
    .theme {
      height: 40px;
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 0 12px;
      border: var(--border-control) solid var(--ink);
      background: transparent;
      font-size: 11px;
      letter-spacing: 1px;
      text-transform: uppercase;
    }
    .theme:hover {
      background: var(--ink);
      color: var(--on-ink);
    }
    .swatch {
      width: 12px;
      height: 12px;
      border: 2px solid currentColor;
      background: linear-gradient(135deg, currentColor 50%, transparent 50%);
    }
    svg {
      width: 14px;
      height: 14px;
      fill: none;
      stroke: currentColor;
      stroke-width: 1.8;
    }
    .btn {
      margin-left: -8px;
    }
  `,
})
export class Nav {
  private readonly settings = inject(SettingsService);
  private readonly router = inject(Router);
  protected readonly inbox = inject(ImportInbox);
  private readonly searchBox = viewChild.required<ElementRef<HTMLInputElement>>('search');

  /** The search text lives in the URL (`/library?q=…`), so Back restores it. */
  protected readonly query = toSignal(
    inject(ActivatedRoute).queryParamMap.pipe(map((params) => params.get('q') ?? '')),
    { initialValue: '' },
  );

  /** A series page belongs to the Library tab. */
  protected readonly onSeries = toSignal(
    this.router.events.pipe(
      filter((event) => event instanceof NavigationEnd),
      map((event) => event.urlAfterRedirects.startsWith('/series/')),
    ),
    { initialValue: this.router.url.startsWith('/series/') },
  );

  protected readonly themeLabel = computed(() => {
    const mode = this.settings.settings().theme;
    return mode === 'system' ? 'Auto' : mode === 'light' ? 'Paper' : 'Ink';
  });

  protected cycleTheme(): void {
    const at = THEME_ORDER.indexOf(this.settings.settings().theme);
    void this.settings.update({ theme: THEME_ORDER[(at + 1) % THEME_ORDER.length] });
  }

  /** Searching shows the matches on the Library screen. */
  protected onSearch(text: string): void {
    const q = text.trim();
    const onLibrary = this.router.url.split('?')[0] === '/library';
    if (!q && !onLibrary) return;
    void this.router.navigate(['/library'], { queryParams: q ? { q } : {}, replaceUrl: onLibrary });
  }

  protected onKey(event: KeyboardEvent): void {
    if (event.ctrlKey && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      const input = this.searchBox().nativeElement;
      input.focus();
      input.select();
    }
  }
}
