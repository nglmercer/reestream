import type {
  ApiResponse,
  ServerStatus,
  StreamInfo,
  Platform,
  AddStreamRequest,
  AddPlatformRequest,
  UpdatePlatformRequest,
  ConfigResponse,
  Recording,
} from './types';

const BASE = '';

async function request<T>(path: string, init?: RequestInit): Promise<ApiResponse<T>> {
  const res = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...init,
  });
  return res.json() as Promise<ApiResponse<T>>;
}

export const api = {
  getStatus: () => request<ServerStatus>('/api/status'),

  getStreams: () => request<StreamInfo[]>('/api/streams'),

  addStream: (req: AddStreamRequest) =>
    request<string>('/api/streams', {
      method: 'POST',
      body: JSON.stringify(req),
    }),

  removeStream: (id: string) =>
    request<string>(`/api/streams/${id}`, { method: 'DELETE' }),

  getStreamStats: (id: string) =>
    request<StreamInfo>(`/api/streams/${id}/stats`),

  getPlatforms: () => request<Platform[]>('/api/platforms'),

  addPlatform: (req: AddPlatformRequest) =>
    request<string>('/api/platforms', {
      method: 'POST',
      body: JSON.stringify(req),
    }),

  removePlatform: (id: string) =>
    request<string>(`/api/platforms/${id}`, { method: 'DELETE' }),

  updatePlatform: (id: string, req: UpdatePlatformRequest) =>
    request<string>(`/api/platforms/${id}`, {
      method: 'PUT',
      body: JSON.stringify(req),
    }),

  togglePlatform: (id: string) =>
    request<string>(`/api/platforms/${id}/toggle`, { method: 'PUT' }),

  getConfig: () => request<ConfigResponse>('/api/config'),

  updateConfig: (req: { rtmp_addr?: string; rtmp_port?: number; stream_key?: string }) =>
    request<ConfigResponse>('/api/config', {
      method: 'PUT',
      body: JSON.stringify(req),
    }),

  reloadConfig: () =>
    request<string>('/api/config/reload', { method: 'POST' }),

  getRecordings: () => request<Recording[]>('/api/recordings'),

  startRecording: (streamId: string, inputUrl: string) =>
    request<string>('/api/recordings/start', {
      method: 'POST',
      body: JSON.stringify({ stream_id: streamId, input_url: inputUrl }),
    }),

  stopRecording: (id: string) =>
    request<string>(`/api/recordings/${id}/stop`, { method: 'POST' }),

  deleteRecording: (id: string) =>
    request<string>(`/api/recordings/${id}`, { method: 'DELETE' }),
};
