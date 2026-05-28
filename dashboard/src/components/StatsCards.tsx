import type { ServerStatus } from '../api';

interface Props {
  status: ServerStatus | null;
  loading: boolean;
}

function formatUptime(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${h}h ${m}m`;
}

export function StatsCards({ status, loading }: Props) {
  if (loading && !status) {
    return (
      <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
        {Array.from({ length: 4 }, (_, i) => (
          <div key={i} class="bg-slate-900 border border-slate-800 rounded-xl p-5 animate-pulse">
            <div class="h-3 w-20 bg-slate-700 rounded mb-3" />
            <div class="h-8 w-16 bg-slate-700 rounded" />
          </div>
        ))}
      </div>
    );
  }

  const cards = [
    { label: 'Uptime', value: status ? formatUptime(status.uptime_seconds) : '--' },
    { label: 'Active Streams', value: status ? String(status.active_streams) : '0' },
    { label: 'Total Viewers', value: status ? String(status.total_viewers) : '0' },
    {
      label: 'Status',
      value: status ? 'Online' : '--',
      color: status ? 'text-emerald-400' : 'text-slate-500',
    },
  ];

  return (
    <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4 mb-6">
      {cards.map((c) => (
        <div key={c.label} class="bg-slate-900 border border-slate-800 rounded-xl p-5">
          <div class="text-xs uppercase tracking-wider text-slate-500 mb-1">{c.label}</div>
          <div class={`text-3xl font-bold ${c.color ?? 'text-sky-400'}`}>{c.value}</div>
        </div>
      ))}
    </div>
  );
}
