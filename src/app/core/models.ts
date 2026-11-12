/** Types shared with the Rust backend (see src-tauri/src/library and import). */

export type SeriesType = 'manga' | 'manhwa';
export type Direction = 'rtl' | 'ltr';
export type Unit = 'volume' | 'chapter';
export type Storage = 'linked' | 'copied';

export interface Volume {
  id: string;
  number: number;
  numberEnd?: number;
  unit: Unit;
  path: string;
  storage: Storage;
  size: number;
  mtime: number;
  pages: number;
  progress?: { page: number; updatedAt: string };
  sessions: { date: string; from: number; to: number }[];
}

export interface Series {
  id: string;
  title: string;
  titleJp?: string;
  type: SeriesType;
  direction: Direction;
  volumes: Volume[];
}

export interface Library {
  version: number;
  series: Series[];
}

export interface AppInfo {
  libraryFolder: string;
  dataFolder: string;
}

export interface Settings {
  theme: 'system' | 'light' | 'dark';
  accent: 'red' | 'orange';
  reduceMotion: boolean;
  /** Memory for ready-to-show pages, in MB. */
  cacheMb: number;
  preloadAhead: number;
  preloadBehind: number;
  defaultDirection: Direction;
  /** Where "Copy into KOMA library" puts files. */
  libraryFolder: string;
  /** Folders checked for new archives. */
  watchFolders: string[];
}

/** Archive details for one volume's file. */
export interface FileInfo {
  volumeId: string;
  /** e.g. "ZIP64 · stored". Empty when the file is missing. */
  archive: string;
  openMs: number;
  missing: boolean;
}

export interface CacheInfo {
  thumbBytes: number;
  pageBytes: number;
}

/** A file found by an import scan. */
export interface Candidate {
  path: string;
  fileName: string;
  size: number;
  pages: number;
  indexMs: number;
  series: string;
  number: number | null;
  numberEnd: number | null;
  unit: Unit;
  seriesType: SeriesType;
  direction: Direction;
  matched: boolean;
  fromComicInfo: boolean;
  existingSeries: boolean;
  duplicate: boolean;
  error: string | null;
}

export interface ImportItem {
  path: string;
  series: string;
  number: number;
  numberEnd: number | null;
  unit: Unit;
  seriesType: SeriesType;
  direction: Direction;
}

export type ImportStep = 'copy' | 'index' | 'cover' | 'saved' | 'thumbs' | 'done' | 'error';

export interface ImportProgress {
  path: string;
  step: ImportStep;
  done: number;
  total: number;
  volumeId: string | null;
  message: string | null;
}
