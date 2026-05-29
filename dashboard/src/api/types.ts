export interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface ServerStatus {
  version: string;
  uptime_seconds: number;
  active_streams: number;
  total_viewers: number;
}

export interface StreamInfo {
  id: string;
  name: string;
  input_url: string;
  status: StreamStatus;
  started_at: number | null;
  viewers: number;
  bitrate: number;
}

export type StreamStatus =
  | 'Idle'
  | 'Live'
  | { Error: string };

export function isStreamStatusError(status: StreamStatus): status is { Error: string } {
  return typeof status === 'object' && status !== null && 'Error' in status;
}

export function streamStatusLabel(status: StreamStatus): string {
  if (typeof status === 'string') return status;
  return `Error: ${status.Error}`;
}

export interface Platform {
  id: string;
  name: string;
  url: string;
  key: string;
  enabled: boolean;
}

export interface AddStreamRequest {
  name: string;
  input_url: string;
}

export interface AddPlatformRequest {
  name: string;
  url: string;
  key: string;
}

export interface UpdatePlatformRequest {
  name?: string;
  url?: string;
  key?: string;
  enabled?: boolean;
}

export type Orientation = 'horizontal' | 'vertical';

export interface Recording {
  id: string;
  stream_id: string;
  filename: string;
  format: string;
  started_at: number;
  size_bytes: number;
  status: 'recording' | 'completed' | 'failed' | string;
}

export interface ServerInfo {
  rtmp_url: string;
  rtmps_url: string | null;
  srt_url: string | null;
  http_url: string;
  hls_url: string;
  flv_url: string;
  dashboard_url: string;
  api_url: string;
  metrics_url: string;
  stream_key_masked: string;
  rtmp_port: number;
  http_port: number;
  srt_port: number;
  hostname: string;
}

export interface ConfigResponse {
  rtmp_addr: string;
  rtmp_port: number;
  stream_key_masked: string;
  platform_count: number;
  platforms: Array<{
    index: number;
    url: string;
    key_masked: string;
    orientation: Orientation;
  }>;
}

export interface SetupStatus {
  first_run: boolean;
  config_exists: boolean;
  has_stream_key: boolean;
  platform_count: number;
}
