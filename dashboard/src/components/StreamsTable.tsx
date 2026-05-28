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
        return { label: 'Live', cls: 'bg-emerald-900/60 text-emerald-400' };
      case 'Idle':
        return { label: 'Idle', cls: 'bg-slate-800 text-slate-400 border border-slate-700' };
      default:
        return { label: status, cls: 'bg-slate-800 text-slate-400' };
    }
  }
  return { label: `Error: ${status.Error}`, cls: 'bg-red-900/60 text-red-400' };
}

export function StreamsTable({ streams, loading, onRefresh }: Props) {
  return (
    <div class="bg-slate-900 border border-slate-800 rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-slate-800">
        <h2 class="text-base font-semibold">Streams</h2>
        <button
          onClick={onRefresh}
          class="px-3 py-1.5 text-sm rounded-lg bg-slate-800 border border-slate-700 hover:bg-slate-700 transition-colors"
        >
          Refresh
        </button>
      </div>
      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs uppercase tracking-wider text-slate-500">
              <th class="px-5 py-3 border-b border-slate-800">ID</th>
              <th class="px-5 py-3 border-b border-slate-800">Name</th>
              <th class="px-5 py-3 border-b border-slate-800">Input</th>
              <th class="px-5 py-3 border-b border-slate-800">Status</th>
              <th class="px-5 py-3 border-b border-slate-800">Viewers</th>
              <th class="px-5 py-3 border-b border-slate-800">Bitrate</th>
            </tr>
          </thead>
          <tbody>
            {loading && streams.length === 0 ? (
              <tr>
                <td colSpan={6} class="px-5 py-10 text-center text-slate-500">Loading…</td>
              </tr>
            ) : streams.length === 0 ? (
              <tr>
                <td colSpan={6} class="px-5 py-10 text-center text-slate-500">No streams</td>
              </tr>
            ) : (
              streams.map((s) => {
                const badge = statusBadge(s.status);
                return (
                  <tr key={s.id} class="hover:bg-slate-800/50 transition-colors">
                    <td class="px-5 py-3 font-mono text-xs text-slate-400">{s.id.slice(0, 8)}…</td>
                    <td class="px-5 py-3">{s.name}</td>
                    <td class="px-5 py-3 font-mono text-xs text-slate-400">{s.input_url}</td>
                    <td class="px-5 py-3">
                      <span class={`inline-block px-2 py-0.5 rounded text-xs font-semibold ${badge.cls}`}>
                        {badge.label}
                      </span>
                    </td>
                    <td class="px-5 py-3">{s.viewers}</td>
                    <td class="px-5 py-3">{s.bitrate} kbps</td>
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
