import { useMemo, useState } from 'preact/hooks';
import type { Event, EventStatus, Channel } from '../api';
import { useLocale } from '../hooks/useLocale';
import { Icon } from './Icon';

interface Props {
  events: Event[];
  channels: Channel[];
  loading: boolean;
  past?: boolean;
  onOpen: (event: Event) => void;
  onCreate: () => void;
  onDuplicate: (event: Event) => void;
  onDelete: (event: Event) => void;
  onChannels: () => void;
  onRefresh: () => void;
}

type Filter = 'all' | 'draft' | 'scheduled';
type MenuAction = 'titles' | 'schedule' | 'duplicate' | 'channels' | 'settings' | 'delete';

function statusLabel(status: EventStatus, t: (key: any, params?: any) => string): string {
  return t(`home.status.${status}`);
}

function statusClass(status: EventStatus): string {
  switch (status) {
    case 'live': return 'status-pill status-pill--live';
    case 'scheduled': return 'status-pill status-pill--scheduled';
    case 'ended': return 'status-pill status-pill--ended';
    case 'cancelled': return 'status-pill status-pill--cancelled';
    default: return 'status-pill status-pill--draft';
  }
}

function formatEdited(timestamp: number, locale: string): string {
  if (!timestamp) return '—';
  const date = new Date(timestamp * 1000);
  return date.toLocaleDateString(locale === 'es' ? 'es-ES' : 'en-US', {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  });
}

function eventType(event: Event, t: (key: any, params?: any) => string): string {
  if (event.streamType === 'studio') return t('home.type.studio');
  if (event.streamType === 'file' || event.streamType === 'playlist') return t('home.type.video');
  return t('home.type.rtmp');
}

function initials(name: string): string {
  return name.trim().split(/\s+/).slice(0, 2).map((part) => part[0]?.toUpperCase() ?? '').join('') || '?';
}

export function HomePage({ events, channels, loading, past = false, onOpen, onCreate, onDuplicate, onDelete, onChannels, onRefresh }: Props) {
  const { t, locale } = useLocale();
  const [filter, setFilter] = useState<Filter>('all');
  const [openMenu, setOpenMenu] = useState<string | null>(null);

  const visibleEvents = useMemo(() => {
    const source = past ? events.filter((event) => event.status === 'ended' || event.status === 'cancelled') : events.filter((event) => event.status !== 'ended' && event.status !== 'cancelled');
    if (filter === 'all') return source;
    return source.filter((event) => event.status === filter);
  }, [events, filter, past]);

  const runAction = (action: MenuAction, event: Event) => {
    setOpenMenu(null);
    if (action === 'delete') {
      onDelete(event);
    } else if (action === 'duplicate') {
      onDuplicate(event);
    } else if (action === 'channels') {
      onChannels();
    } else {
      onOpen(event);
    }
  };

  return (
    <div class="page-content home-page">
      <div class="announcement-bar">
        <Icon name="sparkle" size={16} />
        <span>{t('home.announcement')}</span>
        <button onClick={onCreate}>{t('home.upgrade')}</button>
      </div>

      <div class="page-heading home-heading">
        <div>
          <div class="eyebrow">{past ? t('nav.past') : t('nav.home')}</div>
          <h1>{past ? t('home.pastTitle') : t('home.title')}</h1>
        </div>
        <button class="primary-button" onClick={onCreate}><Icon name="plus" size={17} />{t('home.newStream')}</button>
      </div>

      <div class="stream-toolbar">
        <div class="segmented-tabs">
          {(['all', 'draft', 'scheduled'] as Filter[]).map((value) => (
            <button key={value} class={filter === value ? 'is-active' : ''} onClick={() => setFilter(value)}>
              {t(`home.tab.${value}`)}
            </button>
          ))}
        </div>
        <div class="toolbar-actions">
          <button class="icon-button" title={t('home.search')}><Icon name="search" size={17} /></button>
          <button class="icon-button" title={t('home.refresh')} onClick={onRefresh}><Icon name="refresh" size={17} /></button>
        </div>
      </div>

      <div class="stream-list-card">
        <div class="stream-list-head">
          <span>{t('home.streamTitle')} <Icon name="layers" size={13} /></span>
          <span>{t('home.statusLabel')} <Icon name="layers" size={13} /></span>
          <span>{t('home.channelsLabel')}</span>
          <span>{t('home.lastEdited')} <Icon name="chevronDown" size={13} /></span>
          <span />
        </div>

        {loading && visibleEvents.length === 0 ? (
          <div class="stream-empty"><div class="loading-orb" /><span>{t('common.loading')}</span></div>
        ) : visibleEvents.length === 0 ? (
          <div class="stream-empty">
            <div class="empty-icon"><Icon name="video" size={26} /></div>
            <strong>{past ? t('home.emptyPast') : t('home.emptyTitle')}</strong>
            <span>{past ? t('home.emptyPastDescription') : t('home.emptyDescription')}</span>
            {!past && <button class="secondary-button" onClick={onCreate}><Icon name="plus" size={16} />{t('home.createFirst')}</button>}
          </div>
        ) : (
          visibleEvents.map((event) => {
            const eventChannels = event.destinationIds.map((id) => channels.find((channel) => channel.id === id)).filter(Boolean) as Channel[];
            return (
              <div key={event.id} class="stream-row" onClick={() => onOpen(event)}>
                <div class="stream-title-cell">
                  <div class="stream-thumb"><Icon name={event.streamType === 'studio' ? 'monitor' : 'video'} size={19} /><span class="thumb-grid"><i /><i /><i /></span></div>
                  <div class="stream-title-copy"><strong>{event.title || t('home.untitled')}</strong><small>{eventType(event, t)}</small></div>
                </div>
                <div><span class={statusClass(event.status)}><i />{statusLabel(event.status, t)}</span></div>
                <div class="channel-stack">
                  {eventChannels.length === 0 ? <span class="no-channels">—</span> : eventChannels.slice(0, 3).map((channel, index) => <span key={channel.id} class={`channel-avatar channel-avatar--${index}`} title={channel.displayName}>{initials(channel.displayName)}</span>)}
                  {eventChannels.length > 3 && <span class="channel-more">+{eventChannels.length - 3}</span>}
                </div>
                <div class="edited-cell">{formatEdited(event.updatedAt, locale)}</div>
                <div class="row-actions" onClick={(clickEvent) => clickEvent.stopPropagation()}>
                  <button class={`more-button ${openMenu === event.id ? 'is-open' : ''}`} onClick={() => setOpenMenu(openMenu === event.id ? null : event.id)} aria-label={t('home.more')}><Icon name="more" size={18} /></button>
                  {openMenu === event.id && (
                    <div class="action-menu">
                      <button onClick={() => runAction('titles', event)}><Icon name="edit" size={15} />{t('home.menu.titles')}</button>
                      <button onClick={() => runAction('schedule', event)}><Icon name="calendar" size={15} />{t('home.menu.schedule')}</button>
                      <button onClick={() => runAction('duplicate', event)}><Icon name="copy" size={15} />{t('home.menu.duplicate')}</button>
                      <button onClick={() => runAction('channels', event)}><Icon name="link" size={15} />{t('home.menu.channels')}</button>
                      <button onClick={() => runAction('settings', event)}><Icon name="settings" size={15} />{t('home.menu.settings')}</button>
                      <div class="action-divider" />
                      <button class="is-danger" onClick={() => runAction('delete', event)}><Icon name="trash" size={15} />{t('home.menu.delete')}</button>
                    </div>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
