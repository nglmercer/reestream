import { useEffect, useMemo, useState } from 'preact/hooks';
import type { Channel, Event } from '../api';
import { apiV1 } from '../api';
import { useLocale } from '../hooks/useLocale';
import { useVideoPlayer } from '../hooks/useVideoPlayer';
import { CollapsiblePanel } from './CollapsiblePanel';
import { Icon } from './Icon';
import { ChannelConfigModal, type ChannelFormRequest } from './ChannelConfigModal';

interface Props {
  event: Event;
  channels: Channel[];
  onBack: () => void;
  onAddChannel: (request: ChannelFormRequest) => Promise<void>;
  onUpdateChannel: (id: string, request: ChannelFormRequest) => Promise<void>;
  onRemoveChannel: (id: string) => Promise<void>;
  onUpdate: (event: Event) => void;
}

type Protocol = 'rtmp' | 'rtmps' | 'srt';

interface EventCredentials {
  serverUrl: string;
  streamKey: string;
  backupServerUrl: string | null;
}

interface SrtCredentials {
  primary: { url: string; passphrase: string | null } | null;
  backup: { url: string; passphrase: string | null } | null;
}

interface ProtocolOption {
  id: Protocol;
  label: string;
  url: string;
  key: string | null;
}

function LiveVideo() {
  const { videoRef, error, playing, toggle } = useVideoPlayer({ url: '/stream.flv', autoplay: true, muted: true, lowLatency: true });
  const { t } = useLocale();
  return (
    <div class="live-video-wrap">
      <video ref={videoRef} class="live-video" muted playsinline onClick={toggle} />
      <div class="video-live-badge"><span />{t('detail.live')}</div>
      {!playing && !error && <button class="video-play-button" onClick={toggle}><Icon name="play" size={24} /></button>}
      {error && <div class="video-error"><Icon name="warning" size={18} />{error}</div>}
    </div>
  );
}

function maskKey(value: string | null): string {
  if (!value) return '••••••••••••••••••••••••••••••';
  return `••••••••••••${value.slice(-5)}`;
}

export function StreamDetail({ event, channels, onBack, onAddChannel, onUpdateChannel, onRemoveChannel, onUpdate }: Props) {
  const { t } = useLocale();
  const [credentials, setCredentials] = useState<EventCredentials | null>(null);
  const [srtCredentials, setSrtCredentials] = useState<SrtCredentials | null>(null);
  const [protocol, setProtocol] = useState<Protocol>('rtmp');
  const [streamKey, setStreamKey] = useState<string | null>(null);
  const [showKey, setShowKey] = useState(false);
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const [recordingBusy, setRecordingBusy] = useState(false);
  const [recordingError, setRecordingError] = useState<string | null>(null);
  const [title, setTitle] = useState(event.title);
  const [savingTitle, setSavingTitle] = useState(false);
  const [channelModal, setChannelModal] = useState<Channel | null | undefined>(undefined);

  useEffect(() => {
    setTitle(event.title);
  }, [event.id, event.title]);

  useEffect(() => {
    let active = true;
    setCredentials(null);
    setSrtCredentials(null);
    setStreamKey(null);
    setRecordingId(null);
    setRecordingError(null);

    Promise.all([
      apiV1.getEventStreamKey(event.id),
      apiV1.getEventSrtKeys(event.id).catch(() => null),
      apiV1.getRecordings(event.id).catch(() => null),
    ]).then(([eventCredentials, srt, recordings]) => {
      if (!active) return;
      setCredentials(eventCredentials);
      setStreamKey(eventCredentials.streamKey);
      setSrtCredentials(srt);
      setRecordingId(recordings?.active?.id ?? null);
    }).catch((cause) => {
      if (active) setRecordingError(cause instanceof Error ? cause.message : String(cause));
    });

    return () => { active = false; };
  }, [event.id]);

  const selectedChannels = useMemo(() => new Set(event.destinationIds), [event.destinationIds]);
  const protocolOptions = useMemo<ProtocolOption[]>(() => {
    const options: ProtocolOption[] = [];
    const primaryUrl = credentials?.serverUrl || event.ingest.serverUrl;
    const key = credentials?.streamKey || streamKey || null;

    if (primaryUrl.startsWith('rtmp://') && key) {
      options.push({ id: 'rtmp', label: 'RTMP', url: primaryUrl, key });
    }
    if (primaryUrl.startsWith('rtmps://') && key) {
      options.push({ id: 'rtmps', label: 'RTMPS', url: primaryUrl, key });
    }
    if (credentials?.backupServerUrl?.startsWith('rtmps://') && key) {
      options.push({ id: 'rtmps', label: 'RTMPS', url: credentials.backupServerUrl, key });
    }
    if (srtCredentials?.primary?.url) {
      options.push({
        id: 'srt',
        label: 'SRT',
        url: srtCredentials.primary.url,
        key: srtCredentials.primary.passphrase,
      });
    }
    return options.filter((option, index, all) => all.findIndex((candidate) => candidate.id === option.id) === index);
  }, [credentials, event.ingest.serverUrl, srtCredentials, streamKey]);

  useEffect(() => {
    if (protocolOptions.length > 0 && !protocolOptions.some((option) => option.id === protocol)) {
      setProtocol(protocolOptions[0].id);
    }
  }, [protocol, protocolOptions]);

  const activeProtocol = protocolOptions.find((option) => option.id === protocol) ?? protocolOptions[0] ?? null;
  const isLive = event.status === 'live';

  const saveTitle = async () => {
    const nextTitle = title.trim();
    if (!nextTitle || nextTitle === event.title || savingTitle) return;
    setSavingTitle(true);
    try {
      onUpdate(await apiV1.updateEvent(event.id, { title: nextTitle }));
    } finally {
      setSavingTitle(false);
    }
  };

  const toggleRecording = async () => {
    if (!isLive || recordingBusy) return;
    setRecordingBusy(true);
    setRecordingError(null);
    try {
      if (recordingId) {
        await apiV1.stopEventRecording(event.id);
        setRecordingId(null);
      } else {
        const result = await apiV1.startEventRecording(event.id);
        if (!result.recordingId) throw new Error(t('detail.recordingMissingId'));
        setRecordingId(result.recordingId);
      }
    } catch (cause) {
      setRecordingError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setRecordingBusy(false);
    }
  };

  const toggleChannel = async (channel: Channel) => {
    const updated = selectedChannels.has(channel.id)
      ? await apiV1.removeEventDestination(event.id, channel.id)
      : await apiV1.addEventDestination(event.id, channel.id);
    onUpdate(updated);
  };

  const saveChannel = async (request: ChannelFormRequest, channelId?: string) => {
    if (channelId) {
      await onUpdateChannel(channelId, request);
    } else {
      await onAddChannel(request);
    }
  };

  return (
    <div class="detail-page">
      <div class="detail-topbar">
        <div class="detail-titlebar">
          <button class="detail-back" onClick={onBack} aria-label={t('detail.back')}><Icon name="arrowLeft" size={19} /></button>
          <input class="detail-title-input" value={title || t('home.untitled')} onInput={(inputEvent) => setTitle((inputEvent.target as HTMLInputElement).value)} onBlur={saveTitle} onKeyDown={(keyboardEvent) => keyboardEvent.key === 'Enter' && (keyboardEvent.target as HTMLInputElement).blur()} />
          {savingTitle && <span class="saving-dot" />}
        </div>
        <div class="detail-top-actions">
          <label class={`record-toggle ${!isLive ? 'is-disabled' : ''}`} title={!isLive ? t('detail.recordOnlyLive') : undefined}>
            <input type="checkbox" checked={!!recordingId} disabled={!isLive || recordingBusy} onChange={toggleRecording} />
            <span class="toggle-track"><i /></span><span>{recordingBusy ? t('detail.recordingBusy') : t('detail.record')}</span>
          </label>
          {recordingError && <span class="detail-inline-error" title={recordingError}>{t('detail.recordingError')}</span>}
        </div>
      </div>

      <div class="detail-layout">
        <section class="encoder-card">
          <div class="encoder-stage">
            {isLive ? <LiveVideo /> : <div class="offline-state"><span class="offline-badge">{t('detail.offline')}</span><h2>{t('detail.connectEncoder')}</h2><p>{t('detail.connectDescription')}</p></div>}
          </div>
          <CollapsiblePanel
            title={t('detail.connectionSettings')}
            summary={activeProtocol?.label ?? t('detail.loadingCredentials')}
            className="encoder-settings-panel"
          >
            <div class="encoder-controls">
              {protocolOptions.length > 1 && <div class="protocol-tabs">
                {protocolOptions.map((option) => <button key={option.id} class={activeProtocol?.id === option.id ? 'is-active' : ''} onClick={() => { setProtocol(option.id); setShowKey(false); }}>{option.label}</button>)}
              </div>}
              {activeProtocol ? <div class="credential-grid">
                <div class="credential-field"><label>{activeProtocol.id === 'srt' ? t('detail.srtUrl') : activeProtocol.id === 'rtmps' ? t('detail.rtmpsUrl') : t('detail.serverUrl')}</label><div class="credential-value"><span>{activeProtocol.url}</span><button onClick={() => navigator.clipboard?.writeText(activeProtocol.url)}><Icon name="copy" size={16} /></button></div></div>
                <div class="credential-field"><label>{activeProtocol.id === 'srt' ? t('detail.passphrase') : t('detail.streamKey')}</label><div class="credential-value"><span>{activeProtocol.key ? showKey ? activeProtocol.key : maskKey(activeProtocol.key) : '—'}</span>{activeProtocol.key && <><button onClick={() => setShowKey(!showKey)}><Icon name={showKey ? 'close' : 'monitor'} size={16} /></button><button onClick={() => navigator.clipboard?.writeText(activeProtocol.key!)}><Icon name="copy" size={16} /></button></>}</div></div>
              </div> : <div class="credential-empty">{t('detail.noCredentials')}</div>}
            </div>
          </CollapsiblePanel>
        </section>

        <aside class="detail-channel-panel">
          <CollapsiblePanel title={t('detail.yourChannels')} summary={`${event.destinationIds.length} ${t('detail.paired')}`} className="channel-settings-panel">
            <div class="channel-panel-actions"><button class="panel-action-button" onClick={() => setChannelModal(null)}><Icon name="plus" size={16} />{t('detail.addChannel')}</button></div>
            <div class="channel-count"><span>{channels.filter((channel) => channel.enabled).length} {t('detail.active')}</span></div>
            <div class="detail-channel-list">
              {channels.length === 0 ? <div class="panel-empty"><span>{t('home.noChannels')}</span><button onClick={() => setChannelModal(null)}>{t('detail.addChannel')}</button></div> : channels.map((channel) => {
                const paired = selectedChannels.has(channel.id);
                return <div key={channel.id} class="detail-channel-row"><span class="channel-avatar channel-avatar--0">{channel.displayName.slice(0, 1).toUpperCase()}</span><div class="detail-channel-copy"><strong>{channel.displayName}</strong><small>{channel.status || t('detail.noConnection')}</small>{channel.lastError && <em>{channel.lastError}</em>}</div><div class="detail-channel-actions"><button class="detail-channel-edit" onClick={() => setChannelModal(channel)} aria-label={t('channels.edit')}><Icon name="edit" size={14} /></button><button class={`channel-toggle ${paired ? 'is-on' : ''}`} onClick={() => toggleChannel(channel).catch(() => {})} aria-label={paired ? t('detail.on') : t('detail.off')}><span /></button></div></div>;
              })}
            </div>
          </CollapsiblePanel>
        </aside>
      </div>
      {channelModal !== undefined && <ChannelConfigModal channel={channelModal} onClose={() => setChannelModal(undefined)} onSave={saveChannel} onRemove={onRemoveChannel} />}
    </div>
  );
}
