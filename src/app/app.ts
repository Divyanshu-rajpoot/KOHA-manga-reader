import { ChangeDetectionStrategy, Component, computed, inject } from '@angular/core';
import { toSignal } from '@angular/core/rxjs-interop';
import { NavigationEnd, Router, RouterOutlet } from '@angular/router';
import { filter, map } from 'rxjs';
import { ImportInbox } from './core/inbox.service';
import { ThemeService } from './core/theme.service';
import { WindowService } from './core/window.service';
import { Nav } from './shared/nav/nav';
import { TitleBar } from './shared/title-bar/title-bar';

const SCREEN_NAMES: Record<string, string> = {
  '': 'Home',
  library: 'Library',
  series: 'Series',
  import: 'Import',
  settings: 'Settings',
  read: 'Reader',
};

@Component({
  selector: 'app-root',
  imports: [RouterOutlet, TitleBar, Nav],
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (!win.fullscreen()) {
      <koma-title-bar [screen]="screen()" />
    }
    @if (section() !== 'read') {
      <koma-nav />
    }
    <main><router-outlet /></main>
  `,
  styles: `
    :host {
      display: flex;
      flex-direction: column;
      height: 100vh;
    }
    main {
      flex: 1;
      min-height: 0;
      overflow-y: auto;
    }
  `,
})
export class App {
  protected readonly win = inject(WindowService);
  /** First segment of the current route, e.g. "read". */
  protected readonly section = toSignal(
    inject(Router).events.pipe(
      filter((e) => e instanceof NavigationEnd),
      map((e) => e.urlAfterRedirects.split(/[/?#]/)[1] ?? ''),
    ),
    { initialValue: '' },
  );
  protected readonly screen = computed(() => SCREEN_NAMES[this.section()] ?? 'Home');

  constructor() {
    inject(ThemeService); // applies the theme on startup
    inject(ImportInbox); // listens for dropped files and watched folders
  }
}
