import { useState, useCallback } from 'preact/hooks';
import type { Platform } from '../api';

interface Props {
  platforms: Platform[];
  loading: boolean;
  onRefresh: () => void;
  onToggle: (id: string) => void;
  onAdd: (name: string, url: string, key: string) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
}

const PRESETS: Array<{ name: string; url: string }> = [
  { name: 'Twitch', url: 'rtmp://live.twitch.tv/app' },
  { name: 'YouTube', url: 'rtmp://a.rtmp.youtube.com/live2' },
  { name: 'Facebook', url: 'rtmps://live-api-s.facebook.com:443/rtmp/' },
  { name: 'Instagram', url: 'rtmps://edge-upload.instagram.com:443/rtmp/' },
  { name: 'Kick', url: 'rtmp://fa723fc1b141.global-contribute.live-video.net/app' },
  { name: 'TikTok', url: 'rtmp://push.tiktok.com/live/' },
];

export function PlatformsTable({ platforms, loading, onRefresh, onToggle, onAdd, onRemove }: Props) {
  const [showAdd, setShowAdd] = useState(false);
  const [addName, setAddName] = useState('');
  const [addUrl, setAddUrl] = useState('');
  const [addKey, setAddKey] = useState('');
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);

  const handlePreset = useCallback((preset: (typeof PRESETS)[number]) => {
    setAddName(preset.name);
    setAddUrl(preset.url);
  }, []);

  const handleAdd = useCallback(async () => {
    if (!addName || !addUrl || !addKey) return;
    setAdding(true);
    try {
      await onAdd(addName, addUrl, addKey);
      setAddName('');
      setAddUrl('');
      setAddKey('');
      setShowAdd(false);
    } finally {
      setAdding(false);
    }
  }, [addName, addUrl, addKey, onAdd]);

  const handleRemove = useCallback(
    async (id: string, name: string) => {
      if (!confirm(`Remove platform "${name}"?`)) return;
      setRemoving(id);
      try {
        await onRemove(id);
      } finally {
        setRemoving(null);
      }
    },
    [onRemove],
  );

  return (
    <div class="bg-slate-900 border border-slate-800 rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-slate-800">
        <h2 class="text-base font-semibold">Platforms</h2>
        <div class="flex items-center gap-2">
          <button
            onClick={onRefresh}
            class="px-3 py-1.5 text-sm rounded-lg bg-slate-800 border border-slate-700 hover:bg-slate-700 transition-colors"
          >
            Refresh
          </button>
          <button
            onClick={() => setShowAdd(!showAdd)}
            class="px-3 py-1.5 text-sm rounded-lg bg-sky-600 hover:bg-sky-500 text-white transition-colors"
          >
            {showAdd ? 'Cancel' : '+ Add Platform'}
          </button>
        </div>
      </div>

      {/* Add form */}
      {showAdd && (
        <div class="px-5 py-4 border-b border-slate-800 bg-slate-900/50">
          <div class="flex flex-wrap gap-2 mb-3">
            {PRESETS.map((p) => (
              <button
                key={p.name}
                onClick={() => handlePreset(p)}
                class={`px-2.5 py-1 text-xs rounded-lg border transition-colors ${
                  addName === p.name
                    ? 'bg-sky-600 border-sky-500 text-white'
                    : 'bg-slate-800 border-slate-700 hover:border-sky-500 text-slate-300'
                }`}
              >
                {p.name}
              </button>
            ))}
          </div>
          <div class="grid grid-cols-1 sm:grid-cols-3 gap-3 mb-3">
            <input
              value={addName}
              onInput={(e) => setAddName((e.target as HTMLInputElement).value)}
              placeholder="Name"
              class="bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-sky-500"
            />
            <input
              value={addUrl}
              onInput={(e) => setAddUrl((e.target as HTMLInputElement).value)}
              placeholder="rtmp://server/app"
              class="bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-sky-500"
            />
            <input
              value={addKey}
              onInput={(e) => setAddKey((e.target as HTMLInputElement).value)}
              placeholder="Stream key"
              type="password"
              class="bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-sky-500"
            />
          </div>
          <button
            onClick={handleAdd}
            disabled={adding || !addName || !addUrl || !addKey}
            class="px-4 py-2 text-sm rounded-lg bg-emerald-600 hover:bg-emerald-500 disabled:bg-slate-700 disabled:text-slate-500 text-white transition-colors"
          >
            {adding ? 'Adding…' : 'Add Platform'}
          </button>
        </div>
      )}

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
                <td colSpan={5} class="px-5 py-10 text-center text-slate-500">
                  No platforms. Click "+ Add Platform" to add one.
                </td>
              </tr>
            ) : (
              platforms.map((p) => (
                <tr key={p.id} class="hover:bg-slate-800/50 transition-colors">
                  <td class="px-5 py-3 font-mono text-xs text-slate-400">{p.id.slice(0, 8)}…</td>
                  <td class="px-5 py-3">{p.name}</td>
                  <td class="px-5 py-3 font-mono text-xs text-slate-400">{p.url}</td>
                  <td class="px-5 py-3">
                    <button
                      onClick={() => onToggle(p.id)}
                      class={`inline-block px-2 py-0.5 rounded text-xs font-semibold cursor-pointer transition-colors ${
                        p.enabled
                          ? 'bg-emerald-900/60 text-emerald-400 hover:bg-emerald-900/80'
                          : 'bg-slate-800 text-slate-400 border border-slate-700 hover:bg-slate-700'
                      }`}
                    >
                      {p.enabled ? 'Yes' : 'No'}
                    </button>
                  </td>
                  <td class="px-5 py-3">
                    <button
                      onClick={() => handleRemove(p.id, p.name)}
                      disabled={removing === p.id}
                      class="px-3 py-1 text-xs rounded bg-red-900/30 border border-red-800/50 text-red-400 hover:bg-red-900/50 disabled:opacity-50 transition-colors"
                    >
                      {removing === p.id ? '…' : 'Remove'}
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
