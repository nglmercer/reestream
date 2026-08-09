import { useLocale } from '../hooks/useLocale';
import { Icon, type IconName } from './Icon';

export type DashboardSection = 'home' | 'past' | 'clips' | 'storage' | 'channels' | 'analytics';

interface Props {
  active: DashboardSection;
  collapsed?: boolean;
  channelWarning?: boolean;
  onNavigate: (section: DashboardSection) => void;
  onSettings: () => void;
}

const items: Array<{ id: DashboardSection; icon: IconName; label: 'home' | 'past' | 'clips' | 'storage' | 'channels' | 'analytics' }> = [
  { id: 'home', icon: 'home', label: 'home' },
  { id: 'past', icon: 'refresh', label: 'past' },
  { id: 'clips', icon: 'clapperboard', label: 'clips' },
  { id: 'storage', icon: 'storage', label: 'storage' },
  { id: 'channels', icon: 'radio', label: 'channels' },
  { id: 'analytics', icon: 'activity', label: 'analytics' },
];

export function Sidebar({ active, collapsed = false, channelWarning = false, onNavigate, onSettings }: Props) {
  const { t } = useLocale();

  return (
    <aside class={`product-sidebar ${collapsed ? 'product-sidebar--collapsed' : ''}`}>
      <div class="sidebar-brand">
        <div class="brand-mark"><Icon name="sparkle" size={18} strokeWidth={1.9} /></div>
        {!collapsed && <span class="brand-wordmark">Reestream</span>}
        {!collapsed && <button class="sidebar-collapse" aria-label="Collapse sidebar"><Icon name="chevronRight" size={15} /></button>}
      </div>

      {!collapsed && (
        <>
          <button class="profile-switcher">
            <span class="profile-avatar">A</span>
            <span class="profile-copy"><strong>Reestream</strong><small>{t('nav.freePlan')}</small></span>
            <Icon name="chevronDown" size={15} />
          </button>
          <button class="invite-button"><Icon name="users" size={16} />{t('nav.invite')}</button>
          <div class="sidebar-section-label"><span>{t('nav.workspaces')}</span><Icon name="plus" size={15} /></div>
          <button class="workspace-switcher"><span class="workspace-badge">D</span><span>Default</span><Icon name="chevronDown" size={15} /></button>
        </>
      )}

      <nav class="sidebar-nav" aria-label="Primary navigation">
        {items.map((item) => (
          <button
            key={item.id}
            class={`sidebar-nav-item ${active === item.id ? 'is-active' : ''}`}
            onClick={() => onNavigate(item.id)}
            title={collapsed ? t(`nav.${item.label}`) : undefined}
          >
            <Icon name={item.icon} size={18} />
            {!collapsed && <span>{t(`nav.${item.label}`)}</span>}
            {!collapsed && item.id === 'channels' && channelWarning && <span class="nav-alert"><Icon name="warning" size={11} /></span>}
          </button>
        ))}
      </nav>

      <div class="sidebar-bottom">
        <button class="sidebar-nav-item" onClick={onSettings} title={collapsed ? t('nav.settings') : undefined}>
          <Icon name="settings" size={18} />
          {!collapsed && <span>{t('nav.settings')}</span>}
        </button>
        {!collapsed && <div class="sidebar-help"><Icon name="circleHelp" size={16} /><span>{t('nav.help')}</span><Icon name="external" size={13} /></div>}
      </div>
    </aside>
  );
}
