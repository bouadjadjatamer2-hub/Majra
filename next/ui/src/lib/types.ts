export type DownloadStatus =
  | 'queued'
  | 'downloading'
  | 'paused'
  | 'completed'
  | 'cancelled'
  | 'error';

export interface ConnectionSnapshot {
  id: number;
  interfaceId: string;
  interfaceName: string;
  downloadedBytes: number;
  speedBytesPerSec: number;
  status: string;
}

export type ExtractionStatus =
  | 'pending'
  | 'running'
  | 'paused'
  | 'cancelled'
  | 'done'
  | 'error'
  | 'skipped';

export interface ExtractionSnapshot {
  status: ExtractionStatus;
  entriesDone: number;
  entriesTotal: number | null;
  bytesWritten: number;
  bytesTotal: number | null;
  current: string | null;
  folder: string;
  error: string | null;
}

export interface DownloadSnapshot {
  id: string;
  status: DownloadStatus;
  /// `starting` before the first byte, then `downloading`; `reconnecting` after a stall
  /// restart and `finalizing` while the file is being flushed.
  phase?: string;
  url: string;
  fileName: string;
  destination: string;
  totalBytes: number;
  downloadedBytes: number;
  speedBytesPerSec: number;
  etaSeconds: number | null;
  connections: ConnectionSnapshot[];
  error: string | null;
  startedAt: number;
  completedAt: number | null;
  extraction: ExtractionSnapshot | null;
}

export type InterfaceKind = 'wifi' | 'ethernet' | 'usb' | 'other';

export interface InterfaceInfo {
  id: string;
  name: string;
  address: string;
  kind: InterfaceKind;
  isDefault: boolean;
  usable: boolean;
  network: string | null;
  /// The adapter's reported link rate in bits/s; null when Windows cannot say.
  linkSpeedBps?: number | null;
}

export interface SystemProfile {
  recommendedConnections: number;
  cpuCores: number;
  linkSpeedBps: number | null;
  os: string;
}

export type ScheduleStopAction = 'pause' | 'exit' | 'shutdown';
export type ProxyMode = 'system' | 'none' | 'manual';

export interface AppSettings {
  defaultDestination: string;
  defaultConnections: number;
  maxActive: number;
  extractByDefault: boolean;
  closeToTray: boolean;
  startMinimized: boolean;
  confirmRemove: boolean;
  startAtLogin: boolean;
  shutdownWhenDone: boolean;
  scanWithDefender: boolean;
  clipboardWatch: boolean;
  scheduleEnabled: boolean;
  scheduleStart: string;
  scheduleStop: string;
  scheduleStopAction: ScheduleStopAction;
  proxyMode: ProxyMode;
  proxyUrl: string;
  proxyUsername: string;
  proxyPassword: string;
  autoTune: boolean;
  keepAwake: boolean;
}

export interface ScanResult {
  id: string;
  fileName: string;
  status: 'clean' | 'threat' | 'unavailable' | 'failed';
  detail: string;
}

export interface ScheduleEvent {
  state: 'start' | 'stop';
  action: string;
}

export interface BridgeStatus {
  running: boolean;
  port: number;
  ports: number[];
  api: number;
}

export interface ProbeResult {
  requestedUrl: string;
  finalUrl: string;
  supportsRanges: boolean;
  totalBytes: number | null;
  fileName: string;
  contentType: string | null;
  etag: string | null;
  lastModified: string | null;
}

export interface DownloadRequest {
  url: string;
  destination: string;
  connections: number;
  interfaceIds: string[];
  blockSize?: number | null;
  extract?: boolean;
}

/// One link the browser extension confirmed. The cookies stay in the Rust backend.
export interface BridgeOffer {
  id: string;
  url: string;
  fileName: string | null;
  pageUrl: string | null;
  from: string | null;
  probe: ProbeResult;
}

export interface DownloadEvent {
  type: 'progress' | 'done' | 'paused' | 'cancelled' | 'error';
  snapshot: DownloadSnapshot;
}

export type Category = 'video' | 'audio' | 'document' | 'archive' | 'program' | 'other';
