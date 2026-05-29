import type { ServerStatus } from '../api';
import { useLocale } from '../hooks/useLocale';
import type { TranslationKey } from '../i18n';

interface Props {
  status: ServerStatus | null;
  loading: boolean;
}

function formatUptime(secs: number, t: (k: TranslationKey, p?: Record<string, string | number>) => string): string {
  if (secs < 60) return t('time.seconds', { n: secs });
  if (secs < 3600) return t('time.minutesSeconds', { m: Math.floor(secs / 60), s: secs % 60 });
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return t('time.hoursMinutes', { h, m });
}

export function StatsCards({ status, loading }: Props) {
  const { t } = useLocale();

  if (loading && !status) {
    return (
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        {Array.from({ length: 4 }, (_, i) => (
          <div key={i} class="bg-surface-alt border border-border rounded-xl p-5 animate-pulse">
            <div class="h-3 w-20 bg-surface-hover rounded mb-3" />
            <div class="h-8 w-16 bg-surface-hover rounded" />
          </div>
        ))}
      </div>
    );
  }

  const cards = [
    { label: t('stats.uptime'), value: status ? formatUptime(status.uptime_seconds, t) : t('stats.fallback') },
    { label: t('stats.activeStreams'), value: status ? String(status.active_streams) : '0' },
    { label: t('stats.totalViewers'), value: status ? String(status.total_viewers) : '0' },
    {
      label: t('stats.status'),
      value: status ? t('stats.online') : t('stats.fallback'),
      color: status ? 'text-success' : 'text-fg-faint',
    },
  ];

  return (
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      {cards.map((c) => (
        <div key={c.label} class="bg-surface-alt border border-border rounded-xl p-5">
          <div class="text-xs uppercase tracking-wider text-fg-faint mb-1">{c.label}</div>
          <div class={`text-3xl font-bold ${c.color ?? 'text-accent'}`}>{c.value}</div>
        </div>
      ))}
    </div>
  );
}
