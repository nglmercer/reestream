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

export interface ConfigResponse {
  rtmp_addr: string;
  rtmp_port: number;
  stream_key_masked: string;
  platform_count: number;
  platforms: Array<{
    index: number;
    url: string;
    key_masked: string;
    orientation: string;
  }>;
}
