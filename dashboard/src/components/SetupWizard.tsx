import { useState, useCallback, useEffect } from 'preact/hooks';

interface SetupPlatform {
  name: string;
  url: string;
  key: string;
  orientation: 'horizontal' | 'vertical';
}

interface SetupStatus {
  first_run: boolean;
  config_exists: boolean;
  has_stream_key: boolean;
  platform_count: number;
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
  const [step, setStep] = useState<Step>('welcome');
  const [error, setError] = useState<string | null>(null);

  const [rtmpPort, setRtmpPort] = useState('1935');
  const [streamKey, setStreamKey] = useState('');
  const [platforms, setPlatforms] = useState<SetupPlatform[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    fetch('/api/setup/status')
      .then((r: Response) => r.json())
      .then((d: { success: boolean; data?: SetupStatus }) => {
        if (d.success && d.data && !d.data.first_run) {
          // Already configured, redirect to dashboard
          window.location.href = '/';
        }
      })
      .catch(() => {});
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
        setError(data.error ?? 'Setup failed');
      }
    } catch (e) {
      setError(`Network error: ${e}`);
    } finally {
      setSaving(false);
    }
  }, [rtmpPort, streamKey, platforms]);

  const validPlatforms = platforms.filter((p) => p.url && p.key);
  const canSave = streamKey.length > 0;

  return (
    <div class="min-h-screen bg-slate-950 flex items-center justify-center p-4">
      <div class="w-full max-w-2xl">
        {/* Progress */}
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
                      ? 'bg-sky-500 text-white'
                      : done
                        ? 'bg-emerald-600 text-white'
                        : 'bg-slate-800 text-slate-500'
                  }`}
                >
                  {done ? '✓' : i + 1}
                </div>
                {i < 3 && <div class={`w-8 h-0.5 ${done ? 'bg-emerald-600' : 'bg-slate-800'}`} />}
              </div>
            );
          })}
        </div>

        <div class="bg-slate-900 border border-slate-800 rounded-2xl p-8">
          {/* Welcome */}
          {step === 'welcome' && (
            <div class="text-center">
              <div class="w-16 h-16 mx-auto mb-4 rounded-2xl bg-sky-500/20 flex items-center justify-center">
                <svg class="w-8 h-8 text-sky-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                </svg>
              </div>
              <h1 class="text-2xl font-bold mb-2">Welcome to Reestream</h1>
              <p class="text-slate-400 mb-6">
                Let's set up your streaming relay. This wizard will configure your
                RTMP server and output platforms.
              </p>
              <button
                onClick={() => setStep('server')}
                class="px-6 py-3 bg-sky-600 hover:bg-sky-500 text-white rounded-lg font-medium transition-colors"
              >
                Get Started
              </button>
            </div>
          )}

          {/* Server Config */}
          {step === 'server' && (
            <div>
              <h2 class="text-xl font-bold mb-1">Server Configuration</h2>
              <p class="text-slate-400 text-sm mb-6">Configure your RTMP server settings.</p>

              <div class="space-y-4">
                <div>
                  <label class="block text-sm text-slate-400 mb-1">RTMP Port</label>
                  <input
                    type="number"
                    value={rtmpPort}
                    onInput={(e) => setRtmpPort((e.target as HTMLInputElement).value)}
                    class="w-full bg-slate-800 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 focus:outline-none focus:border-sky-500"
                  />
                  <p class="text-xs text-slate-500 mt-1">Default: 1935. Use 1935 for standard RTMP.</p>
                </div>

                <div>
                  <label class="block text-sm text-slate-400 mb-1">Stream Key</label>
                  <input
                    type="password"
                    value={streamKey}
                    onInput={(e) => setStreamKey((e.target as HTMLInputElement).value)}
                    placeholder="your-secret-stream-key"
                    class="w-full bg-slate-800 border border-slate-700 rounded-lg px-4 py-2.5 text-slate-200 focus:outline-none focus:border-sky-500"
                  />
                  <p class="text-xs text-slate-500 mt-1">This key is required to publish streams. Keep it secret.</p>
                </div>
              </div>

              <div class="flex justify-between mt-8">
                <button
                  onClick={() => setStep('welcome')}
                  class="px-4 py-2 text-slate-400 hover:text-slate-200 transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={() => setStep('platforms')}
                  disabled={!streamKey}
                  class="px-6 py-2.5 bg-sky-600 hover:bg-sky-500 disabled:bg-slate-700 disabled:text-slate-500 text-white rounded-lg font-medium transition-colors"
                >
                  Next
                </button>
              </div>
            </div>
          )}

          {/* Platforms */}
          {step === 'platforms' && (
            <div>
              <h2 class="text-xl font-bold mb-1">Output Platforms</h2>
              <p class="text-slate-400 text-sm mb-4">Add streaming destinations. You can skip this and add them later.</p>

              {/* Presets */}
              <div class="flex flex-wrap gap-2 mb-4">
                {PRESETS.map((p) => (
                  <button
                    key={p.name}
                    onClick={() => addPlatform(p)}
                    class="px-3 py-1.5 text-xs rounded-lg bg-slate-800 border border-slate-700 hover:border-sky-500 hover:text-sky-400 transition-colors"
                  >
                    + {p.name}
                  </button>
                ))}
                <button
                  onClick={addCustomPlatform}
                  class="px-3 py-1.5 text-xs rounded-lg bg-slate-800 border border-slate-700 border-dashed hover:border-sky-500 hover:text-sky-400 transition-colors"
                >
                  + Custom
                </button>
              </div>

              {/* Platform list */}
              {platforms.length === 0 ? (
                <div class="text-center py-8 text-slate-500 text-sm">
                  No platforms added. You can add them later from the dashboard.
                </div>
              ) : (
                <div class="space-y-3 max-h-64 overflow-y-auto">
                  {platforms.map((p, i) => (
                    <div key={i} class="bg-slate-800 rounded-lg p-4 border border-slate-700">
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
                          class="bg-slate-700 border border-slate-600 rounded px-2 py-1 text-sm text-slate-200"
                        >
                          {PRESETS.map((pr) => (
                            <option key={pr.name} value={pr.name}>{pr.name}</option>
                          ))}
                          <option value="Custom">Custom</option>
                        </select>
                        <button
                          onClick={() => removePlatform(i)}
                          class="text-red-400 hover:text-red-300 text-xs"
                        >
                          Remove
                        </button>
                      </div>
                      <input
                        value={p.url}
                        onInput={(e) => updatePlatform(i, 'url', (e.target as HTMLInputElement).value)}
                        placeholder="rtmp://server/app"
                        class="w-full bg-slate-700 border border-slate-600 rounded px-3 py-1.5 text-sm text-slate-200 mb-2 focus:outline-none focus:border-sky-500"
                      />
                      <input
                        value={p.key}
                        onInput={(e) => updatePlatform(i, 'key', (e.target as HTMLInputElement).value)}
                        placeholder={PRESETS.find((pr) => pr.name === p.name)?.placeholder ?? 'stream-key'}
                        class="w-full bg-slate-700 border border-slate-600 rounded px-3 py-1.5 text-sm text-slate-200 focus:outline-none focus:border-sky-500"
                      />
                      <div class="flex items-center gap-3 mt-2">
                        <label class="text-xs text-slate-500">Orientation:</label>
                        <select
                          value={p.orientation}
                          onChange={(e) =>
                            updatePlatform(i, 'orientation', (e.target as HTMLSelectElement).value)
                          }
                          class="bg-slate-700 border border-slate-600 rounded px-2 py-1 text-xs text-slate-300"
                        >
                          <option value="horizontal">Horizontal (16:9)</option>
                          <option value="vertical">Vertical (9:16)</option>
                        </select>
                      </div>
                    </div>
                  ))}
                </div>
              )}

              <div class="flex justify-between mt-6">
                <button
                  onClick={() => setStep('server')}
                  class="px-4 py-2 text-slate-400 hover:text-slate-200 transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={() => setStep('confirm')}
                  class="px-6 py-2.5 bg-sky-600 hover:bg-sky-500 text-white rounded-lg font-medium transition-colors"
                >
                  Next
                </button>
              </div>
            </div>
          )}

          {/* Confirm */}
          {step === 'confirm' && (
            <div>
              <h2 class="text-xl font-bold mb-1">Review Configuration</h2>
              <p class="text-slate-400 text-sm mb-6">Confirm your settings before saving.</p>

              <div class="space-y-3">
                <div class="bg-slate-800 rounded-lg p-4">
                  <div class="text-xs text-slate-500 mb-1">RTMP Port</div>
                  <div class="text-slate-200">{rtmpPort}</div>
                </div>
                <div class="bg-slate-800 rounded-lg p-4">
                  <div class="text-xs text-slate-500 mb-1">Stream Key</div>
                  <div class="text-slate-200 font-mono">{'•'.repeat(Math.min(streamKey.length, 20))}</div>
                </div>
                <div class="bg-slate-800 rounded-lg p-4">
                  <div class="text-xs text-slate-500 mb-1">Platforms ({validPlatforms.length})</div>
                  {validPlatforms.length === 0 ? (
                    <div class="text-slate-500 text-sm">None — add later from dashboard</div>
                  ) : (
                    <div class="space-y-1">
                      {validPlatforms.map((p, i) => (
                        <div key={i} class="text-slate-200 text-sm">
                          {p.name} — {p.orientation}
                        </div>
                      ))}
                    </div>
                  )}
                </div>
              </div>

              {error && (
                <div class="mt-4 p-3 bg-red-900/30 border border-red-800 rounded-lg text-red-400 text-sm">
                  {error}
                </div>
              )}

              <div class="flex justify-between mt-8">
                <button
                  onClick={() => setStep('platforms')}
                  class="px-4 py-2 text-slate-400 hover:text-slate-200 transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={handleSave}
                  disabled={saving || !canSave}
                  class="px-6 py-2.5 bg-emerald-600 hover:bg-emerald-500 disabled:bg-slate-700 disabled:text-slate-500 text-white rounded-lg font-medium transition-colors"
                >
                  {saving ? 'Saving…' : 'Save & Start'}
                </button>
              </div>
            </div>
          )}

          {/* Done */}
          {step === 'done' && (
            <div class="text-center">
              <div class="w-16 h-16 mx-auto mb-4 rounded-2xl bg-emerald-500/20 flex items-center justify-center">
                <svg class="w-8 h-8 text-emerald-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                </svg>
              </div>
              <h1 class="text-2xl font-bold mb-2">Setup Complete!</h1>
              <p class="text-slate-400 mb-6">
                Your Reestream server is configured and ready.
              </p>
              <p class="text-slate-500 text-sm mb-6">
                Restart the server to apply the new configuration:
              </p>
              <code class="block bg-slate-800 rounded-lg px-4 py-3 text-sm text-sky-400 mb-6">
                reestream --config config.toml
              </code>
              <a
                href="/"
                class="inline-block px-6 py-2.5 bg-sky-600 hover:bg-sky-500 text-white rounded-lg font-medium transition-colors"
              >
                Open Dashboard
              </a>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
