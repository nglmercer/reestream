import { useState, useCallback } from 'preact/hooks';
import type { Channel } from '../api';
import { useLocale } from '../hooks/useLocale';

export interface DashboardChannel extends Channel {
  platformName: string;
  keyConfigured: boolean;
}

export type ChannelUpdate = {
  displayName?: string;
  streamUrl?: string;
  streamKey?: string;
  enabled?: boolean;
};

interface Props {
  platforms: DashboardChannel[];
  loading: boolean;
  onRefresh: () => void;
  onToggle: (id: string, enabled: boolean) => void;
  onAdd: (name: string, url: string, key: string) => Promise<void>;
  onRemove: (id: string) => Promise<void>;
  onUpdate: (id: string, req: ChannelUpdate) => Promise<void>;
}

const PRESETS: Array<{ name: string; url: string }> = [
  { name: 'Twitch', url: 'rtmp://live.twitch.tv/app' },
  { name: 'YouTube', url: 'rtmp://a.rtmp.youtube.com/live2' },
  { name: 'Facebook', url: 'rtmps://live-api-s.facebook.com:443/rtmp/' },
  { name: 'Instagram', url: 'rtmps://edge-upload.instagram.com:443/rtmp/' },
  { name: 'Kick', url: 'rtmp://fa723fc1b141.global-contribute.live-video.net/app' },
  { name: 'TikTok', url: 'rtmp://push.tiktok.com/live/' },
];

export function PlatformsTable({ platforms, loading, onRefresh, onToggle, onAdd, onRemove, onUpdate }: Props) {
  const { t } = useLocale();
  const [showAdd, setShowAdd] = useState(false);
  const [addName, setAddName] = useState('');
  const [addUrl, setAddUrl] = useState('');
  const [addKey, setAddKey] = useState('');
  const [adding, setAdding] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);

  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState('');
  const [editUrl, setEditUrl] = useState('');
  const [editKey, setEditKey] = useState('');
  const [editEnabled, setEditEnabled] = useState(true);
  const [saving, setSaving] = useState(false);

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
      if (!confirm(t('platforms.confirmRemove', { name }))) return;
      setRemoving(id);
      try {
        await onRemove(id);
      } finally {
        setRemoving(null);
      }
    },
    [onRemove],
  );

  const startEdit = useCallback((p: DashboardChannel) => {
    setEditingId(p.id);
    setEditName(p.displayName);
    setEditUrl(p.streamUrl);
    setEditKey('');
    setEditEnabled(p.enabled);
  }, []);

  const cancelEdit = useCallback(() => {
    setEditingId(null);
    setEditName('');
    setEditUrl('');
    setEditKey('');
  }, []);

  const handleSave = useCallback(async () => {
    if (!editingId) return;
    setSaving(true);
    try {
      await onUpdate(editingId, {
        displayName: editName,
        streamUrl: editUrl,
        ...(editKey ? { streamKey: editKey } : {}),
        enabled: editEnabled,
      });
      cancelEdit();
    } finally {
      setSaving(false);
    }
  }, [editingId, editName, editUrl, editKey, editEnabled, onUpdate, cancelEdit]);

  return (
    <div class="bg-surface-alt border border-border rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-border">
        <h2 class="text-base font-semibold text-fg">{t('platforms.title')}</h2>
        <div class="flex items-center gap-2">
          <button
            onClick={onRefresh}
            class="px-3 py-1.5 text-sm rounded-lg bg-surface-hover border border-border hover:bg-surface-active transition-colors text-fg-secondary"
          >
            {t('platforms.refresh')}
          </button>
          <button
            onClick={() => setShowAdd(!showAdd)}
            class="px-3 py-1.5 text-sm rounded-lg bg-accent hover:bg-accent-hover text-white transition-colors"
          >
            {showAdd ? t('platforms.cancel') : t('platforms.add')}
          </button>
        </div>
      </div>

      {showAdd && (
        <div class="px-5 py-4 border-b border-border bg-surface-alt">
          <div class="flex flex-wrap gap-2 mb-3">
            {PRESETS.map((p) => (
              <button
                key={p.name}
                onClick={() => handlePreset(p)}
                class={`px-2.5 py-1 text-xs rounded-lg border transition-colors ${
                  addName === p.name
                    ? 'bg-accent border-accent text-white'
                    : 'bg-surface-hover border-border hover:border-accent text-fg-secondary'
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
              placeholder={t('platforms.placeholder.name')}
              class="bg-surface-input border border-border rounded-lg px-3 py-2 text-sm text-fg focus:outline-none focus:border-accent"
            />
            <input
              value={addUrl}
              onInput={(e) => setAddUrl((e.target as HTMLInputElement).value)}
              placeholder={t('platforms.placeholder.url')}
              class="bg-surface-input border border-border rounded-lg px-3 py-2 text-sm text-fg focus:outline-none focus:border-accent"
            />
            <input
              value={addKey}
              onInput={(e) => setAddKey((e.target as HTMLInputElement).value)}
              placeholder={t('platforms.placeholder.key')}
              type="password"
              class="bg-surface-input border border-border rounded-lg px-3 py-2 text-sm text-fg focus:outline-none focus:border-accent"
            />
          </div>
          <button
            onClick={handleAdd}
            disabled={adding || !addName || !addUrl || !addKey}
            class="px-4 py-2 text-sm rounded-lg bg-success hover:opacity-90 disabled:bg-surface-active disabled:text-fg-faint text-white transition-colors"
          >
            {adding ? t('platforms.adding') : t('platforms.add')}
          </button>
        </div>
      )}

      <div class="overflow-x-auto">
        <table class="w-full text-sm">
          <thead>
            <tr class="text-left text-xs uppercase tracking-wider text-fg-faint">
              <th class="px-5 py-3 border-b border-border">{t('platforms.column.id')}</th>
              <th class="px-5 py-3 border-b border-border">{t('platforms.column.name')}</th>
              <th class="px-5 py-3 border-b border-border">{t('platforms.column.url')}</th>
              <th class="px-5 py-3 border-b border-border">{t('platforms.column.key')}</th>
              <th class="px-5 py-3 border-b border-border">{t('platforms.column.enabled')}</th>
              <th class="px-5 py-3 border-b border-border">{t('platforms.column.actions')}</th>
            </tr>
          </thead>
          <tbody>
            {loading && platforms.length === 0 ? (
              <tr>
                <td colSpan={6} class="px-5 py-10 text-center text-fg-faint">{t('platforms.loading')}</td>
              </tr>
            ) : platforms.length === 0 ? (
              <tr>
                <td colSpan={6} class="px-5 py-10 text-center text-fg-faint">
                  {t('platforms.empty')}
                </td>
              </tr>
            ) : (
              platforms.map((p) =>
                editingId === p.id ? (
                  <tr key={p.id} class="bg-surface-hover">
                    <td class="px-5 py-2 font-mono text-xs text-fg-muted">{p.id.slice(0, 8)}…</td>
                    <td class="px-5 py-2">
                      <input
                        value={editName}
                        onInput={(e) => setEditName((e.target as HTMLInputElement).value)}
                        class="w-full bg-surface-active border border-border-strong rounded px-2 py-1 text-sm text-fg focus:outline-none focus:border-accent"
                      />
                    </td>
                    <td class="px-5 py-2">
                      <input
                        value={editUrl}
                        onInput={(e) => setEditUrl((e.target as HTMLInputElement).value)}
                        class="w-full bg-surface-active border border-border-strong rounded px-2 py-1 text-sm text-fg focus:outline-none focus:border-accent"
                      />
                    </td>
                    <td class="px-5 py-2">
                      <input
                        value={editKey}
                        onInput={(e) => setEditKey((e.target as HTMLInputElement).value)}
                        type="password"
                        class="w-full bg-surface-active border border-border-strong rounded px-2 py-1 text-sm text-fg focus:outline-none focus:border-accent"
                      />
                    </td>
                    <td class="px-5 py-2">
                      <button
                        onClick={() => setEditEnabled(!editEnabled)}
                        class={`px-2 py-0.5 rounded text-xs font-semibold cursor-pointer transition-colors ${
                          editEnabled
                            ? 'bg-success-bg text-success'
                            : 'bg-surface-active text-fg-muted border border-border-strong'
                        }`}
                      >
                        {editEnabled ? t('platforms.yes') : t('platforms.no')}
                      </button>
                    </td>
                    <td class="px-5 py-2">
                      <div class="flex items-center gap-2">
                        <button
                          onClick={handleSave}
                          disabled={saving}
                          class="px-3 py-1 text-xs rounded bg-success hover:opacity-90 disabled:bg-surface-active text-white transition-colors"
                        >
                          {saving ? t('platforms.saving') : t('platforms.save')}
                        </button>
                        <button
                          onClick={cancelEdit}
                          class="px-3 py-1 text-xs rounded bg-surface-hover hover:bg-surface-active border border-border text-fg-secondary transition-colors"
                        >
                          {t('platforms.cancel')}
                        </button>
                      </div>
                    </td>
                  </tr>
                ) : (
                  <tr key={p.id} class="hover:bg-surface-hover transition-colors">
                    <td class="px-5 py-3 font-mono text-xs text-fg-muted">{p.id.slice(0, 8)}…</td>
                    <td class="px-5 py-3 text-fg">{p.displayName}</td>
                    <td class="px-5 py-3 font-mono text-xs text-fg-muted">{p.streamUrl}</td>
                    <td class="px-5 py-3 font-mono text-xs text-fg-faint">
                      {p.keyConfigured ? '••••••••' : '—'}
                    </td>
                    <td class="px-5 py-3">
                      <button
                        onClick={() => onToggle(p.id, !p.enabled)}
                        class={`inline-block px-2 py-0.5 rounded text-xs font-semibold cursor-pointer transition-colors ${
                          p.enabled
                            ? 'bg-success-bg text-success hover:opacity-80'
                            : 'bg-surface-hover text-fg-muted border border-border hover:bg-surface-active'
                        }`}
                      >
                        {p.enabled ? t('platforms.yes') : t('platforms.no')}
                      </button>
                    </td>
                    <td class="px-5 py-3">
                      <div class="flex items-center gap-2">
                        <button
                          onClick={() => startEdit(p)}
                          class="px-3 py-1 text-xs rounded border border-accent text-accent hover:bg-accent-bg transition-colors"
                        >
                          {t('platforms.edit')}
                        </button>
                        <button
                          onClick={() => handleRemove(p.id, p.displayName)}
                          disabled={removing === p.id}
                          class="px-3 py-1 text-xs rounded border text-danger hover:bg-danger-bg disabled:opacity-50 transition-colors"
                          style={{ borderColor: 'var(--danger)' }}
                        >
                          {removing === p.id ? t('platforms.removing') : t('platforms.remove')}
                        </button>
                      </div>
                    </td>
                  </tr>
                ),
              )
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
