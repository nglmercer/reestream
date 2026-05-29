import type { StreamInfo, StreamStatus } from '../api';
import { useLocale } from '../hooks/useLocale';

interface Props {
  streams: StreamInfo[];
  loading: boolean;
  onRefresh: () => void;
}

export function StreamsTable({ streams, loading, onRefresh }: Props) {
  const { t } = useLocale();

  function statusBadge(status: StreamStatus): { label: string; cls: string } {
    if (typeof status === 'string') {
      switch (status) {
        case 'Live':
          return { label: t('streams.status.live'), cls: 'bg-success-bg text-success' };
        case 'Idle':
          return { label: t('streams.status.idle'), cls: 'bg-surface-hover text-fg-muted border border-border' };
        default:
          return { label: status, cls: 'bg-surface-hover text-fg-muted' };
      }
    }
    return { label: t('streams.status.error', { message: status.Error }), cls: 'bg-danger-bg text-danger' };
  }

  return (
    <div class="bg-surface-alt border border-border rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-border">
        <h2 class="text-base font-semibold text-fg">{t('streams.title')}</h2>
        <button
          onClick={onRefresh}
          class="px-3 py-1.5 text-sm rounded-lg bg-surface-hover border border-border hover:bg-surface-active transition-colors text-fg-secondary"
        >
          {t('streams.refresh')}
        </button>
      </div>
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs uppercase tracking-wider text-fg-faint">
              <th class="px-5 py-3 border-b border-border">{t('streams.column.id')}</th>
              <th class="px-5 py-3 border-b border-border">{t('streams.column.name')}</th>
              <th class="px-5 py-3 border-b border-border">{t('streams.column.input')}</th>
              <th class="px-5 py-3 border-b border-border">{t('streams.column.status')}</th>
              <th class="px-5 py-3 border-b border-border">{t('streams.column.viewers')}</th>
              <th class="px-5 py-3 border-b border-border">{t('streams.column.bitrate')}</th>
            </tr>
          </thead>
          <tbody>
            {loading && streams.length === 0 ? (
              <tr>
                <td colSpan={6} class="px-5 py-10 text-center text-fg-faint">{t('streams.loading')}</td>
              </tr>
            ) : streams.length === 0 ? (
              <tr>
                <td colSpan={6} class="px-5 py-10 text-center text-fg-faint">{t('streams.empty')}</td>
              </tr>
            ) : (
              streams.map((s) => {
                const badge = statusBadge(s.status);
                return (
                  <tr key={s.id} class="hover:bg-surface-hover transition-colors">
                    <td class="px-5 py-3 font-mono text-xs text-fg-muted">{s.id.slice(0, 8)}…</td>
                    <td class="px-5 py-3 text-fg">{s.name}</td>
                    <td class="px-5 py-3 font-mono text-xs text-fg-muted">{s.input_url}</td>
                    <td class="px-5 py-3">
                      <span class={`inline-block px-2 py-0.5 rounded text-xs font-semibold ${badge.cls}`}>
                        {badge.label}
                      </span>
                    </td>
                    <td class="px-5 py-3 text-fg">{s.viewers}</td>
                    <td class="px-5 py-3 text-fg">{s.bitrate} {t('streams.bitrateUnit')}</td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
