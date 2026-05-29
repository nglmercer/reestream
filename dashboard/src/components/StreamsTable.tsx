import type { StreamInfo, StreamStatus } from '../api';

interface Props {
  streams: StreamInfo[];
  loading: boolean;
  onRefresh: () => void;
}

function statusBadge(status: StreamStatus): { label: string; cls: string } {
  if (typeof status === 'string') {
    switch (status) {
      case 'Live':
        return { label: 'Live', cls: 'bg-success-bg text-success' };
      case 'Idle':
        return { label: 'Idle', cls: 'bg-surface-hover text-fg-muted border border-border' };
      default:
        return { label: status, cls: 'bg-surface-hover text-fg-muted' };
    }
  }
  return { label: `Error: ${status.Error}`, cls: 'bg-danger-bg text-danger' };
}

export function StreamsTable({ streams, loading, onRefresh }: Props) {
  return (
    <div class="bg-surface-alt border border-border rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-border">
        <h2 class="text-base font-semibold text-fg">Streams</h2>
        <button
          onClick={onRefresh}
          class="px-3 py-1.5 text-sm rounded-lg bg-surface-hover border border-border hover:bg-surface-active transition-colors text-fg-secondary"
        >
          Refresh
        </button>
      </div>
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs uppercase tracking-wider text-fg-faint">
              <th class="px-5 py-3 border-b border-border">ID</th>
              <th class="px-5 py-3 border-b border-border">Name</th>
              <th class="px-5 py-3 border-b border-border">Input</th>
              <th class="px-5 py-3 border-b border-border">Status</th>
              <th class="px-5 py-3 border-b border-border">Viewers</th>
              <th class="px-5 py-3 border-b border-border">Bitrate</th>
            </tr>
          </thead>
          <tbody>
            {loading && streams.length === 0 ? (
              <tr>
                <td colSpan={6} class="px-5 py-10 text-center text-fg-faint">Loading…</td>
              </tr>
            ) : streams.length === 0 ? (
              <tr>
                <td colSpan={6} class="px-5 py-10 text-center text-fg-faint">No streams</td>
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
                    <td class="px-5 py-3 text-fg">{s.bitrate} kbps</td>
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
