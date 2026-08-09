import { useEffect, useState } from 'preact/hooks';
import type { Channel } from '../api';
import { useLocale } from '../hooks/useLocale';
import { Icon } from './Icon';

export interface ChannelFormRequest {
  platformId: string;
  displayName: string;
  streamUrl: string;
  streamKey: string;
}

export interface ChannelTemplate {
  platformId: string;
  name: string;
  url: string;
  tone: string;
}

export const CHANNEL_TEMPLATES: readonly ChannelTemplate[] = [
  { platformId: 'twitch', name: 'Twitch', url: 'rtmp://live.twitch.tv/app', tone: 'twitch' },
  { platformId: 'facebook', name: 'Facebook', url: 'rtmps://live-api-s.facebook.com:443/rtmp/', tone: 'facebook' },
  { platformId: 'kick', name: 'Kick', url: 'rtmp://fa723fc1b141.global-contribute.live-video.net/app', tone: 'kick' },
  { platformId: 'youtube', name: 'YouTube', url: 'rtmp://a.rtmp.youtube.com/live2', tone: 'youtube' },
  { platformId: 'custom-rtmp', name: 'Custom RTMP', url: '', tone: 'custom' },
];

interface Props {
  channel?: Channel | null;
  onClose: () => void;
  onSave: (request: ChannelFormRequest, channelId?: string) => Promise<void>;
  onRemove?: (id: string) => Promise<void>;
}

function templateFor(platformId: string | undefined): ChannelTemplate | undefined {
  return CHANNEL_TEMPLATES.find((template) => template.platformId === platformId);
}

function initials(name: string): string {
  return name.trim().split(/\s+/).slice(0, 2).map((part) => part[0]?.toUpperCase() ?? '').join('') || '?';
}

export function ChannelConfigModal({ channel = null, onClose, onSave, onRemove }: Props) {
  const { t } = useLocale();
  const [platformId, setPlatformId] = useState(channel?.platformId ?? CHANNEL_TEMPLATES[0].platformId);
  const [displayName, setDisplayName] = useState(channel?.displayName ?? CHANNEL_TEMPLATES[0].name);
  const [streamUrl, setStreamUrl] = useState(channel?.streamUrl ?? CHANNEL_TEMPLATES[0].url);
  const [streamKey, setStreamKey] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && !saving && !removing) onClose();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose, removing, saving]);

  useEffect(() => {
    const nextTemplate = templateFor(channel?.platformId) ?? CHANNEL_TEMPLATES[0];
    setPlatformId(channel?.platformId ?? nextTemplate.platformId);
    setDisplayName(channel?.displayName ?? nextTemplate.name);
    setStreamUrl(channel?.streamUrl ?? nextTemplate.url);
    setStreamKey('');
    setShowKey(false);
    setError(null);
  }, [channel]);

  const selectTemplate = (template: ChannelTemplate) => {
    const previousTemplate = templateFor(platformId);
    setPlatformId(template.platformId);
    setDisplayName((current) => !current.trim() || current === previousTemplate?.name ? template.name : current);
    setStreamUrl(template.url);
    setError(null);
  };

  const submit = async (event: Event) => {
    event.preventDefault();
    const name = displayName.trim();
    const url = streamUrl.trim();
    const key = streamKey.trim();
    if (!name || !url || (!channel && !key)) {
      setError(t('channels.requiredFields'));
      return;
    }
    try {
      const parsedUrl = new URL(url);
      if (parsedUrl.protocol !== 'rtmp:' && parsedUrl.protocol !== 'rtmps:') throw new Error('unsupported protocol');
    } catch {
      setError(t('channels.invalidUrl'));
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await onSave({ platformId, displayName: name, streamUrl: url, streamKey: key }, channel?.id);
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    if (!channel || !onRemove || removing) return;
    if (!window.confirm(t('platforms.confirmRemove', { name: channel.displayName }))) return;
    setRemoving(true);
    setError(null);
    try {
      await onRemove(channel.id);
      onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setRemoving(false);
    }
  };

  const selectedTemplate = templateFor(platformId);

  return (
    <div class="modal-backdrop channel-config-backdrop" role="presentation" onMouseDown={(event) => event.currentTarget === event.target && !saving && !removing && onClose()}>
      <div class="modal-card channel-config-modal" role="dialog" aria-modal="true" aria-labelledby="channel-config-title">
        <div class="modal-header channel-config-header">
          <div>
            <h2 id="channel-config-title">{channel ? t('channels.modalEditTitle') : t('channels.modalTitle')}</h2>
            <p>{t('channels.modalDescription')}</p>
          </div>
          <button type="button" class="icon-button" onClick={onClose} disabled={saving || removing} aria-label={t('channels.close')}><Icon name="close" size={17} /></button>
        </div>

        {!channel ? <section class="channel-template-section">
          <div class="channel-config-section-heading"><strong>{t('channels.chooseTemplate')}</strong><span>{t('channels.templatesDescription')}</span></div>
          <div class="channel-template-grid">
            {CHANNEL_TEMPLATES.map((template) => <button key={template.platformId} type="button" class={`channel-template channel-template--${template.tone} ${platformId === template.platformId ? 'is-selected' : ''}`} onClick={() => selectTemplate(template)}>
              <span class="channel-template-mark">{initials(template.name)}</span>
              <span><strong>{template.name}</strong><small>{template.platformId === 'custom-rtmp' ? t('channels.templateCustomHint') : t('channels.templateManagedHint')}</small></span>
              {platformId === template.platformId && <Icon name="check" size={15} />}
            </button>)}
          </div>
        </section> : <div class="channel-config-current"><span class={`channel-template-mark channel-template-mark--${selectedTemplate?.tone ?? 'custom'}`}>{initials(channel.displayName)}</span><div><strong>{selectedTemplate?.name ?? channel.platformId}</strong><small>{t('channels.configureExisting')}</small></div></div>}

        <form onSubmit={submit}>
          <label class="field-label">{t('channels.destinationName')}<input value={displayName} onInput={(event) => setDisplayName((event.target as HTMLInputElement).value)} placeholder={t('platforms.placeholder.name')} autoFocus /></label>
          <label class="field-label">{t('channels.streamUrl')}<input value={streamUrl} onInput={(event) => setStreamUrl((event.target as HTMLInputElement).value)} placeholder={t('platforms.placeholder.url')} /></label>
          <label class="field-label"><span>{t('channels.streamKey')} {channel && <small class="field-label-optional">({t('channels.optional')})</small>}</span><div class="secret-input"><input type={showKey ? 'text' : 'password'} value={streamKey} onInput={(event) => setStreamKey((event.target as HTMLInputElement).value)} placeholder={channel ? '••••••••' : t('platforms.placeholder.key')} /><button type="button" onClick={() => setShowKey((current) => !current)} aria-label={showKey ? t('channels.hideKey') : t('channels.showKey')}><Icon name="monitor" size={15} /></button></div><small class="field-help">{channel ? t('channels.streamKeyKeep') : t('channels.streamKeyHelp')}</small></label>
          {error && <div class="form-error"><Icon name="warning" size={15} />{error}</div>}
          <div class="modal-footer channel-config-footer">
            {channel && onRemove ? <button type="button" class="danger-button" onClick={remove} disabled={saving || removing}><Icon name="trash" size={15} />{removing ? t('platforms.removing') : t('channels.remove')}</button> : <span />}
            <div class="form-actions"><button type="button" class="secondary-button" onClick={onClose} disabled={saving || removing}>{t('channels.close')}</button><button type="submit" class="primary-button" disabled={saving || removing}>{saving ? t('channels.saving') : t('channels.save')}</button></div>
          </div>
        </form>
      </div>
    </div>
  );
}
