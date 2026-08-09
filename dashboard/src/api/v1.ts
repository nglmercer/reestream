/** Typed client for the versioned Reestream product API.
 *
 * The legacy client in `client.ts` remains for the current dashboard. New
 * screens should depend on this client so API versioning and auth behavior
 * stay in one place.
 */

export interface V1Error {
  code: string;
  message: string;
}

export interface V1Envelope<T> {
  success: boolean;
  data: T;
  error?: V1Error;
  meta?: { page: number; limit: number; total: number };
}

export interface PlatformCatalogEntry {
  id: string;
  name: string;
  slug: string;
  url: string;
  image: { png: string; svg: string };
  capabilities: {
    stream: boolean;
    oauth: boolean;
    chatRead: boolean;
    chatWrite: boolean;
    chatRelay: boolean;
    scheduling: boolean;
    analytics: boolean;
  };
}

export interface Channel {
  id: string;
  platformId: string;
  displayName: string;
  channelUrl: string | null;
  streamUrl: string;
  enabled: boolean;
  status: string;
  createdAt: number;
  updatedAt: number;
  lastError: string | null;
}

export interface ChannelCredentials {
  id: string;
  streamUrl: string;
  streamKey: string;
  rtmpUsername: string | null;
  rtmpPassword: string | null;
}

export type StreamType = 'studio' | 'encoder' | 'file' | 'playlist';
export type EventStatus = 'draft' | 'scheduled' | 'live' | 'ended' | 'cancelled';

export interface Draft {
  id: string;
  name: string;
  streamType: StreamType;
  title: string;
  description: string;
  destinationIds: string[];
  brandId: string | null;
  studioSessionId: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface Event {
  id: string;
  draftId: string | null;
  streamType: StreamType;
  title: string;
  description: string;
  status: EventStatus;
  scheduledFor: string | null;
  createdAt: number;
  updatedAt: number;
  startedAt: number | null;
  endedAt: number | null;
  durationSeconds: number | null;
  destinationIds: string[];
  sourceFileId: string | null;
  loopsCount: number;
  guestLink: string | null;
  ingest: {
    serverUrl: string;
    backupServerUrl: string | null;
    protocol: string;
  };
  recordingFileId: string | null;
  currentViewers: number;
  peakViewers: number;
  chatMessageCount: number;
}

export interface AuthTokens {
  accessToken: string;
  refreshToken: string;
  tokenType: string;
  expiresIn: number;
}

export interface Profile {
  id: string;
  username: string;
  email: string;
  displayName: string;
  timezone: string;
  avatarUrl: string | null;
  createdAt: number;
}

export interface ConnectionSummary {
  id: string;
  platformId: string;
  status: string;
  scopes: string[];
  connectedAt: number;
  updatedAt: number;
}

export interface Transcription {
  id: string;
  eventId: string;
  fileId: string;
  fileName: string;
  status: 'InProgress' | 'Completed' | 'Failed' | 'Unknown' | string;
  language: string | null;
  downloadUrl: string | null;
  createdAt: number;
  updatedAt: number;
}

export interface EventRecordings {
  primaryVideos: Array<{
    fileId: string;
    fileName: string;
    expiresAt: number | null;
    downloadUrl: string;
  }>;
  secondaryVideos: unknown[];
  audio: unknown[];
  files: StorageFile[];
}

export interface ViewerAnalytics {
  total: {
    mean: number;
    max: number;
    viewsTotal: number;
    peakTime: number | null;
    watchedTime: number;
    viewersPerMinute: Array<{ timestamp: number; viewers: number }>;
  };
  byChannel: Array<{
    channelId: string;
    mean: number;
    max: number;
    viewsTotal: number;
    peakTime: number | null;
    watchedTime: number;
    viewersPerMinute: Array<{ timestamp: number; viewers: number }>;
  }>;
}

export interface MessageAnalytics {
  total: {
    messagesTotal: number;
    chattersTotal: number;
    messagesPerMinute: Array<{ timestamp: number; messages: number }>;
  };
  byChannel: Array<{
    channelId: string;
    messagesTotal: number;
    chattersTotal: number;
    messagesPerMinute: Array<{ timestamp: number; messages: number }>;
  }>;
}

export interface ChatMessage {
  id: string;
  eventId: string;
  destinationId: string | null;
  authorName: string;
  authorId: string | null;
  message: string;
  kind: string;
  replyTo: string | null;
  createdAt: number;
  deleted: boolean;
}

export interface AnalyticsReport {
  eventId: string;
  title: string;
  status: EventStatus;
  views: number;
  peakConcurrentViewers: number;
  averageConcurrentViewers: number;
  chatMessages: number;
  durationSeconds: number;
  destinations: Array<{
    destinationId: string;
    views: number;
    peakViewers: number;
    status: string;
  }>;
  timeseries: Array<{
    eventId: string;
    timestamp: number;
    viewers: number;
    bitrateKbps: number;
  }>;
}

export interface StorageFile {
  id: string;
  name: string;
  mimeType: string;
  sizeBytes: number;
  durationSeconds: number | null;
  status: string;
  labels: string[];
  createdAt: number;
  updatedAt: number;
  downloadUrl: string;
}

export interface StudioSession {
  id: string;
  eventId: string;
  status: string;
  layout: string;
  settings: Record<string, unknown>;
  guestLink: string;
  guests: Array<{ id: string; name: string; role: string; joinUrl: string; status: string }>;
  scenes: Array<{
    id: string;
    name: string;
    layout: string;
    sourceIds: string[];
    active: boolean;
  }>;
  activeSceneId: string | null;
}

export class ReestreamApiError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, error: V1Error) {
    super(error.message);
    this.name = 'ReestreamApiError';
    this.status = status;
    this.code = error.code;
  }
}

export class ReestreamApiV1 {
  private readonly baseUrl: string;
  private accessToken: string | null;

  constructor(baseUrl = '/api/v1', accessToken: string | null = null) {
    this.baseUrl = baseUrl.replace(/\/$/, '');
    this.accessToken = accessToken;
  }

  setAccessToken(token: string | null): void {
    this.accessToken = token;
  }

  async request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const headers = new Headers(init.headers);
    if (init.body && !(init.body instanceof FormData)) {
      headers.set('Content-Type', 'application/json');
    }
    if (this.accessToken) {
      headers.set('Authorization', `Bearer ${this.accessToken}`);
    }
    const response = await fetch(`${this.baseUrl}${path}`, { ...init, headers });
    const payload = (await response.json()) as V1Envelope<T>;
    if (!response.ok || !payload.success) {
      throw new ReestreamApiError(
        response.status,
        payload.error ?? { code: 'request_failed', message: response.statusText },
      );
    }
    return payload.data;
  }

  login(email: string, password: string): Promise<AuthTokens> {
    return this.request<AuthTokens>('/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password }),
    });
  }

  refresh(refreshToken: string): Promise<AuthTokens> {
    return this.request<AuthTokens>('/auth/refresh', {
      method: 'POST',
      body: JSON.stringify({ refreshToken }),
    });
  }

  getProfile(): Promise<Profile> {
    return this.request<Profile>('/profile');
  }

  getIngest(): Promise<{ ingestId: string; serverUrl: string; protocol: string }> {
    return this.request('/ingest');
  }

  getGlobalStreamKey(): Promise<{ streamKey: string; srtUrl: string }> {
    return this.request('/stream-key');
  }

  getChatUrl(): Promise<{ webchatUrl: string }> {
    return this.request('/chat-url');
  }

  getConnections(): Promise<ConnectionSummary[]> {
    return this.request<ConnectionSummary[]>('/connections');
  }

  deleteConnection(id: string): Promise<{ deleted: boolean; id: string }> {
    return this.request(`/connections/${encodeURIComponent(id)}`, { method: 'DELETE' });
  }

  getOAuthAuthorizeUrl(
    platformId: string,
    redirectUri: string,
    state: string,
    scope?: string,
  ): Promise<{ platformId: string; authorizationUrl: string; state: string }> {
    const query = new URLSearchParams({ redirectUri, state });
    if (scope) query.set('scope', scope);
    return this.request(`/oauth/${encodeURIComponent(platformId)}/authorize?${query}`);
  }

  exchangeOAuthCode(
    platformId: string,
    request: {
      code?: string;
      redirectUri?: string;
      state?: string;
      refreshToken?: string;
      grantType?: string;
    },
  ): Promise<{ connection: ConnectionSummary; tokenType: string; expiresIn: number | null }> {
    return this.request(`/oauth/${encodeURIComponent(platformId)}/token`, {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  getPlatforms(): Promise<PlatformCatalogEntry[]> {
    return this.request<PlatformCatalogEntry[]>('/platforms');
  }

  getChannels(): Promise<Channel[]> {
    return this.request<Channel[]>('/channels');
  }

  createChannel(request: {
    platformId: string;
    displayName?: string;
    channelUrl?: string;
    streamUrl: string;
    streamKey: string;
    rtmpUsername?: string;
    rtmpPassword?: string;
  }): Promise<Channel> {
    return this.request<Channel>('/channels', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  getChannelCredentials(id: string): Promise<ChannelCredentials> {
    return this.request<ChannelCredentials>(`/channels/${id}/credentials`);
  }

  getDrafts(): Promise<Draft[]> {
    return this.request<Draft[]>('/streams');
  }

  createDraft(request: {
    name: string;
    streamType?: StreamType;
    title?: string;
    description?: string;
    destinationIds?: string[];
    brandId?: string;
  }): Promise<Draft> {
    return this.request<Draft>('/streams', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  getEvents(status?: EventStatus): Promise<Event[]> {
    const query = status ? `?status=${encodeURIComponent(status)}` : '';
    return this.request<Event[]>(`/events${query}`);
  }

  createEvent(request: {
    streamType?: StreamType;
    draftId?: string;
    title?: string;
    description?: string;
    scheduledFor?: string;
    destinationIds?: string[];
    fileId?: string;
    loopsCount?: number;
  }): Promise<Event> {
    return this.request<Event>('/events', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  getEvent(id: string): Promise<Event> {
    return this.request<Event>(`/events/${id}`);
  }

  getEventStreamKey(id: string): Promise<{
    serverUrl: string;
    streamKey: string;
    backupServerUrl: string | null;
    protocol: string;
  }> {
    return this.request(`/events/${id}/stream-key`);
  }

  goLive(id: string): Promise<Event> {
    return this.request<Event>(`/events/${id}/go-live`, { method: 'POST' });
  }

  endEvent(id: string): Promise<Event> {
    return this.request<Event>(`/events/${id}/end`, { method: 'POST' });
  }

  getRecordings(eventId: string): Promise<EventRecordings> {
    return this.request<EventRecordings>(`/events/${eventId}/recordings`);
  }

  getRecordingDownloadUrl(
    eventId: string,
    fileName: string,
  ): Promise<{ downloadUrl: string; expiresIn: number }> {
    return this.request(`/events/${eventId}/recordings/download-url`, {
      method: 'POST',
      body: JSON.stringify({ fileName }),
    });
  }

  getTranscriptions(eventId: string): Promise<{ transcriptions: Transcription[] }> {
    return this.request(`/events/${eventId}/recordings/transcriptions`);
  }

  requestTranscription(eventId: string): Promise<Transcription> {
    return this.request(`/events/${eventId}/recordings/transcriptions`, { method: 'POST' });
  }

  getChat(eventId: string): Promise<ChatMessage[]> {
    return this.request<ChatMessage[]>(`/events/${eventId}/chat`);
  }

  sendChat(eventId: string, message: string, destinationId?: string): Promise<ChatMessage> {
    return this.request<ChatMessage>('/chat/messages', {
      method: 'POST',
      body: JSON.stringify({ eventId, message, destinationId }),
    });
  }

  getChatSources(): Promise<Array<{ id: string; platformId: string; displayName: string; status: string; enabled: boolean }>> {
    return this.request('/chat/sources');
  }

  getChatActions(): Promise<Array<{ id: string; method: string; path: string }>> {
    return this.request('/chat/actions');
  }

  getChatConnections(): Promise<Array<{ id: string; platformId: string; status: string; connected: boolean }>> {
    return this.request('/chat/connections');
  }

  getChatEvents(): Promise<Event[]> {
    return this.request('/chat/events');
  }

  getAnalytics(eventId: string): Promise<AnalyticsReport> {
    return this.request<AnalyticsReport>(`/events/${eventId}/analytics`);
  }

  getViewerAnalytics(eventId: string): Promise<ViewerAnalytics> {
    return this.request(`/events/${eventId}/analytics/viewers`);
  }

  getMessageAnalytics(eventId: string): Promise<MessageAnalytics> {
    return this.request(`/events/${eventId}/analytics/messages`);
  }

  getViewerSamples(eventId: string): Promise<AnalyticsReport['timeseries']> {
    return this.request(`/events/${eventId}/viewers`);
  }

  getChatHistoryDownloadUrl(eventId: string): Promise<{ downloadUrl: string; expiresIn: number }> {
    return this.request(`/events/${eventId}/chat/history/download-url`, { method: 'POST' });
  }

  async uploadFile(file: File, labels: string[] = []): Promise<StorageFile> {
    const form = new FormData();
    form.append('file', file);
    if (labels.length > 0) form.append('labels', labels.join(','));
    return this.request<StorageFile>('/storage/files', { method: 'POST', body: form });
  }

  getStorageFiles(query = ''): Promise<StorageFile[]> {
    const suffix = query ? `?q=${encodeURIComponent(query)}` : '';
    return this.request<StorageFile[]>(`/storage/files${suffix}`);
  }

  getStorageDownloadUrl(id: string): Promise<{ downloadUrl: string; url: string; expiresIn: number }> {
    return this.request(`/storage/files/${id}/download-url`, { method: 'POST' });
  }

  createClip(request: {
    eventId: string;
    name?: string;
    startSeconds: number;
    endSeconds: number;
  }): Promise<{ id: string; status: string }> {
    return this.request('/clips/projects', {
      method: 'POST',
      body: JSON.stringify(request),
    });
  }

  createStudioSession(eventId: string): Promise<StudioSession> {
    return this.request<StudioSession>('/studio/sessions', {
      method: 'POST',
      body: JSON.stringify({ eventId }),
    });
  }

  startStudioSession(id: string): Promise<StudioSession> {
    return this.request<StudioSession>(`/studio/sessions/${id}/start`, { method: 'POST' });
  }

  endStudioSession(id: string): Promise<StudioSession> {
    return this.request<StudioSession>(`/studio/sessions/${id}/end`, { method: 'POST' });
  }

  openChatSocket(eventId: string): WebSocket {
    return this.openSocket(`/chat/ws?eventId=${encodeURIComponent(eventId)}`);
  }

  openStreamingSocket(): WebSocket {
    return this.openSocket('/streaming/ws');
  }

  private openSocket(path: string): WebSocket {
    const base = this.baseUrl.startsWith('http')
      ? this.baseUrl.replace(/^http/, 'ws')
      : `${window.location.protocol === 'https:' ? 'wss:' : 'ws:'}//${window.location.host}${this.baseUrl}`;
    const separator = path.includes('?') ? '&' : '?';
    const token = this.accessToken
      ? `${separator}access_token=${encodeURIComponent(this.accessToken)}`
      : '';
    return new WebSocket(`${base}${path}${token}`);
  }
}

export const apiV1 = new ReestreamApiV1();
