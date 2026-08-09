import { useState } from 'preact/hooks';
import type { Channel, StreamType } from '../api';
import { useLocale } from '../hooks/useLocale';
import { Icon } from './Icon';

interface Props {
  channels: Channel[];
  onClose: () => void;
  onCreate: (request: { title: string; streamType: StreamType; destinationIds: string[] }) => Promise<void>;
}

export function CreateStreamDialog({ channels, onClose, onCreate }: Props) {
  const { t } = useLocale();
  const [title, setTitle] = useState('');
  const [streamType, setStreamType] = useState<StreamType>('encoder');
  const [destinationIds, setDestinationIds] = useState<string[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const toggleChannel = (id: string) => {
    setDestinationIds((current) => current.includes(id) ? current.filter((value) => value !== id) : [...current, id]);
  };

  const submit = async (event: Event) => {
    event.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await onCreate({ title: title.trim() || t('home.untitled'), streamType, destinationIds });
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div class="modal-backdrop" onClick={onClose}>
      <form class="modal-card" onSubmit={submit} onClick={(event) => event.stopPropagation()}>
        <div class="modal-header"><h2>{t('home.createTitle')}</h2><button type="button" class="icon-button" onClick={onClose}><Icon name="close" size={18} /></button></div>
        <label class="field-label">{t('home.titleLabel')}<input autoFocus value={title} onInput={(event) => setTitle((event.target as HTMLInputElement).value)} placeholder={t('home.titlePlaceholder')} /></label>
        <label class="field-label">{t('home.typeLabel')}<select value={streamType} onChange={(event) => setStreamType((event.target as HTMLSelectElement).value as StreamType)}><option value="encoder">{t('home.type.rtmp')}</option><option value="studio">{t('home.type.studio')}</option><option value="file">{t('home.type.video')}</option></select></label>
        <div class="field-label">{t('home.pairChannels')}<div class="channel-picker">
          {channels.length === 0 ? <span class="field-help">{t('home.noChannels')}</span> : channels.map((channel) => <button type="button" key={channel.id} class={destinationIds.includes(channel.id) ? 'channel-picker-item is-selected' : 'channel-picker-item'} onClick={() => toggleChannel(channel.id)}><span>{channel.displayName}</span><span class="picker-check">{destinationIds.includes(channel.id) ? '✓' : ''}</span></button>)}
        </div></div>
        {error && <div class="form-error"><Icon name="warning" size={15} />{error}</div>}
        <div class="modal-footer"><button type="button" class="secondary-button" onClick={onClose}>{t('home.cancel')}</button><button type="submit" class="primary-button" disabled={saving}>{saving ? t('home.creating') : t('home.createStream')}</button></div>
      </form>
    </div>
  );
}
