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
  network: string | null;
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

export interface DownloadEvent {
  type: 'progress' | 'done' | 'paused' | 'cancelled' | 'error';
  snapshot: DownloadSnapshot;
}

export type Category = 'video' | 'audio' | 'document' | 'archive' | 'program' | 'other';
