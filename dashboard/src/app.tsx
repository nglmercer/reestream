import { useCallback, useState, useEffect } from 'preact/hooks';
import { apiV1 } from './api';
import type {
  Channel as V1Channel,
  DashboardStatus,
  Event,
  PlatformCatalogEntry,
  StreamInfo,
} from './api';
import { usePolling, useStreamWs } from './hooks';
import { useLocale } from './hooks/useLocale';
import { useLogger } from './components/LogViewer';
import { Header } from './components/Header';
import { StatsCards } from './components/StatsCards';
import { VideoPreview } from './components/VideoPreview';
import { StreamsTable } from './components/StreamsTable';
import { PlatformsTable, type DashboardChannel, type ChannelUpdate } from './components/PlatformsTable';
import { LogViewer } from './components/LogViewer';
import { SetupWizard } from './components/SetupWizard';
import { SettingsPanel } from './components/SettingsPanel';
import { RecordingControls } from './components/RecordingControls';

const STATUS_POLL = 5_000;
const PLATFORMS_POLL = 15_000;

function eventToStream(event: Event): StreamInfo {
  return {
    id: event.id,
    name: event.title || event.id,
    inputUrl: event.ingest.serverUrl,
    status: event.status === 'live' ? 'Live' : 'Idle',
    startedAt: event.startedAt,
    viewers: event.currentViewers,
    bitrate: 0,
  };
}

function toDashboardChannel(
  channel: V1Channel,
  catalog: PlatformCatalogEntry[],
): DashboardChannel {
  return {
    ...channel,
    platformName: catalog.find((platform) => platform.id === channel.platformId)?.name
      ?? channel.platformId,
    keyConfigured: true,
  };
}

export function App() {
  const { logs, addLog, clearLogs } = useLogger();
  const { t } = useLocale();
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [liveStreams, setLiveStreams] = useState<StreamInfo[]>([]);
  const [platformCatalog, setPlatformCatalog] = useState<PlatformCatalogEntry[]>([]);

  useEffect(() => {
    let active = true;
    apiV1
      .getSetupStatus()
      .then((status) => {
        if (active) setNeedsSetup(status.firstRun);
      })
      .catch(() => setNeedsSetup(false));
    return () => {
      active = false;
    };
  }, []);

  const { connected: wsConnected } = useStreamWs({
    onInit: (events) => {
      setLiveStreams(events.filter((event) => event.status === 'live').map(eventToStream));
    },
    onEvent: (event, eventName) => {
      if (eventName === 'event.started') {
        addLog(t('log.streamStarted', { name: event.title || event.id }));
      } else if (eventName === 'event.ended' || eventName === 'event.cancelled') {
        addLog(t('log.streamEnded'));
      }
      setLiveStreams((prev) => {
        const next = prev.filter((stream) => stream.id !== event.id);
        return event.status === 'live' ? [...next, eventToStream(event)] : next;
      });
    },
  });

  const fetchStreams = useCallback(async (): Promise<StreamInfo[]> => {
    const events = await apiV1.getEvents('live');
    return events.map(eventToStream);
  }, []);

  const fetchStatus = useCallback(async (): Promise<DashboardStatus> => {
    return apiV1.getStatus();
  }, []);

  const fetchPlatforms = useCallback(async (): Promise<DashboardChannel[]> => {
    const [channels, catalog] = await Promise.all([
      apiV1.getChannels(),
      apiV1.getPlatforms(),
    ]);
    setPlatformCatalog(catalog);
    return channels.map((channel) => toDashboardChannel(channel, catalog));
  }, []);

  const status = usePolling(fetchStatus, STATUS_POLL);
  const streamsPoll = usePolling(fetchStreams, 10_000);
  const platforms = usePolling(fetchPlatforms, PLATFORMS_POLL);

  const streams = wsConnected ? { data: liveStreams, loading: false, refresh: streamsPoll.refresh } : streamsPoll;

  const handleToggle = useCallback(
    async (id: string, enabled: boolean) => {
      try {
        await apiV1.updateChannel(id, { enabled });
        addLog(t('log.platformToggled'));
        platforms.refresh();
      } catch (error) {
        addLog(t('log.toggleFailed', { error: error instanceof Error ? error.message : String(error) }), 'error');
      }
    },
    [addLog, platforms],
  );

  const handleAddPlatform = useCallback(
    async (name: string, url: string, key: string) => {
      const catalogEntry = platformCatalog.find((platform) =>
        platform.name.toLowerCase() === name.toLowerCase()
        || platform.slug.toLowerCase() === name.toLowerCase(),
      );
      try {
        await apiV1.createChannel({
          platformId: catalogEntry?.id ?? 'custom-rtmp',
          displayName: name,
          streamUrl: url,
          streamKey: key,
        });
        addLog(t('log.platformAdded', { name }));
        platforms.refresh();
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : t('log.addFailed'));
      }
    },
    [addLog, platformCatalog, platforms],
  );

  const handleRemovePlatform = useCallback(
    async (id: string) => {
      try {
        await apiV1.deleteChannel(id);
        addLog(t('log.platformRemoved'));
        platforms.refresh();
      } catch (error) {
        addLog(t('log.removeFailed', { error: error instanceof Error ? error.message : String(error) }), 'error');
      }
    },
    [addLog, platforms],
  );

  const handleUpdatePlatform = useCallback(
    async (id: string, req: ChannelUpdate) => {
      try {
        await apiV1.updateChannel(id, req);
        addLog(t('log.platformUpdated'));
        platforms.refresh();
      } catch (error) {
        addLog(t('log.updateFailed', { error: error instanceof Error ? error.message : String(error) }), 'error');
      }
    },
    [addLog, platforms],
  );

  if (status.error) addLog(t('log.statusError', { error: status.error }), 'error');
  if (platforms.error) addLog(t('log.platformsError', { error: platforms.error }), 'error');

  if (needsSetup === true) {
    return <SetupWizard />;
  }

  if (needsSetup === null) {
    return (
      <div class="min-h-screen bg-surface flex items-center justify-center">
        <div class="text-fg-muted animate-pulse">{t('common.loading')}</div>
      </div>
    );
  }

  const streamNames = (streams.data ?? []).map((s) => ({
    id: s.id,
    name: s.name,
    status: s.status,
  }));

  return (
    <div class="min-h-screen bg-surface">
      <Header
        version={status.data?.version ?? t('common.fallback')}
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
