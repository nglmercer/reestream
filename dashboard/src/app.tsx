import { useCallback } from 'preact/hooks';
import { api } from './api';
import type { ServerStatus, StreamInfo, Platform } from './api';
import { usePolling } from './hooks';
import { useLogger } from './components/LogViewer';
import { Header } from './components/Header';
import { StatsCards } from './components/StatsCards';
import { VideoPreview } from './components/VideoPreview';
import { StreamsTable } from './components/StreamsTable';
import { PlatformsTable } from './components/PlatformsTable';
import { LogViewer } from './components/LogViewer';

const STATUS_POLL = 5_000;
const STREAMS_POLL = 10_000;
const PLATFORMS_POLL = 15_000;

export function App() {
  const { logs, addLog, clearLogs } = useLogger();

  const fetchStatus = useCallback(async (): Promise<ServerStatus> => {
    const res = await api.getStatus();
    if (!res.success || !res.data) throw new Error(res.error ?? 'Failed to fetch status');
    return res.data;
  }, []);

  const fetchStreams = useCallback(async (): Promise<StreamInfo[]> => {
    const res = await api.getStreams();
    if (!res.success || !res.data) throw new Error(res.error ?? 'Failed to fetch streams');
    return res.data;
  }, []);

  const fetchPlatforms = useCallback(async (): Promise<Platform[]> => {
    const res = await api.getPlatforms();
    if (!res.success || !res.data) throw new Error(res.error ?? 'Failed to fetch platforms');
    return res.data;
  }, []);

  const status = usePolling(fetchStatus, STATUS_POLL);
  const streams = usePolling(fetchStreams, STREAMS_POLL);
  const platforms = usePolling(fetchPlatforms, PLATFORMS_POLL);

  const handleToggle = useCallback(
    async (id: string) => {
      const res = await api.togglePlatform(id);
      if (res.success) {
        addLog('Platform toggled');
        platforms.refresh();
      } else {
        addLog(`Toggle failed: ${res.error}`, 'error');
      }
    },
    [addLog, platforms],
  );

  if (status.error) addLog(`Status error: ${status.error}`, 'error');
  if (streams.error) addLog(`Streams error: ${streams.error}`, 'error');
  if (platforms.error) addLog(`Platforms error: ${platforms.error}`, 'error');

  const streamNames = (streams.data ?? []).map((s) => ({
    id: s.id,
    name: s.name,
    status: typeof s.status === 'string' ? s.status : Object.keys(s.status)[0],
  }));

  return (
    <div class="min-h-screen bg-slate-950">
      <Header version={status.data?.version ?? '…'} />
      <main class="max-w-7xl mx-auto px-4 sm:px-6 py-6">
        <StatsCards status={status.data} loading={status.loading} />
        <VideoPreview streams={streamNames} />
        <StreamsTable
          streams={streams.data ?? []}
          loading={streams.loading}
          onRefresh={streams.refresh}
        />
        <PlatformsTable
          platforms={platforms.data ?? []}
          loading={platforms.loading}
          onRefresh={platforms.refresh}
          onToggle={handleToggle}
        />
        <LogViewer logs={logs} onClear={clearLogs} />
      </main>
    </div>
  );
}
