import type { DashboardStatus } from '../api';
import { useLocale } from '../hooks/useLocale';
import { Icon, type IconName } from './Icon';
import type { DashboardSection } from './Sidebar';

interface Props {
  section: Exclude<DashboardSection, 'home' | 'past' | 'channels'>;
  status: DashboardStatus | null;
  onPrimary: () => void;
}

const metadata: Record<Props['section'], { icon: IconName; title: string; description: string; action: string }> = {
  clips: { icon: 'clapperboard', title: 'Clips', description: 'Turn your best live moments into shareable clips.', action: 'Create a clip' },
  storage: { icon: 'storage', title: 'Storage', description: 'Keep your recordings, uploads, and reusable media in one place.', action: 'Upload media' },
  analytics: { icon: 'activity', title: 'Analytics', description: 'See how your streams perform across every connected channel.', action: 'Explore analytics' },
};

export function ProductPage({ section, status, onPrimary }: Props) {
  const { t } = useLocale();
  const info = metadata[section];
  return (
    <div class="page-content product-page">
      <div class="page-heading"><div><div class="eyebrow">{info.title}</div><h1>{info.title}</h1><p>{info.description}</p></div><button class="primary-button" onClick={onPrimary}><Icon name={section === 'analytics' ? 'activity' : 'plus'} size={17} />{info.action}</button></div>
      {section === 'analytics' && <div class="analytics-overview"><div class="metric-card"><span>{t('stats.activeStreams')}</span><strong>{status?.activeStreams ?? 0}</strong><small>{t('product.analyticsLive')}</small></div><div class="metric-card"><span>{t('stats.totalViewers')}</span><strong>{status?.totalViewers ?? 0}</strong><small>{t('product.analyticsViewers')}</small></div><div class="metric-card"><span>{t('stats.uptime')}</span><strong>{status ? `${Math.floor(status.uptimeSeconds / 3600)}h` : '—'}</strong><small>{t('product.analyticsUptime')}</small></div></div>}
      <div class="product-empty-card"><div class="product-empty-visual"><div class="product-orbit product-orbit--one" /><div class="product-orbit product-orbit--two" /><div class="product-empty-icon"><Icon name={info.icon} size={30} /></div></div><h2>{t(`product.${section}Empty` as any)}</h2><p>{t(`product.${section}Description` as any)}</p><button class="secondary-button" onClick={onPrimary}><Icon name="sparkle" size={16} />{info.action}</button></div>
    </div>
  );
}
