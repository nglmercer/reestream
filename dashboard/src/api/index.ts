export { api } from './client';
export type {
  ApiResponse,
  ServerStatus,
  StreamInfo,
  StreamStatus,
  Platform,
  AddStreamRequest,
  AddPlatformRequest,
  UpdatePlatformRequest,
  ConfigResponse,
  Orientation,
  Recording,
  ServerInfo,
  SetupStatus,
} from './types';
export { isStreamStatusError, streamStatusLabel } from './types';
