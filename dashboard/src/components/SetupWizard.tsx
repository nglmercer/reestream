import { useState, useCallback, useEffect } from 'preact/hooks';
import type { Orientation, SetupStatus } from '../api';
import { useLocale } from '../hooks/useLocale';

interface SetupPlatform {
  name: string;
  url: string;
  key: string;
  orientation: Orientation;
}

type Step = 'welcome' | 'server' | 'platforms' | 'confirm' | 'done';

const PRESETS: Array<{ name: string; url: string; placeholder: string }> = [
  { name: 'Twitch', url: 'rtmp://live.twitch.tv/app', placeholder: 'live_123456789_abc...' },
  { name: 'YouTube', url: 'rtmp://a.rtmp.youtube.com/live2', placeholder: 'xxxx-xxxx-xxxx-xxxx' },
  { name: 'Facebook', url: 'rtmps://live-api-s.facebook.com:443/rtmp/', placeholder: 'FB-1234567890-1234-abcdef' },
  { name: 'Instagram', url: 'rtmps://edge-upload.instagram.com:443/rtmp/', placeholder: 'IG-1234567890' },
  { name: 'Kick', url: 'rtmp://fa723fc1b141.global-contribute.live-video.net/app', placeholder: 'sk_live_...' },
  { name: 'TikTok', url: 'rtmp://push.tiktok.com/live/', placeholder: 'stream-key' },
];

export function SetupWizard() {
  const { t } = useLocale();
  const [step, setStep] = useState<Step>('welcome');
  const [error, setError] = useState<string | null>(null);

  const [rtmpPort, setRtmpPort] = useState('1935');
  const [streamKey, setStreamKey] = useState('');
  const [platforms, setPlatforms] = useState<SetupPlatform[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const ctrl = new AbortController();
    fetch('/api/setup/status', { signal: ctrl.signal })
      .then((r: Response) => r.json())
      .then((d: { success: boolean; data?: SetupStatus }) => {
        if (d.success && d.data && !d.data.first_run) {
          window.location.href = '/';
        }
      })
      .catch(() => {});
    return () => ctrl.abort();
  }, []);

  const addPlatform = useCallback((preset: (typeof PRESETS)[number]) => {
    setPlatforms((prev) => [
      ...prev,
      { name: preset.name, url: preset.url, key: '', orientation: 'horizontal' },
    ]);
  }, []);

  const addCustomPlatform = useCallback(() => {
    setPlatforms((prev) => [
      ...prev,
      { name: 'Custom', url: '', key: '', orientation: 'horizontal' },
    ]);
  }, []);

  const removePlatform = useCallback((idx: number) => {
    setPlatforms((prev) => prev.filter((_, i) => i !== idx));
  }, []);

  const updatePlatform = useCallback(
    (idx: number, field: keyof SetupPlatform, value: string) => {
      setPlatforms((prev) =>
        prev.map((p, i) => (i === idx ? { ...p, [field]: value } : p)),
      );
    },
    [],
  );

  const handleSave = useCallback(async () => {
    setSaving(true);
    setError(null);
    try {
      const res = await fetch('/api/setup/save', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          rtmp_port: parseInt(rtmpPort, 10),
          stream_key: streamKey,
          platforms: platforms.map((p) => ({
            name: p.name,
            url: p.url,
            key: p.key,
            orientation: p.orientation,
          })),
        }),
      });
      const data = await res.json();
      if (data.success) {
        setStep('done');
      } else {
        setError(data.error ?? t('setup.failed'));
      }
    } catch (e) {
      setError(t('setup.networkError', { error: String(e) }));
    } finally {
      setSaving(false);
    }
  }, [rtmpPort, streamKey, platforms]);

  const validPlatforms = platforms.filter((p) => p.url && p.key);
  const canSave = streamKey.length > 0;

  return (
    <div class="min-h-screen bg-surface flex items-center justify-center p-4">
      <div class="w-full max-w-2xl">
        <div class="flex items-center justify-center gap-2 mb-8">
          {(['welcome', 'server', 'platforms', 'confirm'] as Step[]).map((s, i) => {
            const steps: Step[] = ['welcome', 'server', 'platforms', 'confirm'];
            const currentIdx = steps.indexOf(step);
            const active = s === step;
            const done = i < currentIdx;
            return (
              <div key={s} class="flex items-center gap-2">
                <div
                  class={`w-8 h-8 rounded-full flex items-center justify-center text-xs font-bold transition-colors ${
                    active
                      ? 'bg-accent text-white'
                      : done
                        ? 'bg-success text-white'
                        : 'bg-surface-hover text-fg-faint'
                  }`}
                >
                  {done ? '✓' : i + 1}
                </div>
                {i < 3 && <div class={`w-8 h-0.5 ${done ? 'bg-success' : 'bg-surface-hover'}`} />}
              </div>
            );
          })}
        </div>

        <div class="bg-surface-alt border border-border rounded-2xl p-8">
          {step === 'welcome' && (
            <div class="text-center">
              <div class="w-16 h-16 mx-auto mb-4 rounded-2xl flex items-center justify-center" style={{ backgroundColor: 'var(--accent-bg)' }}>
                <svg class="w-8 h-8 text-accent" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>
              <h1 class="text-2xl font-bold mb-2 text-fg">{t('setup.welcome')}</h1>
              <p class="text-fg-muted mb-6">
                {t('setup.welcomeDesc')}
              </p>
              <button
                onClick={() => setStep('server')}
                class="px-6 py-3 bg-accent hover:bg-accent-hover text-white rounded-lg font-medium transition-colors"
              >
                {t('setup.getStarted')}
              </button>
            </div>
          )}

          {step === 'server' && (
            <div>
              <h2 class="text-xl font-bold mb-1 text-fg">{t('setup.serverConfig')}</h2>
              <p class="text-fg-muted text-sm mb-6">{t('setup.serverConfigDesc')}</p>

              <div class="space-y-4">
                <div>
                  <label class="block text-sm text-fg-muted mb-1">{t('setup.rtmpPort')}</label>
                  <input
                    type="number"
                    value={rtmpPort}
                    onInput={(e) => setRtmpPort((e.target as HTMLInputElement).value)}
                    class="w-full bg-surface-input border border-border rounded-lg px-4 py-2.5 text-fg focus:outline-none focus:border-accent"
                  />
                  <p class="text-xs text-fg-faint mt-1">{t('setup.rtmpPortHelp')}</p>
                </div>

                <div>
                  <label class="block text-sm text-fg-muted mb-1">{t('setup.streamKey')}</label>
                  <input
                    type="password"
                    value={streamKey}
                    onInput={(e) => setStreamKey((e.target as HTMLInputElement).value)}
                    placeholder={t('setup.streamKeyPlaceholder')}
                    class="w-full bg-surface-input border border-border rounded-lg px-4 py-2.5 text-fg focus:outline-none focus:border-accent"
                  />
                  <p class="text-xs text-fg-faint mt-1">{t('setup.streamKeyHelp')}</p>
                </div>
              </div>

              <div class="flex justify-between mt-8">
                <button
                  onClick={() => setStep('welcome')}
                  class="px-4 py-2 text-fg-muted hover:text-fg transition-colors"
                >
                  {t('setup.back')}
                </button>
                <button
                  onClick={() => setStep('platforms')}
                  disabled={!streamKey}
                  class="px-6 py-2.5 bg-accent hover:bg-accent-hover disabled:bg-surface-active disabled:text-fg-faint text-white rounded-lg font-medium transition-colors"
                >
                  {t('setup.next')}
                </button>
              </div>
            </div>
          )}

          {step === 'platforms' && (
            <div>
              <h2 class="text-xl font-bold mb-1 text-fg">{t('setup.outputPlatforms')}</h2>
              <p class="text-fg-muted text-sm mb-4">{t('setup.outputPlatformsDesc')}</p>

              <div class="flex flex-wrap gap-2 mb-4">
                {PRESETS.map((p) => (
                  <button
                    key={p.name}
                    onClick={() => addPlatform(p)}
                    class="px-3 py-1.5 text-xs rounded-lg bg-surface-hover border border-border hover:border-accent hover:text-accent transition-colors text-fg-secondary"
                  >
                    {t('setup.addPreset', { name: p.name })}
                  </button>
                ))}
                <button
                  onClick={addCustomPlatform}
                  class="px-3 py-1.5 text-xs rounded-lg bg-surface-hover border border-border border-dashed hover:border-accent hover:text-accent transition-colors text-fg-secondary"
                >
                  {t('setup.addCustom')}
                </button>
              </div>

              {platforms.length === 0 ? (
                <div class="text-center py-8 text-fg-faint text-sm">
                  {t('setup.noPlatforms')}
                </div>
              ) : (
                <div class="space-y-3 max-h-64 overflow-y-auto">
                  {platforms.map((p, i) => (
                    <div key={i} class="bg-surface-raised rounded-lg p-4 border border-border">
                      <div class="flex items-center justify-between mb-2">
                        <select
                          value={p.name}
                          onChange={(e) => {
                            const val = (e.target as HTMLSelectElement).value;
                            const preset = PRESETS.find((pr) => pr.name === val);
                            if (preset) {
                              updatePlatform(i, 'name', val);
                              updatePlatform(i, 'url', preset.url);
                            } else {
                              updatePlatform(i, 'name', val);
                            }
                          }}
                          class="bg-surface-hover border border-border rounded px-2 py-1 text-sm text-fg"
                        >
                          {PRESETS.map((pr) => (
                            <option key={pr.name} value={pr.name}>{pr.name}</option>
                          ))}
                          <option value="Custom">Custom</option>
                        </select>
                        <button
                          onClick={() => removePlatform(i)}
                          class="text-danger hover:text-danger text-xs"
                        >
                          {t('setup.remove')}
                        </button>
                      </div>
                      <input
                        value={p.url}
                        onInput={(e) => updatePlatform(i, 'url', (e.target as HTMLInputElement).value)}
                        placeholder="rtmp://server/app"
                        class="w-full bg-surface-hover border border-border rounded px-3 py-1.5 text-sm text-fg mb-2 focus:outline-none focus:border-accent"
                      />
                      <input
                        value={p.key}
                        onInput={(e) => updatePlatform(i, 'key', (e.target as HTMLInputElement).value)}
                        placeholder={PRESETS.find((pr) => pr.name === p.name)?.placeholder ?? 'stream-key'}
                        class="w-full bg-surface-hover border border-border rounded px-3 py-1.5 text-sm text-fg focus:outline-none focus:border-accent"
                      />
                      <div class="flex items-center gap-3 mt-2">
                        <label class="text-xs text-fg-faint">{t('setup.orientation')}</label>
                        <select
                          value={p.orientation}
                          onChange={(e) =>
                            updatePlatform(i, 'orientation', (e.target as HTMLSelectElement).value)
                          }
                          class="bg-surface-hover border border-border rounded px-2 py-1 text-xs text-fg-secondary"
                        >
                          <option value="horizontal">{t('setup.horizontal')}</option>
                          <option value="vertical">{t('setup.vertical')}</option>
                        </select>
                      </div>
                    </div>
                  ))}
                </div>
              )}

              <div class="flex justify-between mt-6">
                <button
                  onClick={() => setStep('server')}
                  class="px-4 py-2 text-fg-muted hover:text-fg transition-colors"
                >
                  {t('setup.back')}
                </button>
                <button
                  onClick={() => setStep('confirm')}
                  class="px-6 py-2.5 bg-accent hover:bg-accent-hover text-white rounded-lg font-medium transition-colors"
                >
                  {t('setup.next')}
                </button>
              </div>
            </div>
          )}

          {step === 'confirm' && (
            <div>
              <h2 class="text-xl font-bold mb-1 text-fg">{t('setup.review')}</h2>
              <p class="text-fg-muted text-sm mb-6">{t('setup.reviewDesc')}</p>

              <div class="space-y-3">
                <div class="bg-surface-raised rounded-lg p-4 border border-border">
                  <div class="text-xs text-fg-faint mb-1">{t('setup.reviewPort')}</div>
                  <div class="text-fg">{rtmpPort}</div>
                </div>
                <div class="bg-surface-raised rounded-lg p-4 border border-border">
                  <div class="text-xs text-fg-faint mb-1">{t('setup.reviewKey')}</div>
                  <div class="text-fg font-mono">{'•'.repeat(Math.min(streamKey.length, 20))}</div>
                </div>
                <div class="bg-surface-raised rounded-lg p-4 border border-border">
                  <div class="text-xs text-fg-faint mb-1">{t('setup.reviewPlatforms', { count: validPlatforms.length })}</div>
                  {validPlatforms.length === 0 ? (
                    <div class="text-fg-faint text-sm">{t('setup.reviewNoPlatforms')}</div>
                  ) : (
                    <div class="space-y-1">
                      {validPlatforms.map((p, i) => (
                        <div key={i} class="text-fg text-sm">
                          {p.name} — {p.orientation}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>

              {error && (
                <div class="mt-4 p-3 rounded-lg text-danger text-sm border" style={{ backgroundColor: 'var(--danger-bg)', borderColor: 'var(--danger)' }}>
                  {error}
                </div>
              )}

              <div class="flex justify-between mt-8">
                <button
                  onClick={() => setStep('platforms')}
                  class="px-4 py-2 text-fg-muted hover:text-fg transition-colors"
                >
                  {t('setup.back')}
                </button>
                <button
                  onClick={handleSave}
                  disabled={saving || !canSave}
                  class="px-6 py-2.5 bg-success hover:opacity-90 disabled:bg-surface-active disabled:text-fg-faint text-white rounded-lg font-medium transition-colors"
                >
                  {saving ? t('setup.saving') : t('setup.saveStart')}
                </button>
              </div>
            </div>
          )}

          {step === 'done' && (
            <div class="text-center">
              <div class="w-16 h-16 mx-auto mb-4 rounded-2xl flex items-center justify-center" style={{ backgroundColor: 'var(--success-bg)' }}>
                <svg class="w-8 h-8 text-success" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                </svg>
              </div>
              <h1 class="text-2xl font-bold mb-2 text-fg">{t('setup.done')}</h1>
              <p class="text-fg-muted mb-6">
                {t('setup.doneDesc')}
              </p>
              <p class="text-fg-faint text-sm mb-6">
                {t('setup.doneHint')}
              </p>
              <code class="block bg-surface-raised rounded-lg px-4 py-3 text-sm text-accent mb-6 border border-border">
                reestream --config config.toml
              </code>
              <a
                href="/"
                class="inline-block px-6 py-2.5 bg-accent hover:bg-accent-hover text-white rounded-lg font-medium transition-colors"
              >
                {t('setup.openDashboard')}
              </a>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
