import { Injectable, effect, inject, signal } from '@angular/core';
import { KomaApi } from './koma-api';
import { Settings } from './models';

const THEME_KEY = 'koma.theme';

const DEFAULTS: Settings = {
  theme: 'system',
  accent: 'red',
  reduceMotion: false,
  cacheMb: 1024,
  preloadAhead: 6,
  preloadBehind: 2,
  defaultDirection: 'rtl',
  libraryFolder: '',
  watchFolders: [],
};

function storedTheme(): Settings['theme'] {
  try {
    const stored = localStorage.getItem(THEME_KEY);
    if (stored === 'light' || stored === 'dark') return stored;
  } catch {
    // fall through
  }
  return 'system';
}

/** User settings, saved by the Rust side in settings.json. */
@Injectable({ providedIn: 'root' })
export class SettingsService {
  private readonly api = inject(KomaApi);

  // The theme is mirrored in localStorage so the first paint is already right.
  readonly settings = signal<Settings>({ ...DEFAULTS, theme: storedTheme() });

  constructor() {
    if (this.api.inTauri) {
      void this.api.getSettings().then((settings) => this.settings.set(settings));
    }
    effect(() => {
      const { accent, reduceMotion } = this.settings();
      const root = document.documentElement;
      if (accent === 'orange') root.dataset['accent'] = 'orange';
      else delete root.dataset['accent'];
      root.dataset['reduceMotion'] = String(reduceMotion);
    });
  }

  /** Changes some settings and saves them. */
  async update(patch: Partial<Settings>): Promise<void> {
    const next = { ...this.settings(), ...patch };
    this.settings.set(next);
    if (patch.theme) {
      try {
        if (patch.theme === 'system') localStorage.removeItem(THEME_KEY);
        else localStorage.setItem(THEME_KEY, patch.theme);
      } catch {
        // The theme still applies for this session.
      }
    }
    if (!this.api.inTauri) return;
    // Saves run one after another, and only the newest answer is applied,
    // so two quick changes cannot undo each other.
    const turn = ++this.latest;
    const save = this.queue.then(() => this.api.saveSettings(next));
    this.queue = save.catch(() => undefined);
    const stored = await save;
    if (turn === this.latest) this.settings.set(stored);
  }

  private latest = 0;
  private queue: Promise<unknown> = Promise.resolve();
}
