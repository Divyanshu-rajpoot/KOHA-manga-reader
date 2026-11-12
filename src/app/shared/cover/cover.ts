import { ChangeDetectionStrategy, Component, computed, inject, input, signal } from '@angular/core';
import { KomaApi } from '../../core/koma-api';

/** A volume's cover: the first page of its archive, in a 3:4.3 ink frame. */
@Component({
  selector: 'koma-cover',
  changeDetection: ChangeDetectionStrategy.OnPush,
  template: `
    @if (!failed()) {
      <img [src]="url()" alt="" [attr.loading]="lazy() ? 'lazy' : null" draggable="false" (error)="failed.set(true)" />
    }
    @if (badge(); as text) {
      <span class="badge">{{ text }}</span>
    }
  `,
  styles: `
    :host {
      position: relative;
      display: block;
      aspect-ratio: 3 / 4.3;
      border: var(--border-panel) solid var(--ink);
      background: var(--bg2);
      overflow: hidden;
    }
    :host(.thin) {
      border-width: var(--border-control);
    }
    img {
      display: block;
      width: 100%;
      height: 100%;
      object-fit: cover;
    }
    .badge {
      position: absolute;
      right: 8px;
      bottom: 8px;
      min-width: 30px;
      padding: 2px 6px;
      background: var(--ink);
      color: var(--on-ink);
      font-family: var(--font-display);
      font-size: 16px;
      text-align: center;
    }
  `,
})
export class Cover {
  private readonly api = inject(KomaApi);

  readonly volumeId = input.required<string>();
  /** Small label in the corner, usually the volume number. */
  readonly badge = input<string | null>(null);
  readonly lazy = input(true);

  protected readonly failed = signal(false);
  protected readonly url = computed(() => this.api.coverUrl(this.volumeId()));
}
