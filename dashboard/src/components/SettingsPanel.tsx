import { useState, useEffect, useCallback } from 'preact/hooks';

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

  useEffect(() => {
    Promise.all([
      fetch('/api/setup/info').then((r) => r.json()),
    ])
      .then(([infoRes]) => {
        if (infoRes.success) setInfo(infoRes.data);
      })
      .catch(() => addLog('Failed to load server info', 'error'))
      .finally(() => setLoading(false));
  }, [addLog]);

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
      setCopied(label);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      // Fallback
      const ta = document.createElement('textarea');
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand('copy');
      document.body.removeChild(ta);
      setCopied(label);
      setTimeout(() => setCopied(null), 1500);
    }
  }, []);

  if (loading) {
    return (
      <div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50">
        <div class="bg-slate-900 border border-slate-800 rounded-2xl p-8">
          <div class="text-slate-400 animate-pulse">Loading settings…</div>
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
    <div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4" onClick={onClose}>
      <div
        class="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-2xl max-h-[85vh] overflow-y-auto"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div class="flex items-center justify-between px-6 py-4 border-b border-slate-800 sticky top-0 bg-slate-900 z-10">
          <h2 class="text-lg font-bold">Settings</h2>
          <button
            onClick={onClose}
            class="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-slate-800 text-slate-400 hover:text-slate-200 transition-colors"
          >
            <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="p-6 space-y-6">
          {/* Stream Key Section */}
          <div>
            <h3 class="text-sm font-semibold text-slate-400 uppercase tracking-wider mb-3">Stream Key</h3>
            <div class="bg-slate-800 rounded-xl p-4 border border-slate-700">
              <div class="flex items-center gap-3 mb-3">
                <div class="flex-1 font-mono text-sm bg-slate-950 rounded-lg px-4 py-2.5 border border-slate-700">
                  {showKey && streamKey ? streamKey : info?.stream_key_masked ?? '****'}
                </div>
                <button
                  onClick={handleRevealKey}
                  class="px-3 py-2.5 text-xs rounded-lg bg-slate-700 hover:bg-slate-600 transition-colors whitespace-nowrap"
                >
                  {showKey ? 'Hide' : 'Reveal'}
                </button>
                <button
                  onClick={() => {
                    const key = streamKey ?? info?.stream_key_masked ?? '';
                    copyToClipboard(key, 'key');
                  }}
                  class="px-3 py-2.5 text-xs rounded-lg bg-slate-700 hover:bg-slate-600 transition-colors whitespace-nowrap"
                >
                  {copied === 'key' ? 'Copied!' : 'Copy'}
                </button>
              </div>
              <button
                onClick={handleResetKey}
                disabled={resetting}
                class="w-full px-4 py-2 text-sm rounded-lg bg-red-900/30 border border-red-800/50 text-red-400 hover:bg-red-900/50 disabled:opacity-50 transition-colors"
              >
                {resetting ? 'Resetting…' : 'Reset Stream Key'}
              </button>
              <p class="text-xs text-slate-500 mt-2">
                Resetting generates a new key. Update your streaming software immediately.
              </p>
            </div>
          </div>

          {/* Endpoints Section */}
          <div>
            <h3 class="text-sm font-semibold text-slate-400 uppercase tracking-wider mb-3">
              Server Endpoints
            </h3>
            <div class="space-y-2">
              {endpoints.filter((ep) => ep.value != null).map((ep) => (
                <div
                  key={ep.label}
                  class="bg-slate-800 rounded-lg px-4 py-3 border border-slate-700 flex items-center justify-between gap-3"
                >
                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <span class="text-sm font-medium text-slate-200">{ep.label}</span>
                      <span class="text-xs text-slate-500">{ep.note}</span>
                    </div>
                    <div class="font-mono text-xs text-sky-400 truncate mt-0.5">{ep.value}</div>
                  </div>
                  <button
                    onClick={() => copyToClipboard(ep.value!, ep.label)}
                    class="shrink-0 px-2 py-1 text-xs rounded bg-slate-700 hover:bg-slate-600 transition-colors"
                  >
                    {copied === ep.label ? 'Copied!' : 'Copy'}
                  </button>
                </div>
              ))}
            </div>
          </div>

          {/* OBS Instructions */}
          <div>
            <h3 class="text-sm font-semibold text-slate-400 uppercase tracking-wider mb-3">
              Quick Setup (OBS / Streamlabs)
            </h3>
            <div class="bg-slate-800 rounded-xl p-4 border border-slate-700 space-y-3">
              <div class="flex items-start gap-3">
                <span class="shrink-0 w-6 h-6 rounded-full bg-sky-600 text-white text-xs flex items-center justify-center font-bold">1</span>
                <div>
                  <div class="text-sm text-slate-200">Open OBS → Settings → Stream</div>
                </div>
              </div>
              <div class="flex items-start gap-3">
                <span class="shrink-0 w-6 h-6 rounded-full bg-sky-600 text-white text-xs flex items-center justify-center font-bold">2</span>
                <div>
                  <div class="text-sm text-slate-200">
                    Service: <span class="text-slate-400">Custom</span>
                  </div>
                </div>
              </div>
              <div class="flex items-start gap-3">
                <span class="shrink-0 w-6 h-6 rounded-full bg-sky-600 text-white text-xs flex items-center justify-center font-bold">3</span>
                <div>
                  <div class="text-sm text-slate-200">
                    Server: <code class="text-sky-400 bg-slate-900 px-1.5 py-0.5 rounded text-xs">{info?.rtmp_url ?? 'rtmp://localhost:1935'}</code>
                  </div>
                </div>
              </div>
              <div class="flex items-start gap-3">
                <span class="shrink-0 w-6 h-6 rounded-full bg-sky-600 text-white text-xs flex items-center justify-center font-bold">4</span>
                <div>
                  <div class="text-sm text-slate-200">
                    Stream Key: <code class="text-sky-400 bg-slate-900 px-1.5 py-0.5 rounded text-xs">{showKey && streamKey ? streamKey : info?.stream_key_masked ?? '****'}</code>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
