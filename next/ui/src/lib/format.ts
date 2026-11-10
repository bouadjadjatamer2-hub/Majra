import type { Category, DownloadSnapshot, DownloadStatus } from './types';

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

export function formatBytes(value: number, digits = 1): string {
  if (!Number.isFinite(value) || value <= 0) return '0 B';
  let size = value;
  let unit = 0;
  while (size >= 1024 && unit < UNITS.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return `${size.toFixed(unit === 0 ? 0 : digits)} ${UNITS[unit]}`;
}

export function formatSpeed(bytesPerSecond: number): string {
  return bytesPerSecond > 512 ? `${formatBytes(bytesPerSecond)}/s` : '—';
}

export function formatEta(seconds: number | null): string {
  if (seconds === null || !Number.isFinite(seconds) || seconds <= 0) return '—';
  const total = Math.round(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  const pad = (value: number) => String(value).padStart(2, '0');
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(secs)}` : `${minutes}:${pad(secs)}`;
}

export function percent(download: Pick<DownloadSnapshot, 'downloadedBytes' | 'totalBytes'>): number {
  if (download.totalBytes <= 0) return 0;
  return Math.min(100, Math.max(0, (download.downloadedBytes / download.totalBytes) * 100));
}

export function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url.replace(/^https?:\/\//, '').split('/')[0] ?? url;
  }
}

const EXTENSIONS: Record<Exclude<Category, 'other'>, string[]> = {
  video: ['mp4', 'mkv', 'avi', 'mov', 'webm', 'm4v', 'wmv', 'flv', 'mpg', 'mpeg', 'ts'],
  audio: ['mp3', 'flac', 'wav', 'aac', 'ogg', 'm4a', 'opus', 'wma'],
  document: ['pdf', 'doc', 'docx', 'xls', 'xlsx', 'ppt', 'pptx', 'txt', 'epub', 'rtf', 'odt', 'csv'],
  archive: ['zip', 'rar', '7z', 'tar', 'gz', 'bz2', 'xz', 'iso', 'cab'],
  program: ['exe', 'msi', 'msix', 'apk', 'dmg', 'pkg', 'deb', 'rpm', 'appimage'],
};

export function categoryOf(fileName: string): Category {
  const extension = fileName.split('.').pop()?.toLowerCase() ?? '';
  for (const [category, list] of Object.entries(EXTENSIONS)) {
    if (list.includes(extension)) return category as Category;
  }
  return 'other';
}

export function formatWhen(ms: number, lang: 'en' | 'ar'): string {
  try {
    return new Intl.DateTimeFormat(lang === 'ar' ? 'ar' : 'en-GB', {
      day: '2-digit',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
    }).format(new Date(ms));
  } catch {
    return new Date(ms).toLocaleString();
  }
}

export function statusTone(status: DownloadStatus): 'ok' | 'run' | 'warn' | 'bad' | 'info' {
  switch (status) {
    case 'completed':
      return 'ok';
    case 'downloading':
      return 'run';
    case 'paused':
    case 'queued':
      return 'warn';
    case 'error':
      return 'bad';
    default:
      return 'info';
  }
}
