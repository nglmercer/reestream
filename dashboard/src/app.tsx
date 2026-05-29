import { useCallback, useState, useEffect } from 'preact/hooks';
import { api } from './api';
import type { ServerStatus, StreamInfo, Platform } from './api';
import { usePolling, useStreamWs } from './hooks';
import { useLogger } from './components/LogViewer';
import { Header } from './components/Header';
import { StatsCards } from './components/StatsCards';
import { VideoPreview } from './components/VideoPreview';
import { StreamsTable } from './components/StreamsTable';
import { PlatformsTable } from './components/PlatformsTable';
import { LogViewer } from './components/LogViewer';
import { SetupWizard } from './components/SetupWizard';
import { SettingsPanel } from './components/SettingsPanel';
import { RecordingControls } from './components/RecordingControls';

const STATUS_POLL = 5_000;
const PLATFORMS_POLL = 15_000;

export function App() {
  const { logs, addLog, clearLogs } = useLogger();
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [liveStreams, setLiveStreams] = useState<StreamInfo[]>([]);
  const [wsConnected, setWsConnected] = useState(false);

  useEffect(() => {
    const ctrl = new AbortController();
    fetch('/api/setup/status', { signal: ctrl.signal })
      .then((r) => r.json())
      .then((d) => {
        if (d.success) setNeedsSetup(d.data.first_run);
        else setNeedsSetup(false);
      })
      .catch(() => setNeedsSetup(false));
    return () => ctrl.abort();
  }, []);

  useStreamWs({
    onInit: (streams) => {
      setLiveStreams(streams as StreamInfo[]);
      setWsConnected(true);
    },
    onStarted: (id, name, input_url) => {
      addLog(`Stream started: ${name}`);
      setLiveStreams((prev) => {
        if (prev.some((s) => s.id === id)) return prev;
        return [...prev, {
          id,
          name,
          input_url,
          status: 'Live',
          started_at: Math.floor(Date.now() / 1000),
          viewers: 0,
          bitrate: 0,
        }];
      });
    },
    onStopped: (id) => {
      addLog('Stream ended');
      setLiveStreams((prev) => prev.filter((s) => s.id !== id));
    },
    onUpdated: (id, viewers, bitrate) => {
      setLiveStreams((prev) =>
        prev.map((s) => (s.id === id ? { ...s, viewers, bitrate } : s)),
      );
    },
    onError: (id, message) => {
      addLog(`Stream error: ${message}`, 'error');
      setLiveStreams((prev) =>
        prev.map((s) => (s.id === id ? { ...s, status: { Error: message } } : s)),
      );
    },
  });

  const fetchStreams = useCallback(async (): Promise<StreamInfo[]> => {
    const res = await api.getStreams();
    if (!res.success || !res.data) throw new Error(res.error ?? 'Failed to fetch streams');
    return res.data;
  }, []);

  const fetchStatus = useCallback(async (): Promise<ServerStatus> => {
    const res = await api.getStatus();
    if (!res.success || !res.data) throw new Error(res.error ?? 'Failed to fetch status');
    return res.data;
  }, []);

  const fetchPlatforms = useCallback(async (): Promise<Platform[]> => {
    const res = await api.getPlatforms();
    if (!res.success || !res.data) throw new Error(res.error ?? 'Failed to fetch platforms');
    return res.data;
  }, []);

  const status = usePolling(fetchStatus, STATUS_POLL);
  const streamsPoll = usePolling(fetchStreams, 10_000);
  const platforms = usePolling(fetchPlatforms, PLATFORMS_POLL);

  const streams = wsConnected ? { data: liveStreams, loading: false, refresh: streamsPoll.refresh } : streamsPoll;

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

  const handleAddPlatform = useCallback(
    async (name: string, url: string, key: string) => {
      const res = await api.addPlatform({ name, url, key });
      if (res.success) {
        addLog(`Platform "${name}" added`);
        platforms.refresh();
      } else {
        throw new Error(res.error ?? 'Failed to add platform');
      }
    },
    [addLog, platforms],
  );

  const handleRemovePlatform = useCallback(
    async (id: string) => {
      const res = await api.removePlatform(id);
      if (res.success) {
        addLog('Platform removed');
        platforms.refresh();
      } else {
        addLog(`Remove failed: ${res.error}`, 'error');
      }
    },
    [addLog, platforms],
  );

  const handleUpdatePlatform = useCallback(
    async (id: string, req: { name?: string; url?: string; key?: string; enabled?: boolean }) => {
      const res = await api.updatePlatform(id, req);
      if (res.success) {
        addLog('Platform updated');
        platforms.refresh();
      } else {
        addLog(`Update failed: ${res.error}`, 'error');
      }
    },
    [addLog, platforms],
  );

  if (status.error) addLog(`Status error: ${status.error}`, 'error');
  if (platforms.error) addLog(`Platforms error: ${platforms.error}`, 'error');

  if (needsSetup === true) {
    return <SetupWizard />;
  }

  if (needsSetup === null) {
    return (
      <div class="min-h-screen bg-surface flex items-center justify-center">
        <div class="text-fg-muted animate-pulse">Loading…</div>
      </div>
    );
  }

  const streamNames = (streams.data ?? []).map((s) => ({
    id: s.id,
    name: s.name,
    status: typeof s.status === 'string' ? s.status : Object.keys(s.status)[0],
  }));

  return (
    <div class="min-h-screen bg-surface">
      <Header
        version={status.data?.version ?? '…'}
        onSettings={() => setShowSettings(true)}
        wsConnected={wsConnected}
      />
      <main class="max-w-7xl mx-auto px-4 sm:px-6 py-6">
        <StatsCards status={status.data} loading={status.loading} />
        <VideoPreview streams={streamNames} />
        <RecordingControls addLog={addLog} />
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
          onAdd={handleAddPlatform}
          onRemove={handleRemovePlatform}
          onUpdate={handleUpdatePlatform}
        />
        <LogViewer logs={logs} onClear={clearLogs} />
      </main>

      {showSettings && (
        <SettingsPanel onClose={() => setShowSettings(false)} addLog={addLog} />
      )}
    </div>
  );
}
