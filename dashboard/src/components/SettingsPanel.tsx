import { useState, useEffect, useCallback, useRef } from 'preact/hooks';

interface ServerInfo {
  rtmp_url: string;
  rtmps_url: string | null;
  srt_url: string | null;
  http_url: string;
  hls_url: string;
  flv_url: string;
  dashboard_url: string;
  api_url: string;
  metrics_url: string;
  stream_key_masked: string;
  rtmp_port: number;
  http_port: number;
  srt_port: number;
  hostname: string;
}

interface Props {
  onClose: () => void;
  addLog: (msg: string, level?: 'info' | 'warn' | 'error') => void;
}

export function SettingsPanel({ onClose, addLog }: Props) {
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [streamKey, setStreamKey] = useState<string | null>(null);
  const [showKey, setShowKey] = useState(false);
  const [loading, setLoading] = useState(true);
  const [resetting, setResetting] = useState(false);
  const [copied, setCopied] = useState<string | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const ctrl = new AbortController();
    fetch('/api/setup/info', { signal: ctrl.signal })
      .then((r) => r.json())
      .then((infoRes) => {
        if (infoRes.success) setInfo(infoRes.data);
      })
      .catch(() => addLog('Failed to load server info', 'error'))
      .finally(() => setLoading(false));
    return () => ctrl.abort();
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
      const res = await fetch('/api/setup/key');
      const data = await res.json();
      if (data.success) {
        setStreamKey(data.data);
        setShowKey(true);
      }
    } catch {
      addLog('Failed to reveal stream key', 'error');
    }
  }, [streamKey, showKey, addLog]);

  const handleResetKey = useCallback(async () => {
    if (!confirm('Generate a new stream key? The old key will stop working immediately.')) return;
    setResetting(true);
    try {
      const res = await fetch('/api/setup/key', { method: 'POST' });
      const data = await res.json();
      if (data.success) {
        setStreamKey(data.data);
        setShowKey(true);
        addLog('Stream key reset successfully');
      } else {
        addLog(`Reset failed: ${data.error}`, 'error');
      }
    } catch {
      addLog('Failed to reset stream key', 'error');
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
      <div class="fixed inset-0 flex items-center justify-center z-50" style={{ backgroundColor: 'var(--overlay)' }}>
        <div class="bg-surface-alt border border-border rounded-2xl p-8">
          <div class="text-fg-muted animate-pulse">Loading settings…</div>
        </div>
      </div>
    );
  }

  const endpoints = info
    ? [
        { label: 'RTMP Ingest', value: info.rtmp_url, note: 'Primary input' },
        { label: 'RTMPS Ingest', value: info.rtmps_url, note: 'TLS encrypted' },
        { label: 'SRT Ingest', value: info.srt_url, note: 'Low latency' },
        { label: 'HLS Stream', value: info.hls_url, note: 'For playback' },
        { label: 'FLV Stream', value: info.flv_url, note: 'Low latency playback' },
        { label: 'Dashboard', value: info.dashboard_url, note: 'Web UI' },
        { label: 'API', value: info.api_url, note: 'REST API' },
        { label: 'Metrics', value: info.metrics_url, note: 'Prometheus' },
      ]
    : [];

  return (
    <div class="fixed inset-0 flex items-center justify-center z-50 p-4" style={{ backgroundColor: 'var(--overlay)' }} onClick={onClose}>
      <div
        class="bg-surface-alt border border-border rounded-2xl w-full max-w-2xl max-h-[85vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        <div class="flex items-center justify-between px-6 py-4 border-b border-border sticky top-0 bg-surface-alt z-10">
          <h2 class="text-lg font-bold text-fg">Settings</h2>
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
            <h3 class="text-sm font-semibold text-fg-muted uppercase tracking-wider mb-3">Stream Key</h3>
            <div class="bg-surface-raised rounded-xl p-4 border border-border">
              <div class="flex items-center gap-3 mb-3">
                <div class="flex-1 font-mono text-sm bg-surface rounded-lg px-4 py-2.5 border border-border text-fg">
                  {showKey && streamKey ? streamKey : info?.stream_key_masked ?? '****'}
                </div>
                <button
                  onClick={handleRevealKey}
                  class="px-3 py-2.5 text-xs rounded-lg bg-surface-hover hover:bg-surface-active border border-border transition-colors text-fg-secondary whitespace-nowrap"
                >
                  {showKey ? 'Hide' : 'Reveal'}
                </button>
                <button
                  onClick={() => {
                    const key = streamKey ?? info?.stream_key_masked ?? '';
                    copyToClipboard(key, 'key');
                  }}
                  class="px-3 py-2.5 text-xs rounded-lg bg-surface-hover hover:bg-surface-active border border-border transition-colors text-fg-secondary whitespace-nowrap"
                >
                  {copied === 'key' ? 'Copied!' : 'Copy'}
                </button>
              </div>
              <button
                onClick={handleResetKey}
                disabled={resetting}
                class="w-full px-4 py-2 text-sm rounded-lg border text-danger disabled:opacity-50 transition-colors"
                style={{ backgroundColor: 'var(--danger-bg)', borderColor: 'var(--danger)' }}
              >
                {resetting ? 'Resetting…' : 'Reset Stream Key'}
              </button>
              <p class="text-xs text-fg-faint mt-2">
                Resetting generates a new key. Update your streaming software immediately.
              </p>
            </div>
          </div>

          <div>
            <h3 class="text-sm font-semibold text-fg-muted uppercase tracking-wider mb-3">
              Server Endpoints
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
                    {copied === ep.label ? 'Copied!' : 'Copy'}
                  </button>
                </div>
              ))}
            </div>
          </div>

          <div>
            <h3 class="text-sm font-semibold text-fg-muted uppercase tracking-wider mb-3">
              Quick Setup (OBS / Streamlabs)
            </h3>
            <div class="bg-surface-raised rounded-xl p-4 border border-border space-y-3">
              {[
                'Open OBS → Settings → Stream',
                'Service: Custom',
              ].map((text, i) => (
                <div key={i} class="flex items-start gap-3">
                  <span class="shrink-0 w-6 h-6 rounded-full bg-accent text-white text-xs flex items-center justify-center font-bold">{i + 1}</span>
                  <div class="text-sm text-fg">{text}</div>
                </div>
              ))}
              <div class="flex items-start gap-3">
                <span class="shrink-0 w-6 h-6 rounded-full bg-accent text-white text-xs flex items-center justify-center font-bold">3</span>
                <div class="text-sm text-fg">
                  Server: <code class="text-accent bg-surface px-1.5 py-0.5 rounded text-xs">{info?.rtmp_url ?? 'rtmp://localhost:1935'}</code>
                </div>
              </div>
              <div class="flex items-start gap-3">
                <span class="shrink-0 w-6 h-6 rounded-full bg-accent text-white text-xs flex items-center justify-center font-bold">4</span>
                <div class="text-sm text-fg">
                  Stream Key: <code class="text-accent bg-surface px-1.5 py-0.5 rounded text-xs">{showKey && streamKey ? streamKey : info?.stream_key_masked ?? '****'}</code>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
