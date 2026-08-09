import { useState, useEffect, useCallback, useRef } from 'preact/hooks';
import { apiV1 } from '../api';
import { useLocale } from '../hooks/useLocale';

interface ServerInfoView {
  rtmpUrl: string;
  rtmpsUrl: string | null;
  srtUrl: string | null;
  hlsUrl: string;
  flvUrl: string;
  dashboardUrl: string;
  apiUrl: string;
  metricsUrl: string;
  streamKeyMasked: string;
}

function maskKey(key: string | undefined): string {
  if (!key) return '****';
  if (key.length <= 4) return '****';
  return `${key.slice(0, 4)}…${key.slice(-4)}`;
}

interface Props {
  onClose: () => void;
  addLog: (msg: string, level?: 'info' | 'warn' | 'error') => void;
}

export function SettingsPanel({ onClose, addLog }: Props) {
  const { t } = useLocale();
  const [info, setInfo] = useState<ServerInfoView | null>(null);
  const [streamKey, setStreamKey] = useState<string | null>(null);
  const [showKey, setShowKey] = useState(false);
  const [loading, setLoading] = useState(true);
  const [resetting, setResetting] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let active = true;
    Promise.all([
      apiV1.getIngest(),
      apiV1.getGlobalStreamKey().catch(() => null),
    ])
      .then(([ingest, credentials]) => {
        if (!active) return;
        const origin = window.location.origin;
        const rtmpUrl = ingest.serverUrl.replace(/\/live\/?$/, '');
        setInfo({
          rtmpUrl,
          rtmpsUrl: ingest.backupServerUrl,
          srtUrl: ingest.srtUrl ?? credentials?.srtUrl ?? null,
          hlsUrl: `${origin}/stream.m3u8`,
          flvUrl: `${origin}/stream.flv`,
          dashboardUrl: origin,
          apiUrl: `${origin}/api/v1`,
          metricsUrl: `${origin}/metrics`,
          streamKeyMasked: maskKey(credentials?.streamKey),
        });
      })
      .catch(() => addLog(t('log.settingsLoadFailed'), 'error'))
      .finally(() => setLoading(false));
    return () => {
      active = false;
    };
  }, [addLog]);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const handleRevealKey = useCallback(async () => {
    if (streamKey) {
      setShowKey(!showKey);
      return;
    }
    try {
      const data = await apiV1.getGlobalStreamKey();
      setStreamKey(data.streamKey);
      setShowKey(true);
    } catch {
      addLog(t('log.keyRevealFailed'), 'error');
    }
  }, [streamKey, showKey, addLog]);

  const handleResetKey = useCallback(async () => {
    if (!confirm(t('settings.confirmReset'))) return;
    setResetting(true);
    try {
      const data = await apiV1.resetGlobalStreamKey();
      setStreamKey(data.streamKey);
      setShowKey(true);
      addLog(t('log.keyResetSuccess'));
    } catch (error) {
      addLog(t('log.keyResetFailed', { error: error instanceof Error ? error.message : String(error) }), 'error');
    } finally {
      setResetting(false);
    }
  }, [addLog]);

  const copyToClipboard = useCallback(async (text: string, label: string) => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      const ta = document.createElement('textarea');
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
    }
    setCopied(label);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setCopied(null), 1500);
  }, []);

  if (loading) {
    return (
      <div class="settings-overlay fixed inset-0 flex items-center justify-center z-50" style={{ backgroundColor: 'var(--overlay)' }}>
        <div class="settings-card bg-surface-alt border border-border rounded-2xl p-8">
          <div class="text-fg-muted animate-pulse">{t('settings.loading')}</div>
        </div>
      </div>
    );
  }

  const endpoints = info
      ? [
        { label: t('settings.endpoint.rtmp'), value: info.rtmpUrl, note: t('settings.endpoint.rtmpNote') },
        { label: t('settings.endpoint.rtmps'), value: info.rtmpsUrl, note: t('settings.endpoint.rtmpsNote') },
        { label: t('settings.endpoint.srt'), value: info.srtUrl, note: t('settings.endpoint.srtNote') },
        { label: t('settings.endpoint.hls'), value: info.hlsUrl, note: t('settings.endpoint.hlsNote') },
        { label: t('settings.endpoint.flv'), value: info.flvUrl, note: t('settings.endpoint.flvNote') },
        { label: t('settings.endpoint.dashboard'), value: info.dashboardUrl, note: t('settings.endpoint.dashboardNote') },
        { label: t('settings.endpoint.api'), value: info.apiUrl, note: t('settings.endpoint.apiNote') },
        { label: t('settings.endpoint.metrics'), value: info.metricsUrl, note: t('settings.endpoint.metricsNote') },
      ]
    : [];

  return (
    <div class="settings-overlay fixed inset-0 flex items-center justify-center z-50 p-4" style={{ backgroundColor: 'var(--overlay)' }} onClick={onClose}>
      <div
        class="settings-card bg-surface-alt border border-border rounded-2xl w-full max-w-2xl max-h-[85vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <div class="flex items-center justify-between px-6 py-4 border-b border-border sticky top-0 bg-surface-alt z-10">
          <h2 class="text-lg font-bold text-fg">{t('settings.title')}</h2>
          <button
            onClick={onClose}
            class="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-surface-hover text-fg-muted hover:text-fg transition-colors"
          >
            <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="p-6 space-y-6">
          <div>
            <h3 class="text-sm font-semibold text-fg-muted uppercase tracking-wider mb-3">{t('settings.streamKey')}</h3>
            <div class="bg-surface-raised rounded-xl p-4 border border-border">
              <div class="flex items-center gap-3 mb-3">
                <div class="flex-1 font-mono text-sm bg-surface rounded-lg px-4 py-2.5 border border-border text-fg">
                  {showKey && streamKey ? streamKey : info?.streamKeyMasked ?? '****'}
                </div>
                <button
                  onClick={handleRevealKey}
                  class="px-3 py-2.5 text-xs rounded-lg bg-surface-hover hover:bg-surface-active border border-border transition-colors text-fg-secondary whitespace-nowrap"
                >
                  {showKey ? t('settings.hide') : t('settings.reveal')}
                </button>
                <button
                  onClick={() => {
                    const key = streamKey ?? info?.streamKeyMasked ?? '';
                    copyToClipboard(key, 'key');
                  }}
                  class="px-3 py-2.5 text-xs rounded-lg bg-surface-hover hover:bg-surface-active border border-border transition-colors text-fg-secondary whitespace-nowrap"
                >
                  {copied === 'key' ? t('settings.copied') : t('settings.copy')}
                </button>
              </div>
              <button
                onClick={handleResetKey}
                disabled={resetting}
                class="w-full px-4 py-2 text-sm rounded-lg border text-danger disabled:opacity-50 transition-colors"
                style={{ backgroundColor: 'var(--danger-bg)', borderColor: 'var(--danger)' }}
              >
                {resetting ? t('settings.resetting') : t('settings.resetKey')}
              </button>
              <p class="text-xs text-fg-faint mt-2">
                {t('settings.resetHelp')}
              </p>
            </div>
          </div>

          <div>
            <h3 class="text-sm font-semibold text-fg-muted uppercase tracking-wider mb-3">
              {t('settings.endpoints')}
            </h3>
            <div class="space-y-2">
              {endpoints.filter((ep) => ep.value != null).map((ep) => (
                <div
                  key={ep.label}
                  class="bg-surface-raised rounded-lg px-4 py-3 border border-border flex items-center justify-between gap-3"
                >
                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-sm font-medium text-fg">{ep.label}</span>
                      <span class="text-xs text-fg-faint">{ep.note}</span>
                    </div>
                    <div class="font-mono text-xs text-accent truncate mt-0.5">{ep.value}</div>
                  </div>
                  <button
                    onClick={() => copyToClipboard(ep.value!, ep.label)}
                    class="shrink-0 px-2 py-1 text-xs rounded bg-surface-hover hover:bg-surface-active border border-border transition-colors text-fg-secondary"
                  >
                    {copied === ep.label ? t('settings.copied') : t('settings.copy')}
                  </button>
                </div>
              ))}
            </div>
          </div>

          <div>
            <h3 class="text-sm font-semibold text-fg-muted uppercase tracking-wider mb-3">
              {t('settings.obsSetup')}
            </h3>
            <div class="bg-surface-raised rounded-xl p-4 border border-border space-y-3">
              {[
                t('settings.obsStep1'),
                t('settings.obsStep2Service') + t('settings.obsStep2Value'),
              ].map((text, i) => (
                <div key={i} class="flex items-start gap-3">
                  <span class="shrink-0 w-6 h-6 rounded-full bg-accent text-white text-xs flex items-center justify-center font-bold">{i + 1}</span>
                  <div class="text-sm text-fg">{text}</div>
                </div>
              ))}
              <div class="flex items-start gap-3">
                <span class="shrink-0 w-6 h-6 rounded-full bg-accent text-white text-xs flex items-center justify-center font-bold">3</span>
                <div class="text-sm text-fg">
                  {t('settings.obsStep3')}<code class="text-accent bg-surface px-1.5 py-0.5 rounded text-xs">{info?.rtmpUrl ?? 'rtmp://localhost:1935'}</code>
                </div>
              </div>
              <div class="flex items-start gap-3">
                <span class="shrink-0 w-6 h-6 rounded-full bg-accent text-white text-xs flex items-center justify-center font-bold">4</span>
                <div class="text-sm text-fg">
                  {t('settings.obsStep4')}<code class="text-accent bg-surface px-1.5 py-0.5 rounded text-xs">{showKey && streamKey ? streamKey : info?.streamKeyMasked ?? '****'}</code>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
