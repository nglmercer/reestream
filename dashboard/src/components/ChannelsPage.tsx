import { useState } from 'preact/hooks';
import type { DashboardChannel, ChannelUpdate } from './PlatformsTable';
import { useLocale } from '../hooks/useLocale';
import { Icon } from './Icon';
import { ChannelConfigModal, type ChannelFormRequest } from './ChannelConfigModal';

interface Props {
  channels: DashboardChannel[];
  loading: boolean;
  onRefresh: () => void;
  onAdd: (request: ChannelFormRequest) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onUpdate: (id: string, request: ChannelUpdate) => Promise<void>;
  onToggle: (id: string, enabled: boolean) => void;
}

function initials(name: string) {
  return name.trim().split(/\s+/).slice(0, 2).map((part) => part[0]?.toUpperCase() ?? '').join('') || '?';
}

export function ChannelsPage({ channels, loading, onRefresh, onAdd, onRemove, onUpdate, onToggle }: Props) {
  const { t } = useLocale();
  const [modalChannel, setModalChannel] = useState<DashboardChannel | null | undefined>(undefined);

  const saveChannel = async (request: ChannelFormRequest, channelId?: string) => {
    if (channelId) {
      await onUpdate(channelId, {
        displayName: request.displayName,
        streamUrl: request.streamUrl,
        ...(request.streamKey ? { streamKey: request.streamKey } : {}),
      });
      return;
    }
    await onAdd(request);
  };

  return (
    <div class="page-content channels-page">
      <div class="page-heading"><div><h1>{t('channels.title')}</h1></div><div class="heading-actions"><button class="icon-button" onClick={onRefresh} title={t('home.refresh')}><Icon name="refresh" size={17} /></button><button class="primary-button" onClick={() => setModalChannel(null)}><Icon name="plus" size={17} />{t('channels.add')}</button></div></div>
      <div class="channel-page-toolbar"><span>{channels.length} {t('channels.connected')}</span></div>
      {loading && channels.length === 0 ? <div class="channel-grid-loading">{t('common.loading')}</div> : channels.length === 0 ? <div class="large-empty-state"><h2>{t('channels.empty')}</h2><p>{t('channels.emptyDescription')}</p><button class="secondary-button" onClick={() => setModalChannel(null)}><Icon name="plus" size={16} />{t('channels.add')}</button></div> : <div class="channel-card-grid">{channels.map((channel) => <div class="channel-card" key={channel.id}>
        <div class="channel-card-head"><span class="channel-avatar channel-avatar--0">{initials(channel.displayName)}</span><div class="channel-card-name"><strong>{channel.displayName}</strong><small>{channel.platformName}</small></div><button class="more-button" onClick={() => setModalChannel(channel)} aria-label={t('channels.edit')}><Icon name="edit" size={16} /></button></div>
        <div class="channel-card-url"><Icon name="link" size={14} />{channel.streamUrl}</div>
        <div class="channel-card-footer"><span class={channel.enabled ? 'connection-state is-connected' : 'connection-state'}><i />{channel.enabled ? t('channels.enabled') : t('channels.disabled')}</span><div class="channel-card-actions"><button onClick={() => onToggle(channel.id, !channel.enabled)}>{channel.enabled ? t('channels.disable') : t('channels.enable')}</button><button class="danger-link" onClick={() => { if (confirm(t('platforms.confirmRemove', { name: channel.displayName }))) onRemove(channel.id); }}>{t('platforms.remove')}</button></div></div>
        {channel.lastError && <div class="channel-error"><Icon name="warning" size={14} />{channel.lastError}</div>}
      </div>)}</div>}
      {modalChannel !== undefined && <ChannelConfigModal channel={modalChannel} onClose={() => setModalChannel(undefined)} onSave={saveChannel} onRemove={onRemove} />}
    </div>
  );
}
