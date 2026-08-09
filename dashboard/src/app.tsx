import { useCallback, useEffect, useMemo, useState } from 'preact/hooks';
import { apiV1 } from './api';
import type { Channel as V1Channel, Event, PlatformCatalogEntry, StreamType } from './api';
import { usePolling, useStreamWs } from './hooks';
import { useLocale } from './hooks/useLocale';
import { useLogger } from './components/LogViewer';
import { Sidebar, type DashboardSection } from './components/Sidebar';
import { HomePage } from './components/HomePage';
import { CreateStreamDialog } from './components/CreateStreamDialog';
import { StreamDetail } from './components/StreamDetail';
import { ChannelsPage } from './components/ChannelsPage';
import { SetupWizard } from './components/SetupWizard';
import { SettingsPanel } from './components/SettingsPanel';
import type { ChannelUpdate, DashboardChannel } from './components/PlatformsTable';

const EVENTS_POLL = 10_000;
const CHANNELS_POLL = 15_000;
const DASHBOARD_SECTIONS: readonly DashboardSection[] = ['home', 'past', 'channels'];

function routeFromLocation(): { section: DashboardSection; eventId: string | null } {
  const path = window.location.pathname.replace(/\/+$/, '') || '/home';
  const eventMatch = path.match(/^\/shows\/([^/]+)$/);
  if (eventMatch) {
    return {
      section: 'home',
      eventId: decodeURIComponent(eventMatch[1]),
    };
  }

  const requestedSection = path.slice(1) as DashboardSection;
  return {
    section: DASHBOARD_SECTIONS.includes(requestedSection) ? requestedSection : 'home',
    eventId: null,
  };
}

function pathForRoute(section: DashboardSection, eventId: string | null): string {
  return eventId ? `/shows/${encodeURIComponent(eventId)}` : `/${section}`;
}

function toDashboardChannel(channel: V1Channel, catalog: PlatformCatalogEntry[]): DashboardChannel {
  return {
    ...channel,
    platformName: catalog.find((platform) => platform.id === channel.platformId)?.name ?? channel.platformId,
    keyConfigured: true,
  };
}

export function App() {
  const { t } = useLocale();
  const { addLog } = useLogger();
  const [initialRoute] = useState(routeFromLocation);
  const [needsSetup, setNeedsSetup] = useState<boolean | null>(null);
  const [section, setSection] = useState<DashboardSection>(initialRoute.section);
  const [selectedEventId, setSelectedEventId] = useState<string | null>(initialRoute.eventId);
  const [showCreate, setShowCreate] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [realtimeEvents, setRealtimeEvents] = useState<Event[]>([]);
  const [wsInitialized, setWsInitialized] = useState(false);
  const [platformCatalog, setPlatformCatalog] = useState<PlatformCatalogEntry[]>([]);

  const setRoute = useCallback((nextSection: DashboardSection, eventId: string | null = null) => {
    const nextPath = pathForRoute(nextSection, eventId);
    if (window.location.pathname !== nextPath) {
      window.history.pushState({}, '', nextPath);
    }
    setSection(nextSection);
    setSelectedEventId(eventId);
  }, []);

  const navigate = useCallback((next: DashboardSection) => {
    setRoute(next);
  }, [setRoute]);

  const openEvent = useCallback((eventId: string) => {
    setRoute('home', eventId);
  }, [setRoute]);

  useEffect(() => {
    const handlePopState = () => {
      const route = routeFromLocation();
      setSection(route.section);
      setSelectedEventId(route.eventId);
    };
    window.addEventListener('popstate', handlePopState);
    return () => window.removeEventListener('popstate', handlePopState);
  }, []);

  useEffect(() => {
    let active = true;
    apiV1.getSetupStatus().then((status) => {
      if (active) setNeedsSetup(status.firstRun);
    }).catch(() => {
      if (active) setNeedsSetup(false);
    });
    return () => { active = false; };
  }, []);

  const eventsPoll = usePolling(() => apiV1.getEvents(), EVENTS_POLL);
  const channelsPoll = usePolling(async () => {
    const [channels, catalog] = await Promise.all([apiV1.getChannels(), apiV1.getPlatforms()]);
    setPlatformCatalog(catalog);
    return channels.map((channel) => toDashboardChannel(channel, catalog));
  }, CHANNELS_POLL);
  const { connected: wsConnected } = useStreamWs({
    onInit: (events) => {
      setRealtimeEvents(events);
      setWsInitialized(true);
    },
    onEvent: (event, eventName) => {
      if (eventName === 'event.started') addLog(t('log.streamStarted', { name: event.title || event.id }));
      if (eventName === 'event.ended' || eventName === 'event.cancelled') addLog(t('log.streamEnded'));
      setRealtimeEvents((current) => {
        const withoutEvent = current.filter((item) => item.id !== event.id);
        return event.status === 'ended' || event.status === 'cancelled' ? withoutEvent : [...withoutEvent, event];
      });
    },
  });

  const events = wsConnected && wsInitialized ? realtimeEvents : eventsPoll.data ?? [];
  const channels = channelsPoll.data ?? [];
  const selectedEvent = selectedEventId ? events.find((event) => event.id === selectedEventId) ?? null : null;

  const refreshEvents = useCallback(() => eventsPoll.refresh(), [eventsPoll.refresh]);
  const refreshChannels = useCallback(() => channelsPoll.refresh(), [channelsPoll.refresh]);

  const updateRealtimeEvent = useCallback((event: Event) => {
    setRealtimeEvents((current) => [...current.filter((item) => item.id !== event.id), event]);
    refreshEvents();
  }, [refreshEvents]);

  const createEvent = useCallback(async (request: { title: string; streamType: StreamType; destinationIds: string[] }) => {
    const event = await apiV1.createEvent(request);
    setRealtimeEvents((current) => [...current, event]);
    refreshEvents();
    openEvent(event.id);
  }, [openEvent, refreshEvents]);

  const duplicateEvent = useCallback(async (event: Event) => {
    try {
      const duplicate = await apiV1.createEvent({
        title: `${event.title || t('home.untitled')} ${t('home.copySuffix')}`,
        description: event.description,
        streamType: event.streamType,
        destinationIds: event.destinationIds,
        loopsCount: event.loopsCount,
      });
      setRealtimeEvents((current) => [...current, duplicate]);
      refreshEvents();
    } catch (cause) {
      window.alert(cause instanceof Error ? cause.message : String(cause));
    }
  }, [refreshEvents, t]);

  const deleteEvent = useCallback(async (event: Event) => {
    if (!window.confirm(t('home.confirmDelete', { title: event.title || t('home.untitled') }))) return;
    try {
      await apiV1.deleteEvent(event.id);
      setRealtimeEvents((current) => current.filter((item) => item.id !== event.id));
      if (selectedEventId === event.id) navigate('home');
      refreshEvents();
    } catch (cause) {
      window.alert(cause instanceof Error ? cause.message : String(cause));
    }
  }, [navigate, refreshEvents, selectedEventId, t]);

  const addChannel = useCallback(async (name: string, url: string, key: string) => {
    const catalogEntry = platformCatalog.find((platform) => platform.name.toLowerCase() === name.toLowerCase() || platform.slug.toLowerCase() === name.toLowerCase());
    await apiV1.createChannel({ platformId: catalogEntry?.id ?? 'custom-rtmp', displayName: name, streamUrl: url, streamKey: key });
    refreshChannels();
  }, [platformCatalog, refreshChannels]);

  const removeChannel = useCallback(async (id: string) => {
    await apiV1.deleteChannel(id);
    refreshChannels();
  }, [refreshChannels]);

  const updateChannel = useCallback(async (id: string, request: ChannelUpdate) => {
    await apiV1.updateChannel(id, request);
    refreshChannels();
  }, [refreshChannels]);

  const toggleChannel = useCallback(async (id: string, enabled: boolean) => {
    try {
      await apiV1.updateChannel(id, { enabled });
      refreshChannels();
    } catch (cause) {
      window.alert(cause instanceof Error ? cause.message : String(cause));
    }
  }, [refreshChannels]);

  const channelWarning = channels.some((channel) => !!channel.lastError);
  const page = useMemo(() => {
    if (selectedEvent) {
      return <StreamDetail event={selectedEvent} channels={channels} onBack={() => navigate('home')} onChannels={() => navigate('channels')} onUpdate={updateRealtimeEvent} />;
    }
    if (section === 'home' || section === 'past') {
      return <HomePage events={events} channels={channels} loading={eventsPoll.loading} past={section === 'past'} onOpen={(event) => openEvent(event.id)} onCreate={() => setShowCreate(true)} onDuplicate={duplicateEvent} onDelete={deleteEvent} onChannels={() => navigate('channels')} onRefresh={refreshEvents} />;
    }
    if (section === 'channels') {
      return <ChannelsPage channels={channels} loading={channelsPoll.loading} onRefresh={refreshChannels} onAdd={addChannel} onRemove={removeChannel} onUpdate={updateChannel} onToggle={toggleChannel} />;
    }
    return null;
  }, [selectedEvent, section, events, channels, eventsPoll.loading, channelsPoll.loading, addChannel, removeChannel, updateChannel, toggleChannel, duplicateEvent, deleteEvent, updateRealtimeEvent, navigate, openEvent, refreshEvents]);

  if (needsSetup === true) return <SetupWizard />;
  if (needsSetup === null) return <div class="boot-screen"><div class="brand-mark"><span>✦</span></div><span>{t('common.loading')}</span></div>;

  return (
    <div class="dashboard-app">
      <Sidebar active={section} collapsed={!!selectedEvent} channelWarning={channelWarning} onNavigate={navigate} onSettings={() => setShowSettings(true)} />
      <main class={`dashboard-main ${selectedEvent ? 'dashboard-main--detail' : ''}`}>
        {page}
      </main>
      {showCreate && <CreateStreamDialog channels={channels} onClose={() => setShowCreate(false)} onCreate={createEvent} />}
      {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} addLog={addLog} />}
    </div>
  );
}
