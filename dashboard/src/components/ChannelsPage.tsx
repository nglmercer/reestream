import { useState } from 'preact/hooks';
import type { DashboardChannel, ChannelUpdate } from './PlatformsTable';
import { useLocale } from '../hooks/useLocale';
import { Icon } from './Icon';

interface Props {
  channels: DashboardChannel[];
  loading: boolean;
  onRefresh: () => void;
  onAdd: (name: string, url: string, key: string) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onUpdate: (id: string, request: ChannelUpdate) => Promise<void>;
  onToggle: (id: string, enabled: boolean) => void;
}

function initials(name: string) {
  return name.trim().split(/\s+/).slice(0, 2).map((part) => part[0]?.toUpperCase() ?? '').join('') || '?';
}

export function ChannelsPage({ channels, loading, onRefresh, onAdd, onRemove, onUpdate, onToggle }: Props) {
  const { t } = useLocale();
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [key, setKey] = useState('');
  const [saving, setSaving] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editUrl, setEditUrl] = useState('');
  const [editKey, setEditKey] = useState('');

  const resetForm = () => { setAdding(false); setName(''); setUrl(''); setKey(''); };
  const submit = async (event: Event) => {
    event.preventDefault();
    if (!name.trim() || !url.trim() || !key.trim()) return;
    setSaving(true);
    try { await onAdd(name.trim(), url.trim(), key.trim()); resetForm(); } finally { setSaving(false); }
  };
  const startEdit = (channel: DashboardChannel) => { setEditing(channel.id); setEditName(channel.displayName); setEditUrl(channel.streamUrl); setEditKey(''); };
  const saveEdit = async (id: string) => { await onUpdate(id, { displayName: editName.trim(), streamUrl: editUrl.trim(), ...(editKey ? { streamKey: editKey } : {}) }); setEditing(null); };

  return (
    <div class="page-content channels-page">
      <div class="page-heading"><div><h1>{t('channels.title')}</h1></div><div class="heading-actions"><button class="icon-button" onClick={onRefresh} title={t('home.refresh')}><Icon name="refresh" size={17} /></button><button class="primary-button" onClick={() => setAdding(!adding)}><Icon name="plus" size={17} />{t('channels.add')}</button></div></div>
      {adding && <form class="channel-create-card" onSubmit={submit}><div class="channel-create-title"><h2>{t('channels.connect')}</h2><button type="button" class="icon-button" onClick={resetForm}><Icon name="close" size={17} /></button></div><div class="form-grid"><label class="field-label">{t('platforms.column.name')}<input value={name} onInput={(event) => setName((event.target as HTMLInputElement).value)} placeholder={t('platforms.placeholder.name')} /></label><label class="field-label">{t('platforms.column.url')}<input value={url} onInput={(event) => setUrl((event.target as HTMLInputElement).value)} placeholder={t('platforms.placeholder.url')} /></label><label class="field-label">{t('platforms.column.key')}<input type="password" value={key} onInput={(event) => setKey((event.target as HTMLInputElement).value)} placeholder={t('platforms.placeholder.key')} /></label></div><div class="form-actions"><button type="button" class="secondary-button" onClick={resetForm}>{t('home.cancel')}</button><button type="submit" class="primary-button" disabled={saving}>{saving ? t('channels.saving') : t('channels.connect')}</button></div></form>}
      <div class="channel-page-toolbar"><span>{channels.length} {t('channels.connected')}</span></div>
      {loading && channels.length === 0 ? <div class="channel-grid-loading">{t('common.loading')}</div> : channels.length === 0 ? <div class="large-empty-state"><h2>{t('channels.empty')}</h2><p>{t('channels.emptyDescription')}</p><button class="secondary-button" onClick={() => setAdding(true)}><Icon name="plus" size={16} />{t('channels.add')}</button></div> : <div class="channel-card-grid">{channels.map((channel) => editing === channel.id ? <div class="channel-card channel-card--editing" key={channel.id}><div class="channel-edit-head"><strong>{t('channels.edit')}</strong></div><label class="field-label">{t('platforms.column.name')}<input value={editName} onInput={(event) => setEditName((event.target as HTMLInputElement).value)} /></label><label class="field-label">{t('platforms.column.url')}<input value={editUrl} onInput={(event) => setEditUrl((event.target as HTMLInputElement).value)} /></label><label class="field-label">{t('platforms.column.key')}<input type="password" value={editKey} onInput={(event) => setEditKey((event.target as HTMLInputElement).value)} placeholder="••••••••" /></label><div class="form-actions"><button class="secondary-button" onClick={() => setEditing(null)}>{t('home.cancel')}</button><button class="primary-button" onClick={() => saveEdit(channel.id)}>{t('platforms.save')}</button></div></div> : <div class="channel-card" key={channel.id}><div class="channel-card-head"><span class="channel-avatar channel-avatar--0">{initials(channel.displayName)}</span><div class="channel-card-name"><strong>{channel.displayName}</strong><small>{channel.platformId}</small></div><button class="more-button" onClick={() => startEdit(channel)}><Icon name="edit" size={16} /></button></div><div class="channel-card-url"><Icon name="link" size={14} />{channel.streamUrl}</div><div class="channel-card-footer"><span class={channel.enabled ? 'connection-state is-connected' : 'connection-state'}><i />{channel.enabled ? t('channels.enabled') : t('channels.disabled')}</span><div class="channel-card-actions"><button onClick={() => onToggle(channel.id, !channel.enabled)}>{channel.enabled ? t('channels.disable') : t('channels.enable')}</button><button class="danger-link" onClick={() => { if (confirm(t('platforms.confirmRemove', { name: channel.displayName }))) onRemove(channel.id); }}>{t('platforms.remove')}</button></div></div>{channel.lastError && <div class="channel-error"><Icon name="warning" size={14} />{channel.lastError}</div>}</div>)}</div>}
    </div>
  );
}
