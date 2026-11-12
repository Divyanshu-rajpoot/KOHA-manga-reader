import { Injectable, computed, effect, inject, signal } from '@angular/core';
import { SettingsService } from './settings.service';

export type Theme = 'light' | 'dark';

/** Applies the light ("paper") or dark ("ink") theme. "System" follows the Windows theme. */
@Injectable({ providedIn: 'root' })
export class ThemeService {
  private readonly settings = inject(SettingsService);
  private readonly systemQuery = matchMedia('(prefers-color-scheme: dark)');
  private readonly systemDark = signal(this.systemQuery.matches);
  private readonly motionQuery = matchMedia('(prefers-reduced-motion: reduce)');
  private readonly systemReducedMotion = signal(this.motionQuery.matches);

  readonly theme = computed<Theme>(() => {
    const mode = this.settings.settings().theme;
    if (mode === 'system') return this.systemDark() ? 'dark' : 'light';
    return mode;
  });

  /** True when either Windows or the in-app setting asks for less motion. */
  readonly reduceMotion = computed(() => this.systemReducedMotion() || this.settings.settings().reduceMotion);

  constructor() {
    this.systemQuery.addEventListener('change', (e) => this.systemDark.set(e.matches));
    this.motionQuery.addEventListener('change', (e) => this.systemReducedMotion.set(e.matches));
    effect(() => {
      document.documentElement.dataset['theme'] = this.theme();
    });
  }
}
