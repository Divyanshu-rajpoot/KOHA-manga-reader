import { Routes } from '@angular/router';
import { Home } from './features/home/home';
import { Import } from './features/import/import';
import { LibraryScreen } from './features/library/library';
import { Reader } from './features/reader/reader';
import { SeriesScreen } from './features/series/series';
import { SettingsScreen } from './features/settings/settings';

export const routes: Routes = [
  { path: '', component: Home },
  { path: 'library', component: LibraryScreen },
  { path: 'series/:seriesId', component: SeriesScreen },
  { path: 'import', component: Import },
  { path: 'settings', component: SettingsScreen },
  { path: 'read/:volumeId', component: Reader },
  { path: '**', redirectTo: '' },
];
