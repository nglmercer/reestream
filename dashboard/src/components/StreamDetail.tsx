import { useEffect, useMemo, useState } from 'preact/hooks';
import type { Channel, Event } from '../api';
import { apiV1 } from '../api';
import { useLocale } from '../hooks/useLocale';
import { useVideoPlayer } from '../hooks/useVideoPlayer';
import { Icon } from './Icon';

interface Props {
  event: Event;
  channels: Channel[];
  onBack: () => void;
  onChannels: () => void;
  onUpdate: (event: Event) => void;
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

export function StreamDetail({ event, channels, onBack, onChannels, onUpdate }: Props) {
  const { t } = useLocale();
  const [streamKey, setStreamKey] = useState<string | null>(null);
  const [showKey, setShowKey] = useState(false);
  const [recordingId, setRecordingId] = useState<string | null>(null);
  const [title, setTitle] = useState(event.title);
  const [savingTitle, setSavingTitle] = useState(false);

  useEffect(() => {
    setTitle(event.title);
    apiV1.getEventStreamKey(event.id).then((credentials) => setStreamKey(credentials.streamKey)).catch(() => {});
  }, [event.id, event.title]);

  const selectedChannels = useMemo(() => new Set(event.destinationIds), [event.destinationIds]);
  const endpoint = event.ingest.serverUrl || 'rtmp://localhost:1935/live';
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
    if (recordingId) {
      await apiV1.stopManualRecording(recordingId);
      setRecordingId(null);
    } else {
      const result = await apiV1.startManualRecording(event.id, 'http://127.0.0.1:8080/stream.flv');
      setRecordingId(result.id);
    }
  };

  const toggleChannel = async (channel: Channel) => {
    const updated = selectedChannels.has(channel.id)
      ? await apiV1.removeEventDestination(event.id, channel.id)
      : await apiV1.addEventDestination(event.id, channel.id);
    onUpdate(updated);
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
          <label class="record-toggle"><input type="checkbox" checked={!!recordingId} onChange={() => toggleRecording().catch(() => {})} /><span class="toggle-track"><i /></span><span>{t('detail.record')}</span></label>
        </div>
      </div>

      <div class="detail-layout">
        <section class="encoder-card">
          <div class="encoder-stage">
            {isLive ? <LiveVideo /> : <div class="offline-state"><span class="offline-badge">{t('detail.offline')}</span><h2>{t('detail.connectEncoder')}</h2><p>{t('detail.connectDescription')}</p></div>}
          </div>
          <div class="encoder-controls">
            <div class="credential-grid">
              <div class="credential-field"><label>{t('detail.serverUrl')}</label><div class="credential-value"><span>{endpoint}</span><button onClick={() => navigator.clipboard?.writeText(endpoint)}><Icon name="copy" size={16} /></button></div></div>
              <div class="credential-field"><label>{t('detail.streamKey')}</label><div class="credential-value"><span>{showKey ? streamKey ?? maskKey(null) : maskKey(streamKey)}</span><button onClick={() => setShowKey(!showKey)}><Icon name={showKey ? 'close' : 'monitor'} size={16} /></button><button onClick={() => streamKey && navigator.clipboard?.writeText(streamKey)}><Icon name="copy" size={16} /></button></div></div>
            </div>
          </div>
        </section>

        <aside class="detail-channel-panel">
          <div class="channel-panel-head"><div><h2>{t('detail.yourChannels')}</h2><span>{event.destinationIds.length} {t('detail.paired')}</span></div></div>
          <div class="channel-panel-actions"><button class="panel-action-button" onClick={onChannels}><Icon name="plus" size={16} />{t('detail.addChannel')}</button></div>
          <div class="channel-count"><span>{channels.filter((channel) => channel.enabled).length} {t('detail.active')}</span></div>
          <div class="detail-channel-list">
            {channels.length === 0 ? <div class="panel-empty"><span>{t('home.noChannels')}</span><button onClick={onChannels}>{t('detail.addChannel')}</button></div> : channels.map((channel) => {
              const paired = selectedChannels.has(channel.id);
              return <div key={channel.id} class="detail-channel-row"><span class="channel-avatar channel-avatar--0">{channel.displayName.slice(0, 1).toUpperCase()}</span><div class="detail-channel-copy"><strong>{channel.displayName}</strong><small>{channel.status || t('detail.noConnection')}</small>{channel.lastError && <em>{channel.lastError}</em>}</div><button class={`channel-toggle ${paired ? 'is-on' : ''}`} onClick={() => toggleChannel(channel).catch(() => {})}><span /></button></div>;
            })}
          </div>
        </aside>
      </div>
    </div>
  );
}
