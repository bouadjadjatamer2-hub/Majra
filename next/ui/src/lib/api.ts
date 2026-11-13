import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import { listen } from '@tauri-apps/api/event';
import type {
  AppSettings,
  BridgeOffer,
  BridgeStatus,
  DownloadEvent,
  DownloadRequest,
  DownloadSnapshot,
  InterfaceInfo,
  ProbeResult,
  ScanResult,
  ScheduleEvent,
  SystemProfile,
} from './types';

export const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export function demoInterfaces(): InterfaceInfo[] {
  return [
    { id: 'auto', name: 'Auto', address: '', kind: 'other', isDefault: true, usable: false, network: null, linkSpeedBps: null },
    {
      id: 'wifi|192.168.1.24',
      name: 'Wi-Fi',
      address: '192.168.1.24',
      kind: 'wifi',
      isDefault: true,
      usable: true,
      network: '192.168.1.0/24',
      linkSpeedBps: 866_000_000,
    },
    {
      id: 'eth|10.0.0.7',
      name: 'Ethernet',
      address: '10.0.0.7',
      kind: 'ethernet',
      isDefault: false,
      usable: true,
      network: '10.0.0.0/8',
      linkSpeedBps: 1_000_000_000,
    },
    {
      id: 'usb|192.168.42.2',
      name: 'USB tethering',
      address: '192.168.42.2',
      kind: 'usb',
      isDefault: false,
      usable: false,
      network: '192.168.42.0/24',
      linkSpeedBps: 480_000_000,
    },
  ];
}

export function demoSettings(): AppSettings {
  return {
    defaultDestination: '',
    defaultConnections: 8,
    maxActive: 3,
    extractByDefault: false,
    closeToTray: true,
    startMinimized: false,
    confirmRemove: true,
    startAtLogin: false,
    shutdownWhenDone: false,
    scanWithDefender: false,
    clipboardWatch: false,
    scheduleEnabled: false,
    scheduleStart: '02:00',
    scheduleStop: '07:00',
    scheduleStopAction: 'pause',
    proxyMode: 'system',
    proxyUrl: '',
    proxyUsername: '',
    proxyPassword: '',
    autoTune: true,
    keepAwake: true,
  };
}

function demoFileName(url: string): string {
  try {
    const last = new URL(url).pathname.split('/').filter(Boolean).pop();
    return last ? decodeURIComponent(last) : 'download.bin';
  } catch {
    return 'download.bin';
  }
}

export async function demoProbe(url: string): Promise<ProbeResult> {
  await new Promise((resolve) => setTimeout(resolve, 420));
  return {
    requestedUrl: url,
    finalUrl: url,
    supportsRanges: true,
    totalBytes: 64 * 1024 * 1024 + Math.floor(Math.random() * 512 * 1024 * 1024),
    fileName: demoFileName(url),
    contentType: 'application/octet-stream',
    etag: '"preview"',
    lastModified: null,
  };
}

export const api = {
  listInterfaces(): Promise<InterfaceInfo[]> {
    return isTauri ? invoke<InterfaceInfo[]>('list_interfaces') : Promise.resolve(demoInterfaces());
  },

  testInterface(address: string): Promise<number> {
    return isTauri ? invoke<number>('test_interface', { address }) : Promise.resolve(24);
  },

  getSettings(): Promise<AppSettings> {
    return isTauri ? invoke<AppSettings>('get_settings') : Promise.resolve(demoSettings());
  },

  saveSettings(settings: AppSettings): Promise<AppSettings> {
    return isTauri ? invoke<AppSettings>('save_settings', { settings }) : Promise.resolve(settings);
  },

  settingsPath(): Promise<string> {
    return isTauri
      ? invoke<string>('settings_path')
      : Promise.resolve('%APPDATA%\\app.majra.desktop\\settings.json');
  },

  /// The version the shell was built with. The window shows this, so a release never
  /// displays a number that only exists in a translation file.
  appVersion(): Promise<string> {
    return isTauri ? getVersion() : Promise.resolve('');
  },

  dataFolder(): Promise<string> {
    return isTauri
      ? invoke<string>('data_folder')
      : Promise.resolve('%APPDATA%\\app.majra.desktop');
  },

  bridgeStatus(): Promise<BridgeStatus> {
    return isTauri
      ? invoke<BridgeStatus>('bridge_status')
      : Promise.resolve({ running: true, port: 19288, ports: [19288, 19289, 19290], api: 1 });
  },

  openFolder(path: string): Promise<void> {
    return isTauri ? invoke<void>('open_folder', { path }) : Promise.resolve();
  },

  revealPath(path: string): Promise<void> {
    return isTauri ? invoke<void>('reveal_path', { path }) : Promise.resolve();
  },

  openFile(path: string): Promise<void> {
    return isTauri ? invoke<void>('open_file', { path }) : Promise.resolve();
  },

  systemProfile(): Promise<SystemProfile> {
    return isTauri
      ? invoke<SystemProfile>('system_profile')
      : Promise.resolve({ recommendedConnections: 8, cpuCores: 8, linkSpeedBps: 1_000_000_000, os: 'windows' });
  },

  openUrl(url: string): Promise<void> {
    return isTauri ? invoke<void>('open_url', { url }) : Promise.resolve();
  },

  probeLink(url: string, offerId: string | null = null): Promise<ProbeResult> {
    return isTauri ? invoke<ProbeResult>('probe_link', { url, offerId }) : demoProbe(url);
  },

  startDownload(request: DownloadRequest, offerId: string | null = null): Promise<string> {
    return invoke<string>('start_download', { request, offerId });
  },

  releaseOffer(id: string): Promise<void> {
    return isTauri ? invoke<void>('release_offer', { id }) : Promise.resolve();
  },

  pause(id: string): Promise<void> {
    return invoke<void>('pause_download', { id });
  },

  resume(id: string): Promise<void> {
    return invoke<void>('resume_download', { id });
  },

  cancel(id: string): Promise<void> {
    return invoke<void>('cancel_download', { id });
  },

  forget(id: string): Promise<boolean> {
    return invoke<boolean>('forget_download', { id });
  },

  listDownloads(): Promise<DownloadSnapshot[]> {
    return isTauri ? invoke<DownloadSnapshot[]>('list_downloads') : Promise.resolve([]);
  },

  setMaxActive(value: number): Promise<number> {
    return isTauri ? invoke<number>('set_max_active', { value }) : Promise.resolve(value);
  },

  onDownloadEvent(handler: (event: DownloadEvent) => void): Promise<() => void> {
    if (!isTauri) return Promise.resolve(() => {});
    return listen<DownloadEvent>('majra://download', (event) => handler(event.payload));
  },

  onOffer(handler: (offer: BridgeOffer) => void): Promise<() => void> {
    if (!isTauri) return Promise.resolve(() => {});
    return listen<BridgeOffer>('majra://offer', (event) => handler(event.payload));
  },

  /// The tray's "New download" item: the window is already visible, the UI only has to
  /// open the add dialog.
  onNewDownload(handler: () => void): Promise<() => void> {
    if (!isTauri) return Promise.resolve(() => {});
    return listen<null>('majra://new-download', () => handler());
  },

  readClipboard(): Promise<string> {
    return isTauri ? invoke<string>('read_clipboard') : Promise.resolve('');
  },

  cancelShutdown(): Promise<void> {
    return isTauri ? invoke<void>('cancel_shutdown') : Promise.resolve();
  },

  onScan(handler: (result: ScanResult) => void): Promise<() => void> {
    if (!isTauri) return Promise.resolve(() => {});
    return listen<ScanResult>('majra://scan', (event) => handler(event.payload));
  },

  onShutdown(handler: (seconds: number) => void): Promise<() => void> {
    if (!isTauri) return Promise.resolve(() => {});
    return listen<number>('majra://shutdown', (event) => handler(event.payload));
  },

  onSchedule(handler: (event: ScheduleEvent) => void): Promise<() => void> {
    if (!isTauri) return Promise.resolve(() => {});
    return listen<ScheduleEvent>('majra://schedule', (event) => handler(event.payload));
  },
};

export function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
