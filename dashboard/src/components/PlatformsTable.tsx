import type { Platform } from '../api';

interface Props {
  platforms: Platform[];
  loading: boolean;
  onRefresh: () => void;
  onToggle: (id: string) => void;
}

export function PlatformsTable({ platforms, loading, onRefresh, onToggle }: Props) {
  return (
    <div class="bg-slate-900 border border-slate-800 rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-slate-800">
        <h2 class="text-base font-semibold">Platforms</h2>
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
              <th class="px-5 py-3 border-b border-slate-800">URL</th>
              <th class="px-5 py-3 border-b border-slate-800">Enabled</th>
              <th class="px-5 py-3 border-b border-slate-800">Actions</th>
            </tr>
          </thead>
          <tbody>
            {loading && platforms.length === 0 ? (
              <tr>
                <td colSpan={5} class="px-5 py-10 text-center text-slate-500">Loading…</td>
              </tr>
            ) : platforms.length === 0 ? (
              <tr>
                <td colSpan={5} class="px-5 py-10 text-center text-slate-500">No platforms</td>
              </tr>
            ) : (
              platforms.map((p) => (
                <tr key={p.id} class="hover:bg-slate-800/50 transition-colors">
                  <td class="px-5 py-3 font-mono text-xs text-slate-400">{p.id.slice(0, 8)}…</td>
                  <td class="px-5 py-3">{p.name}</td>
                  <td class="px-5 py-3 font-mono text-xs text-slate-400">{p.url}</td>
                  <td class="px-5 py-3">
                    <span
                      class={`inline-block px-2 py-0.5 rounded text-xs font-semibold ${
                        p.enabled
                          ? 'bg-emerald-900/60 text-emerald-400'
                          : 'bg-slate-800 text-slate-400 border border-slate-700'
                      }`}
                    >
                      {p.enabled ? 'Yes' : 'No'}
                    </span>
                  </td>
                  <td class="px-5 py-3">
                    <button
                      onClick={() => onToggle(p.id)}
                      class="px-3 py-1 text-xs rounded bg-sky-600 hover:bg-sky-500 text-white transition-colors"
                    >
                      Toggle
                    </button>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
