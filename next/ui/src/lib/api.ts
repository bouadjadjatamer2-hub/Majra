import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type {
  DownloadEvent,
  DownloadRequest,
  DownloadSnapshot,
  InterfaceInfo,
  ProbeResult,
} from './types';

export const isTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export function demoInterfaces(): InterfaceInfo[] {
  return [
    { id: 'auto', name: 'Auto', address: '', kind: 'other', isDefault: true, network: null },
    {
      id: 'wifi|192.168.1.24',
      name: 'Wi-Fi',
      address: '192.168.1.24',
      kind: 'wifi',
      isDefault: true,
      network: '192.168.1.0/24',
    },
    {
      id: 'eth|10.0.0.7',
      name: 'Ethernet',
      address: '10.0.0.7',
      kind: 'ethernet',
      isDefault: false,
      network: '10.0.0.0/8',
    },
  ];
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

  probeLink(url: string): Promise<ProbeResult> {
    return isTauri ? invoke<ProbeResult>('probe_link', { url }) : demoProbe(url);
  },

  startDownload(request: DownloadRequest): Promise<string> {
    return invoke<string>('start_download', { request });
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

  setSpeedLimit(bytesPerSec: number): Promise<number> {
    return isTauri
      ? invoke<number>('set_speed_limit', { bytesPerSec })
      : Promise.resolve(bytesPerSec);
  },

  onDownloadEvent(handler: (event: DownloadEvent) => void): Promise<() => void> {
    if (!isTauri) return Promise.resolve(() => {});
    return listen<DownloadEvent>('majra://download', (event) => handler(event.payload));
  },
};

export function errorText(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
