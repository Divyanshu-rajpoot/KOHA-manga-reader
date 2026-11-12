import { ChangeDetectionStrategy, Component, inject, input } from '@angular/core';
import { WindowService } from '../../core/window.service';

@Component({
  selector: 'koma-title-bar',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    <div class="title" data-tauri-drag-region>KOMA · {{ screen() }}</div>
    <div class="controls">
      <button type="button" aria-label="Minimize" (click)="win.minimize()">
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5h10" /></svg>
      </button>
      <button
        type="button"
        [attr.aria-label]="win.maximized() ? 'Restore' : 'Maximize'"
        (click)="win.toggleMaximize()"
      >
        @if (win.maximized()) {
          <svg viewBox="0 0 10 10" aria-hidden="true">
            <path d="M0.5 2.5h7v7h-7z M2.5 2.5v-2h7v7h-2" />
          </svg>
        } @else {
          <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0.5 0.5h9v9h-9z" /></svg>
        }
      </button>
      <button type="button" class="close" aria-label="Close" (click)="win.close()">
        <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10" /></svg>
      </button>
    </div>
  `,
  styles: `
    :host {
      display: flex;
      align-items: stretch;
      justify-content: space-between;
      height: var(--titlebar-h);
      flex: none;
      background: var(--titlebar-bg);
      color: var(--titlebar-text);
    }
    .title {
      flex: 1;
      display: flex;
      align-items: center;
      padding-left: 14px;
      font-size: 12px;
    }
    .controls {
      display: flex;
    }
    button {
      width: 46px;
      border: 0;
      background: transparent;
      color: inherit;
      display: grid;
      place-items: center;
      transition: background var(--t-micro) var(--ease);
    }
    button:hover {
      background: color-mix(in srgb, var(--titlebar-text) 16%, transparent);
    }
    button.close:hover {
      background: var(--acc);
      color: var(--on-acc);
    }
    button:focus-visible {
      outline-offset: -2px;
    }
    svg {
      width: 10px;
      height: 10px;
      fill: none;
      stroke: currentColor;
      stroke-width: 1;
    }
  `,
})
export class TitleBar {
  protected readonly win = inject(WindowService);
  readonly screen = input('Home');
}
