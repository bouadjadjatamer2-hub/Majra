import { api, demoInterfaces, errorText, isTauri } from './api';
import { categoryOf } from './format';
import type {
  Category,
  ConnectionSnapshot,
  DownloadEvent,
  DownloadRequest,
  DownloadSnapshot,
  InterfaceInfo,
} from './types';

export type Filter =
  | 'all'
  | 'downloading'
  | 'queued'
  | 'paused'
  | 'completed'
  | 'error'
  | Category;

export type Toast = { id: number; kind: 'error' | 'info'; text: string };

const MB = 1024 * 1024;
const GB = 1024 * MB;
const DEMO_IFACES = [
  { id: 'wifi|192.168.1.24', name: 'Wi-Fi' },
  { id: 'eth|10.0.0.7', name: 'Ethernet' },
];

function demoConnections(
  count: number,
  speed: number,
  status: 'downloading' | 'paused' | 'idle' | 'done' = 'downloading',
): ConnectionSnapshot[] {
  return Array.from({ length: count }, (_, index) => {
    const iface = DEMO_IFACES[index % DEMO_IFACES.length]!;
    const share = speed / Math.max(1, count);
    return {
      id: index,
      interfaceId: iface.id,
      interfaceName: iface.name,
      downloadedBytes: 0,
      speedBytesPerSec:
        status === 'downloading' ? share * (0.6 + ((index * 37) % 80) / 100) : 0,
      status: status === 'downloading' ? (index % 7 === 6 ? 'retry 1' : 'downloading') : status,
    };
  });
}

function seedDownloads(): DownloadSnapshot[] {
  const now = Date.now();
  const row = (
    id: string,
    fileName: string,
    status: DownloadSnapshot['status'],
    totalBytes: number,
    downloadedBytes: number,
    speedBytesPerSec: number,
    minutesAgo: number,
    extra: Partial<DownloadSnapshot> = {},
  ): DownloadSnapshot => {
    const remaining = totalBytes - downloadedBytes;
    return {
      id,
      status,
      url: `https://cdn.example.net/releases/${encodeURIComponent(fileName)}`,
      fileName,
      destination: `C:\\Users\\Lenovo\\Downloads\\${fileName}`,
      totalBytes,
      downloadedBytes,
      speedBytesPerSec,
      etaSeconds:
        status === 'downloading' && speedBytesPerSec > 0
          ? Math.round(remaining / speedBytesPerSec)
          : null,
      connections: demoConnections(
        extra.connections?.length ?? 8,
        speedBytesPerSec,
        status === 'downloading'
          ? 'downloading'
          : status === 'paused'
            ? 'paused'
            : status === 'completed'
              ? 'done'
              : 'idle',
      ),
      error: null,
      startedAt: now - minutesAgo * 60_000,
      completedAt: status === 'completed' ? now - (minutesAgo - 8) * 60_000 : null,
      extraction: null,
      ...extra,
    };
  };

  return [
    row('demo-ubuntu', 'ubuntu-24.04-desktop-amd64.iso', 'downloading', 6.1 * GB, 3.82 * GB, 6.4 * MB, 14),
    row('demo-c4d', 'Cinema-4D-Studio-R26.zip', 'paused', 1.2 * GB, 460 * MB, 0, 46, {
      extraction: {
        status: 'paused',
        entriesDone: 34,
        entriesTotal: 118,
        bytesWritten: 210 * MB,
        bytesTotal: 640 * MB,
        current: null,
        folder: 'C:\\Users\\Lenovo\\Downloads\\Cinema-4D-Studio-R26',
        error: null,
      },
    }),
    row('demo-wedding', 'wedding-highlights-4k.mp4', 'downloading', 2.3 * GB, 1.86 * GB, 3.2 * MB, 9),
    row('demo-report', 'report-q3-final.pdf', 'completed', 12.4 * MB, 12.4 * MB, 0, 190),
    row('demo-node', 'node-v22.11.0-x64.msi', 'completed', 30.1 * MB, 30.1 * MB, 0, 320),
    row('demo-archive', 'family-archive-2019.7z', 'error', 4.7 * GB, 2.2 * GB, 0, 74, {
      error: 'The server closed the connection during block 18.',
    }),
    row('demo-notes', 'lecture-notes-week-07.pdf', 'queued', 8.9 * MB, 0, 0, 2),
    row('demo-photos', 'photos-2024-backup.zip', 'completed', 1.4 * GB, 1.4 * GB, 0, 500),
  ];
}

class AppState {
  downloads = $state<DownloadSnapshot[]>([]);
  interfaces = $state<InterfaceInfo[]>(demoInterfaces());
  filter = $state<Filter>('all');
  selectedId = $state<string | null>(null);
  query = $state('');
  lang = $state<'en' | 'ar'>('en');
  theme = $state<'dark' | 'light'>('dark');
  showAdd = $state(false);
  detailsOpen = $state(true);
  lowRam = $state(true);
  maxActive = $state(3);
  /// Global ceiling in bytes per second, 0 = unlimited.
  speedLimit = $state(0);
  speedHistory = $state<Record<string, number[]>>({});
  toasts = $state<Toast[]>([]);
  loaded = $state(false);

  private toastSeq = 0;
  private demoTimer: number | null = null;

  get selected(): DownloadSnapshot | null {
    return this.downloads.find((download) => download.id === this.selectedId) ?? null;
  }

  get active(): DownloadSnapshot[] {
    return this.downloads.filter((download) => download.status === 'downloading');
  }

  get totalSpeed(): number {
    return this.active.reduce((sum, download) => sum + download.speedBytesPerSec, 0);
  }

  get pausedCount(): number {
    return this.downloads.filter(
      (download) => download.status === 'paused' || download.status === 'error',
    ).length;
  }

  get visible(): DownloadSnapshot[] {
    const query = this.query.trim().toLowerCase();
    return this.downloads.filter((download) => {
      if (
        query &&
        !`${download.fileName} ${download.url}`.toLowerCase().includes(query)
      ) {
        return false;
      }
      return matchesFilter(download, this.filter);
    });
  }

  count(filter: Filter): number {
    if (filter === 'all') return this.downloads.length;
    return this.downloads.filter((download) => matchesFilter(download, filter)).length;
  }

  async init(): Promise<void> {
    this.restorePreferences();
    if (!isTauri) {
      this.downloads = seedDownloads();
      this.seedHistory();
      const wanted =
        typeof location !== 'undefined'
          ? new URLSearchParams(location.search).get('select')
          : null;
      this.selectedId =
        wanted && this.downloads.some((download) => download.id === wanted)
          ? wanted
          : (this.downloads[0]?.id ?? null);
      this.startDemoTicker();
      this.loaded = true;
      return;
    }
    try {
      this.interfaces = await api.listInterfaces();
      await api.setMaxActive(this.maxActive);
      await api.setSpeedLimit(this.speedLimit);
      this.downloads = await api.listDownloads();
      this.selectedId = this.downloads[0]?.id ?? null;
    } catch (error) {
      this.toast('error', errorText(error));
    } finally {
      this.loaded = true;
    }
  }

  applyEvent(event: DownloadEvent): void {
    const snapshot = event.snapshot;
    const index = this.downloads.findIndex((download) => download.id === snapshot.id);
    if (index === -1) {
      this.downloads = [snapshot, ...this.downloads];
    } else {
      this.downloads[index] = snapshot;
    }
    if (!this.selectedId) this.selectedId = snapshot.id;
    if (snapshot.status === 'downloading') {
      this.pushSpeed(snapshot.id, snapshot.speedBytesPerSec);
    }
    if (event.type === 'error' && snapshot.error) {
      this.toast('error', snapshot.error);
    }
  }

  pushSpeed(id: string, speed: number): void {
    const history = this.speedHistory[id] ?? [];
    const next = [...history, speed].slice(-48);
    this.speedHistory = { ...this.speedHistory, [id]: next };
  }

  async addDownload(request: DownloadRequest): Promise<void> {
    if (!isTauri) {
      this.addDemo(request);
      return;
    }
    const id = await api.startDownload(request);
    this.selectedId = id;
  }

  async pause(id: string): Promise<void> {
    if (!isTauri) {
      this.setDemoStatus(id, 'paused');
      return;
    }
    try {
      await api.pause(id);
    } catch (error) {
      this.toast('error', errorText(error));
    }
  }

  async resume(id: string): Promise<void> {
    if (!isTauri) {
      this.setDemoStatus(id, 'downloading');
      return;
    }
    try {
      await api.resume(id);
    } catch (error) {
      this.toast('error', errorText(error));
    }
  }

  /// Start every paused or failed row, one at a time. Rows restored from the last
  /// session land here first: nothing downloads until the user asks for it.
  async resumeAll(): Promise<void> {
    const ids = this.downloads
      .filter((download) => download.status === 'paused' || download.status === 'error')
      .map((download) => download.id);
    for (const id of ids) {
      await this.resume(id);
    }
  }

  async remove(id: string): Promise<void> {
    const download = this.downloads.find((item) => item.id === id);
    if (!download) return;
    if (isTauri) {
      try {
        if (download.status === 'downloading' || download.status === 'paused' || download.status === 'queued') {
          await api.cancel(id);
        }
        await api.forget(id);
      } catch (error) {
        this.toast('error', errorText(error));
      }
    }
    this.downloads = this.downloads.filter((item) => item.id !== id);
    if (this.selectedId === id) {
      this.selectedId = this.downloads[0]?.id ?? null;
    }
  }

  togglePause(id: string): void {
    const download = this.downloads.find((item) => item.id === id);
    if (!download) return;
    if (download.status === 'downloading' || download.status === 'queued') void this.pause(id);
    else if (download.status === 'paused' || download.status === 'error') void this.resume(id);
  }

  toggleTheme(): void {
    this.theme = this.theme === 'dark' ? 'light' : 'dark';
    this.persist('majra.theme', this.theme);
  }

  toggleLang(): void {
    this.lang = this.lang === 'en' ? 'ar' : 'en';
    this.persist('majra.lang', this.lang);
  }

  toggleLowRam(): void {
    this.lowRam = !this.lowRam;
    this.persist('majra.lowRam', this.lowRam ? '1' : '0');
  }

  /// How many downloads may run at once. Rows above the limit wait as Queued and the
  /// core starts them as slots free up.
  setMaxActive(value: number): void {
    const next = Math.max(1, Math.min(8, Math.round(value)));
    if (next === this.maxActive) return;
    this.maxActive = next;
    this.persist('majra.maxActive', String(next));
    if (isTauri) void api.setMaxActive(next);
  }

  /// One ceiling for the whole list, in bytes per second. Zero means unlimited.
  setSpeedLimit(bytesPerSec: number): void {
    const next = Math.max(0, Math.min(100 * MB, Math.round(bytesPerSec)));
    if (next === this.speedLimit) return;
    this.speedLimit = next;
    this.persist('majra.speedLimit', String(next));
    if (isTauri) void api.setSpeedLimit(next);
  }

  toast(kind: Toast['kind'], text: string): void {
    const id = ++this.toastSeq;
    this.toasts = [...this.toasts, { id, kind, text }];
    setTimeout(() => {
      this.toasts = this.toasts.filter((toast) => toast.id !== id);
    }, 5200);
  }

  private setDemoStatus(id: string, status: DownloadSnapshot['status']): void {
    this.downloads = this.downloads.map((download) => {
      if (download.id !== id) return download;
      const speed = status === 'downloading' ? Math.max(2.2 * MB, download.speedBytesPerSec || 4.4 * MB) : 0;
      return {
        ...download,
        status,
        speedBytesPerSec: speed,
        etaSeconds:
          status === 'downloading'
            ? Math.max(1, Math.round((download.totalBytes - download.downloadedBytes) / speed))
            : null,
        connections: download.connections.map((connection) => ({
          ...connection,
          speedBytesPerSec:
            status === 'downloading' ? speed / Math.max(1, download.connections.length) : 0,
          status: status === 'downloading' ? 'downloading' : status,
        })),
      };
    });
  }

  private addDemo(request: DownloadRequest): void {
    const fileName = fileNameOf(request.url);
    const total = 48 * MB + Math.floor(Math.random() * 320 * MB);
    const connections = Math.max(1, Math.min(request.connections || 8, 8));
    const speed = (2.4 + Math.random() * 4.2) * MB;
    const id = `demo-${Date.now()}`;
    const download: DownloadSnapshot = {
      id,
      status: 'downloading',
      url: request.url,
      fileName,
      destination: request.destination
        ? `${request.destination.replace(/[\\/]$/, '')}\\${fileName}`
        : `C:\\Users\\Lenovo\\Downloads\\${fileName}`,
      totalBytes: total,
      downloadedBytes: 0,
      speedBytesPerSec: speed,
      etaSeconds: Math.round(total / speed),
      connections: demoConnections(connections, speed),
      error: null,
      startedAt: Date.now(),
      completedAt: null,
      extraction: request.extract
        ? {
            status: 'running',
            entriesDone: 0,
            entriesTotal: null,
            bytesWritten: 0,
            bytesTotal: null,
            current: null,
            folder: (request.destination || 'C:\\Users\\Lenovo\\Downloads').replace(/[\\/]$/, '') + '\\' + fileName.replace(/\.[^.]+$/, ''),
            error: null,
          }
        : null,
    };
    this.downloads = [download, ...this.downloads];
    this.selectedId = id;
    this.pushSpeed(id, speed);
  }

  private seedHistory(): void {
    const history: Record<string, number[]> = {};
    for (const download of this.downloads) {
      if (download.status !== 'downloading') continue;
      const base = download.speedBytesPerSec;
      history[download.id] = Array.from({ length: 40 }, (_, index) => {
        const wave = Math.sin(index / 4.2) * 0.16 + Math.sin(index / 1.7) * 0.06;
        return Math.max(120_000, base * (1 + wave));
      });
    }
    this.speedHistory = history;
  }

  private startDemoTicker(): void {
    if (this.demoTimer !== null) return;
    this.demoTimer = window.setInterval(() => {
      const now = Date.now();
      this.downloads = this.downloads.map((download) => {
        if (download.status !== 'downloading') return download;
        const speed = Math.max(
          220_000,
          download.speedBytesPerSec * (0.94 + Math.random() * 0.12) || 3 * MB,
        );
        const gained = speed * 0.6;
        const downloadedBytes = Math.min(download.totalBytes, download.downloadedBytes + gained);
        const completed = downloadedBytes >= download.totalBytes;
        const extraction = download.extraction
          ? completed
            ? {
                ...download.extraction,
                status: 'done' as const,
                entriesDone:
                  download.extraction.entriesTotal ?? download.extraction.entriesDone,
                bytesWritten:
                  download.extraction.bytesTotal ?? download.extraction.bytesWritten,
                current: null,
              }
            : download.extraction.status === 'running'
              ? {
                  ...download.extraction,
                  entriesDone:
                    download.extraction.entriesDone + (Math.random() < 0.35 ? 1 : 0),
                  bytesWritten: Math.min(
                    download.extraction.bytesTotal ?? Number.POSITIVE_INFINITY,
                    download.extraction.bytesWritten + 2 * MB,
                  ),
                }
              : download.extraction
          : null;
        return {
          ...download,
          downloadedBytes,
          speedBytesPerSec: completed ? 0 : speed,
          etaSeconds: completed
            ? null
            : Math.max(1, Math.round((download.totalBytes - downloadedBytes) / speed)),
          status: completed ? 'completed' : 'downloading',
          completedAt: completed ? now : null,
          extraction,
          connections: download.connections.map((connection, index) => ({
            ...connection,
            downloadedBytes: connection.downloadedBytes + gained / download.connections.length,
            speedBytesPerSec: completed
              ? 0
              : (speed / download.connections.length) * (0.72 + ((index * 29) % 60) / 100),
            status: completed ? 'done' : index % 7 === 6 ? 'retry 1' : 'downloading',
          })),
        };
      });
      for (const download of this.downloads) {
        if (download.status === 'downloading') this.pushSpeed(download.id, download.speedBytesPerSec);
      }
    }, 650);
  }

  private restorePreferences(): void {
    if (typeof localStorage !== 'undefined') {
      const theme = localStorage.getItem('majra.theme');
      if (theme === 'light' || theme === 'dark') this.theme = theme;
      const lang = localStorage.getItem('majra.lang');
      if (lang === 'ar' || lang === 'en') this.lang = lang;
      const lowRam = localStorage.getItem('majra.lowRam');
      if (lowRam === '0' || lowRam === '1') this.lowRam = lowRam === '1';
      const maxActive = Number(localStorage.getItem('majra.maxActive'));
      if (Number.isFinite(maxActive) && maxActive >= 1 && maxActive <= 8) {
        this.maxActive = Math.round(maxActive);
      }
      const speedLimit = Number(localStorage.getItem('majra.speedLimit'));
      if (Number.isFinite(speedLimit) && speedLimit >= 0 && speedLimit <= 100 * MB) {
        this.speedLimit = Math.round(speedLimit);
      }
    }

    // Deep links (and visual checks) can pin the presentation: ?theme=light&lang=ar&lowram=0
    if (typeof location === 'undefined') return;
    const params = new URLSearchParams(location.search);
    const theme = params.get('theme');
    if (theme === 'light' || theme === 'dark') this.theme = theme;
    const lang = params.get('lang');
    if (lang === 'ar' || lang === 'en') this.lang = lang;
    const lowRam = params.get('lowram');
    if (lowRam === '0' || lowRam === '1') this.lowRam = lowRam === '1';
  }

  private persist(key: string, value: string): void {
    try {
      localStorage.setItem(key, value);
    } catch {
      // A locked-down webview can refuse storage; the session still works.
    }
  }
}

function fileNameOf(url: string): string {
  try {
    const last = new URL(url).pathname.split('/').filter(Boolean).pop();
    return last ? decodeURIComponent(last) : 'download.bin';
  } catch {
    return 'download.bin';
  }
}

function matchesFilter(download: DownloadSnapshot, filter: Filter): boolean {
  if (filter === 'all') return true;
  if (
    filter === 'downloading' ||
    filter === 'queued' ||
    filter === 'paused' ||
    filter === 'completed' ||
    filter === 'error'
  ) {
    return download.status === filter;
  }
  return categoryOf(download.fileName) === filter;
}

export const app = new AppState();
