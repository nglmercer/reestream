import { useLocale } from '../hooks/useLocale';
import { Icon, type IconName } from './Icon';

export type DashboardSection = 'home' | 'past' | 'channels';

interface Props {
  active: DashboardSection;
  collapsed?: boolean;
  channelWarning?: boolean;
  onNavigate: (section: DashboardSection) => void;
  onSettings: () => void;
}

const items: Array<{ id: DashboardSection; icon: IconName; label: 'home' | 'past' | 'channels' }> = [
  { id: 'home', icon: 'home', label: 'home' },
  { id: 'past', icon: 'refresh', label: 'past' },
  { id: 'channels', icon: 'radio', label: 'channels' },
];

export function Sidebar({ active, collapsed = false, channelWarning = false, onNavigate, onSettings }: Props) {
  const { t } = useLocale();

  return (
    <aside class={`product-sidebar ${collapsed ? 'product-sidebar--collapsed' : ''}`}>
      <div class="sidebar-brand">
        <div class="brand-mark"><Icon name="sparkle" size={18} strokeWidth={1.9} /></div>
        {!collapsed && <span class="brand-wordmark">Reestream</span>}
      </div>

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
      </div>
    </aside>
  );
}
