import { Unit } from './models';

export function formatSize(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  if (bytes >= 1024 ** 2) return `${Math.round(bytes / 1024 ** 2)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** 7 → "07", 12.5 → "12.5" */
export function pad2(number: number): string {
  return Number.isInteger(number) ? String(number).padStart(2, '0') : String(number);
}

/** "2026-11-11" or an ISO time → "11.11.26" */
export function formatShortDate(date: string): string {
  const [year, month, day] = date.slice(0, 10).split('-');
  return year && month && day ? `${day}.${month}.${year.slice(2)}` : date;
}

/** ISO time → "today", "yesterday", "3 days ago", "2 weeks ago"… */
export function formatAgo(iso: string, now = new Date()): string {
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return '';
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((startOf(now) - startOf(then)) / 86_400_000);
  if (days <= 0) return 'today';
  if (days === 1) return 'yesterday';
  if (days < 14) return `${days} days ago`;
  if (days < 60) return `${Math.round(days / 7)} weeks ago`;
  if (days < 730) return `${Math.round(days / 30)} months ago`;
  return `${Math.round(days / 365)} years ago`;
}

/** "Vol. 7", "Vols. 1–5", "Ch. 12.5" */
export function formatVolume(unit: Unit, number: number, numberEnd?: number | null): string {
  if (unit === 'chapter') {
    return numberEnd != null ? `Chs. ${number}–${numberEnd}` : `Ch. ${number}`;
  }
  return numberEnd != null ? `Vols. ${number}–${numberEnd}` : `Vol. ${number}`;
}

/** Parses "7", "12.5" or a range like "1-5". Returns null if it is not a number. */
export function parseVolumeNumber(text: string): { number: number; numberEnd: number | null } | null {
  const match = /^(\d+(?:\.\d+)?)(?:\s*[-–]\s*(\d+(?:\.\d+)?))?$/.exec(text.trim());
  if (!match) return null;
  const number = Number(match[1]);
  const numberEnd = match[2] === undefined ? null : Number(match[2]);
  if (numberEnd !== null && numberEnd <= number) return null;
  return { number, numberEnd };
}
