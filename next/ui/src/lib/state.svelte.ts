import { api, demoInterfaces, demoSettings, errorText, isTauri } from './api';
import type { Update } from '@tauri-apps/plugin-updater';
import { categoryOf } from './format';
import { t } from './i18n';
import type {
  AppSettings,
  BridgeOffer,
  BridgeStatus,
  Category,
  ConnectionSnapshot,
  DownloadEvent,
  DownloadRequest,
  DownloadSnapshot,
  InterfaceInfo,
  SystemProfile,
} from './types';

export type InterfaceTest = {
  state: 'testing' | 'ok' | 'fail';
  latency: number | null;
  error: string | null;
};

export type Filter =
  | 'all'
  | 'downloading'
  | 'queued'
  | 'paused'
  | 'completed'
  | 'cancelled'
  | 'error'
  | Category;

export type ToastAction = { label: string; run: () => void };
export type Toast = {
  id: number;
  kind: 'error' | 'info';
  text: string;
  action?: ToastAction;
};

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
      phase: status === 'downloading' ? 'downloading' : '',
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
  showSettings = $state(false);
  /// A link the browser extension confirmed and the user hasn't started or dismissed.
  offer = $state<BridgeOffer | null>(null);
  detailsOpen = $state(true);
  lowRam = $state(true);
  maxActive = $state(0);
  defaultDestination = $state('');
  defaultConnections = $state(8);
  extractByDefault = $state(false);
  confirmRemove = $state(true);
  closeToTray = $state(true);
  startMinimized = $state(false);
  startAtLogin = $state(false);
  shutdownWhenDone = $state(false);
  scanWithDefender = $state(false);
  clipboardWatch = $state(false);
  scheduleEnabled = $state(false);
  scheduleStart = $state('02:00');
  scheduleStop = $state('07:00');
  scheduleStopAction = $state<'pause' | 'exit' | 'shutdown'>('pause');
  proxyMode = $state<'system' | 'none' | 'manual'>('system');
  proxyUrl = $state('');
  proxyUsername = $state('');
  proxyPassword = $state('');
  autoTune = $state(true);
  keepAwake = $state(true);
  checkUpdates = $state(true);
  /// The update dialog and everything it shows: the version found, the release notes,
  /// and how far the download got.
  showUpdate = $state(false);
  updateState = $state<'idle' | 'checking' | 'available' | 'downloading' | 'installing' | 'error'>(
    'idle',
  );
  updateVersion = $state('');
  updateNotes = $state('');
  updateProgress = $state(0);
  updateError = $state<string | null>(null);
  /// Set when the user picks "Later": the same release is not pushed again this run,
  /// but the button in settings can still reopen it.
  updateDismissed = $state(false);
  /// What this machine can do, from the Rust side: a suggested connection count sized
  /// for the default adapter's link rate and the CPU.
  systemProfile = $state<SystemProfile | null>(null);
  /// A link the clipboard watcher found; the add dialog picks it up when it mounts.
  prefillUrl = $state<string | null>(null);
  settingsPath = $state('');
  dataFolder = $state('');
  /// The running build's version, read from the shell so the window never claims a
  /// version the binary is not. The browser preview fills it from the bundled string.
  version = $state('');
  bridge = $state<BridgeStatus | null>(null);
  interfaceTests = $state<Record<string, InterfaceTest>>({});
  speedHistory = $state<Record<string, number[]>>({});
  toasts = $state<Toast[]>([]);
  loaded = $state(false);

  private toastSeq = 0;
  private demoTimer: number | null = null;
  private pendingUpdate: Update | null = null;

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

  /// The connection count a new download starts with. Auto-tune follows the adapter's
  /// link rate; the manual setting is used when the user turns it off.
  get effectiveConnections(): number {
    if (!this.autoTune) return this.defaultConnections;
    return this.systemProfile?.recommendedConnections ?? this.defaultConnections;
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
      this.version = t('version');
      this.applySettings(demoSettings());
      this.downloads = seedDownloads();
      this.seedHistory();
      this.bridge = await api.bridgeStatus();
      this.settingsPath = await api.settingsPath();
      this.dataFolder = await api.dataFolder();
      this.systemProfile = await api.systemProfile();
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
      const [
        interfaces,
        settings,
        downloads,
        bridge,
        settingsPath,
        dataFolder,
        systemProfile,
        version,
      ] = await Promise.all([
        api.listInterfaces(),
        api.getSettings(),
        api.listDownloads(),
        api.bridgeStatus(),
        api.settingsPath(),
        api.dataFolder(),
        api.systemProfile(),
        api.appVersion(),
      ]);
      this.interfaces = interfaces;
      this.applySettings(settings);
      this.downloads = downloads;
      this.bridge = bridge;
      this.settingsPath = settingsPath;
      this.dataFolder = dataFolder;
      this.systemProfile = systemProfile;
      if (version) this.version = version;
      this.selectedId = this.downloads[0]?.id ?? null;
    } catch (error) {
      this.toast('error', errorText(error));
    } finally {
      this.loaded = true;
    }
  }

  applySettings(settings: AppSettings): void {
    this.defaultDestination = settings.defaultDestination ?? '';
    this.defaultConnections = clamp(settings.defaultConnections, 1, 32);
    this.maxActive = clamp(settings.maxActive, 0, 64);
    this.extractByDefault = Boolean(settings.extractByDefault);
    this.closeToTray = settings.closeToTray !== false;
    this.startMinimized = Boolean(settings.startMinimized);
    this.confirmRemove = settings.confirmRemove !== false;
    this.startAtLogin = Boolean(settings.startAtLogin);
    this.shutdownWhenDone = Boolean(settings.shutdownWhenDone);
    this.scanWithDefender = Boolean(settings.scanWithDefender);
    this.clipboardWatch = Boolean(settings.clipboardWatch);
    this.scheduleEnabled = Boolean(settings.scheduleEnabled);
    this.scheduleStart = settings.scheduleStart || '02:00';
    this.scheduleStop = settings.scheduleStop || '07:00';
    this.scheduleStopAction = settings.scheduleStopAction ?? 'pause';
    this.proxyMode = settings.proxyMode ?? 'system';
    this.proxyUrl = settings.proxyUrl ?? '';
    this.proxyUsername = settings.proxyUsername ?? '';
    this.proxyPassword = settings.proxyPassword ?? '';
    this.autoTune = settings.autoTune !== false;
    this.keepAwake = settings.keepAwake !== false;
    this.checkUpdates = settings.checkUpdates !== false;
  }

  currentSettings(): AppSettings {
    return {
      defaultDestination: this.defaultDestination,
      defaultConnections: this.defaultConnections,
      maxActive: this.maxActive,
      extractByDefault: this.extractByDefault,
      closeToTray: this.closeToTray,
      startMinimized: this.startMinimized,
      confirmRemove: this.confirmRemove,
      startAtLogin: this.startAtLogin,
      shutdownWhenDone: this.shutdownWhenDone,
      scanWithDefender: this.scanWithDefender,
      clipboardWatch: this.clipboardWatch,
      scheduleEnabled: this.scheduleEnabled,
      scheduleStart: this.scheduleStart,
      scheduleStop: this.scheduleStop,
      scheduleStopAction: this.scheduleStopAction,
      proxyMode: this.proxyMode,
      proxyUrl: this.proxyUrl,
      proxyUsername: this.proxyUsername,
      proxyPassword: this.proxyPassword,
      autoTune: this.autoTune,
      keepAwake: this.keepAwake,
      checkUpdates: this.checkUpdates,
    };
  }

  async saveSettings(settings: AppSettings): Promise<boolean> {
    try {
      const saved = await api.saveSettings(settings);
      this.applySettings(saved);
      this.persist('majra.maxActive', String(saved.maxActive));
      return true;
    } catch (error) {
      this.toast('error', errorText(error));
      return false;
    }
  }

  /// Ask GitHub for the newest release. A manual check always reports its result; the
  /// quiet startup check only opens the dialog when there is something to install.
  async checkForUpdates(manual = false): Promise<void> {
    if (!isTauri) {
      if (manual) this.toast('info', t('updateUnsupported'));
      return;
    }
    if (
      this.updateState === 'checking' ||
      this.updateState === 'downloading' ||
      this.updateState === 'installing'
    ) {
      return;
    }
    this.updateState = 'checking';
    this.updateError = null;
    try {
      const { check } = await import('@tauri-apps/plugin-updater');
      const found = await check();
      if (!found) {
        this.updateState = 'idle';
        if (manual) this.toast('info', t('updateLatest'));
        return;
      }
      this.pendingUpdate = found;
      this.updateVersion = found.version;
      this.updateNotes = (found.body ?? '').trim();
      this.updateState = 'available';
      if (manual) this.updateDismissed = false;
      if (!this.updateDismissed) this.showUpdate = true;
    } catch (error) {
      // Offline is normal for the startup check, so it stays silent. A manual check
      // has to say why nothing happened.
      this.updateState = 'error';
      this.updateError = errorText(error);
      if (manual) this.toast('error', `${t('updateFailed')} ${this.updateError}`);
    }
  }

  async installUpdate(): Promise<void> {
    if (
      !this.pendingUpdate ||
      this.updateState === 'downloading' ||
      this.updateState === 'installing'
    ) {
      return;
    }
    this.updateState = 'downloading';
    this.updateProgress = 0;
    this.updateError = null;
    try {
      let total = 0;
      let done = 0;
      await this.pendingUpdate.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          total = event.data.contentLength ?? 0;
          done = 0;
          this.updateProgress = 0;
        } else if (event.event === 'Progress') {
          done += event.data.chunkLength;
          if (total > 0) {
            this.updateProgress = Math.min(100, Math.round((done / total) * 100));
          }
        } else {
          this.updateState = 'installing';
          this.updateProgress = 100;
        }
      });
      // The installer has replaced the files; a restart is what actually switches over.
      const { relaunch } = await import('@tauri-apps/plugin-process');
      await relaunch();
    } catch (error) {
      this.updateState = 'error';
      this.updateError = errorText(error);
    }
  }

  /// "Later" closes the dialog but keeps the release around: the settings screen and a
  /// toast action can bring it back without another network request.
  dismissUpdate(): void {
    if (!this.showUpdate) return;
    if (this.updateState === 'downloading' || this.updateState === 'installing') return;
    this.showUpdate = false;
    if (this.updateState !== 'available') return;
    this.updateDismissed = true;
    this.toast('info', t('updateWaiting').replace('{v}', this.updateVersion), {
      label: t('updateNow'),
      run: () => {
        this.showUpdate = true;
      },
    });
  }

  async refreshInterfaces(): Promise<void> {
    try {
      this.interfaces = await api.listInterfaces();
      this.interfaceTests = {};
    } catch (error) {
      this.toast('error', errorText(error));
    }
  }

  async testInterface(interfaceInfo: InterfaceInfo): Promise<void> {
    this.interfaceTests = {
      ...this.interfaceTests,
      [interfaceInfo.id]: { state: 'testing', latency: null, error: null },
    };
    try {
      const latency = await api.testInterface(interfaceInfo.address);
      this.interfaceTests = {
        ...this.interfaceTests,
        [interfaceInfo.id]: { state: 'ok', latency, error: null },
      };
    } catch (error) {
      this.interfaceTests = {
        ...this.interfaceTests,
        [interfaceInfo.id]: { state: 'fail', latency: null, error: errorText(error) },
      };
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
    } else if (event.type === 'done') {
      // The file is on disk and unpacked. Offer to open it right there, so finishing a
      // download does not require finding the row again.
      this.toast('info', `${snapshot.fileName} · ${t('downloadFinished')}`, {
        label: t('open'),
        run: () => void this.openDownload(snapshot),
      });
    }
  }

  pushSpeed(id: string, speed: number): void {
    const history = this.speedHistory[id] ?? [];
    const next = [...history, speed].slice(-48);
    this.speedHistory = { ...this.speedHistory, [id]: next };
  }

  async addDownload(request: DownloadRequest, offerId: string | null = null): Promise<void> {
    if (!isTauri) {
      this.addDemo(request);
      return;
    }
    const id = await api.startDownload(request, offerId);
    this.selectedId = id;
  }

  /// The extension confirmed that its own copy is gone: show the link it handed over.
  receiveOffer(offer: BridgeOffer): void {
    this.offer = offer;
    // A download hand-off is the user asking for something now; the update prompt can
    // wait for the next launch or the settings button instead of covering the dialog.
    this.showUpdate = false;
    this.showAdd = true;
  }

  /// Closing the dialog also drops an unstarted offer, so its cookies are not kept alive.
  closeAdd(): void {
    if (this.offer) {
      if (isTauri) void api.releaseOffer(this.offer.id);
      this.offer = null;
    }
    this.prefillUrl = null;
    this.showAdd = false;
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

  /// Stop a download and delete its partial file. The row stays in the list as
  /// "cancelled" so the user sees what happened; it is not kept for the next session.
  async cancel(id: string, force = false): Promise<void> {
    const download = this.downloads.find((item) => item.id === id);
    if (!download || download.status === 'completed') return;
    if (!force) {
      const question =
        this.lang === 'ar'
          ? 'تلغي هذا التحميل وتحذف الملف الجزئي؟'
          : 'Cancel this download and delete the partial file?';
      if (typeof window !== 'undefined' && !window.confirm(question)) return;
    }
    if (!isTauri) {
      this.setDemoStatus(id, 'cancelled');
      return;
    }
    try {
      await api.cancel(id);
    } catch (error) {
      this.toast('error', errorText(error));
    }
  }

  /// Open a finished download: the extracted folder when unpacking finished, otherwise
  /// the file with the program Windows associates with it.
  async openDownload(download: DownloadSnapshot): Promise<void> {
    if (download.status !== 'completed') return;
    if (download.extraction?.status === 'done') {
      await this.openFolder(download.extraction.folder);
      return;
    }
    try {
      await api.openFile(download.destination);
    } catch (error) {
      this.toast('error', errorText(error));
    }
  }

  async remove(id: string, force = false): Promise<void> {    const download = this.downloads.find((item) => item.id === id);
    if (!download) return;
    if (!force && this.confirmRemove) {
      const question =
        this.lang === 'ar' ? 'تحذف هذا التحميل من القائمة؟' : 'Remove this download from the list?';
      if (typeof window !== 'undefined' && !window.confirm(question)) return;
    }
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

  /// Drop finished, failed and cancelled rows from the list. Files already on disk are
  /// never touched; this only clears Majra's own history.
  async clearHistory(): Promise<number> {
    const ids = this.downloads
      .filter((download) =>
        ['completed', 'cancelled', 'error'].includes(download.status),
      )
      .map((download) => download.id);
    for (const id of ids) {
      if (isTauri) {
        try {
          await api.forget(id);
        } catch (error) {
          this.toast('error', errorText(error));
        }
      }
    }
    if (ids.length > 0) {
      this.downloads = this.downloads.filter((download) => !ids.includes(download.id));
      if (this.selectedId && ids.includes(this.selectedId)) {
        this.selectedId = this.downloads[0]?.id ?? null;
      }
    }
    return ids.length;
  }

  async openFolder(path: string): Promise<void> {
    try {
      await api.openFolder(path);
    } catch (error) {
      this.toast('error', errorText(error));
    }
  }

  async revealPath(path: string): Promise<void> {
    try {
      await api.revealPath(path);
    } catch (error) {
      this.toast('error', errorText(error));
    }
  }

  async openUrl(url: string): Promise<void> {
    try {
      await api.openUrl(url);
    } catch (error) {
      this.toast('error', errorText(error));
    }
  }

  togglePause(id: string): void {
    const download = this.downloads.find((item) => item.id === id);
    if (!download) return;
    if (download.status === 'downloading' || download.status === 'queued') void this.pause(id);
    else if (download.status === 'paused' || download.status === 'error') void this.resume(id);
  }

  setTheme(theme: 'dark' | 'light'): void {
    this.theme = theme;
    this.persist('majra.theme', theme);
  }

  setLang(lang: 'en' | 'ar'): void {
    this.lang = lang;
    this.persist('majra.lang', lang);
  }

  setLowRam(value: boolean): void {
    this.lowRam = value;
    this.persist('majra.lowRam', value ? '1' : '0');
  }

  toggleTheme(): void {
    this.setTheme(this.theme === 'dark' ? 'light' : 'dark');
  }

  toggleLang(): void {
    this.setLang(this.lang === 'en' ? 'ar' : 'en');
  }

  toggleLowRam(): void {
    this.setLowRam(!this.lowRam);
  }

  /// How many downloads may run at once. Zero means no limit: every row starts at
  /// once. Any other value queues the extra rows until a slot frees up.
  setMaxActive(value: number): void {
    const next = Math.max(0, Math.min(64, Math.round(value)));
    if (next === this.maxActive) return;
    this.maxActive = next;
    this.persist('majra.maxActive', String(next));
    if (isTauri) void api.setMaxActive(next).catch((error) => this.toast('error', errorText(error)));
  }

  toast(kind: Toast['kind'], text: string, action?: ToastAction, ttl = 5200): void {
    const id = ++this.toastSeq;
    this.toasts = [...this.toasts, { id, kind, text, action }];
    setTimeout(() => {
      this.toasts = this.toasts.filter((toast) => toast.id !== id);
    }, ttl);
  }

  dismissToast(id: number): void {
    this.toasts = this.toasts.filter((toast) => toast.id !== id);
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
      if (Number.isFinite(maxActive) && maxActive >= 0 && maxActive <= 64) {
        this.maxActive = Math.round(maxActive);
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

function clamp(value: number, min: number, max: number): number {
  const number = Number.isFinite(value) ? Math.round(value) : min;
  return Math.max(min, Math.min(max, number));
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
    filter === 'cancelled' ||
    filter === 'error'
  ) {
    return download.status === filter;
  }
  return categoryOf(download.fileName) === filter;
}

export const app = new AppState();
