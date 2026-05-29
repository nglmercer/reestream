import { useState, useEffect, useCallback } from 'preact/hooks';
import { api } from '../api';

interface Recording {
  id: string;
  stream_id: string;
  filename: string;
  format: string;
  started_at: number;
  size_bytes: number;
  status: string;
}

interface Props {
  addLog: (msg: string, level?: 'info' | 'warn' | 'error') => void;
}

export function RecordingControls({ addLog }: Props) {
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [loading, setLoading] = useState(true);
  const [recording, setRecording] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const res = await api.getRecordings();
      if (res.success && res.data) setRecordings(res.data as Recording[]);
    } catch {
      // ignore
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, 10_000);
    return () => clearInterval(id);
  }, [refresh]);

  const handleStart = useCallback(async () => {
    setRecording(true);
    try {
      const res = await api.startRecording('live', 'rtmp://0.0.0.0:1935/live');
      if (res.success) {
        addLog(`Recording started: ${res.data}`);
        refresh();
      } else {
        addLog(`Recording failed: ${res.error}`, 'error');
      }
    } catch (e) {
      addLog(`Recording error: ${e}`, 'error');
    } finally {
      setRecording(false);
    }
  }, [addLog, refresh]);

  const handleStop = useCallback(
    async (id: string) => {
      const res = await api.stopRecording(id);
      if (res.success) {
        addLog('Recording stopped');
        refresh();
      } else {
        addLog(`Stop failed: ${res.error}`, 'error');
      }
    },
    [addLog, refresh],
  );

  const handleDelete = useCallback(
    async (id: string) => {
      if (!confirm('Delete this recording file?')) return;
      const res = await api.deleteRecording(id);
      if (res.success) {
        addLog('Recording deleted');
        refresh();
      } else {
        addLog(`Delete failed: ${res.error}`, 'error');
      }
    },
    [addLog, refresh],
  );

  const formatSize = (bytes: number): string => {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1048576).toFixed(1)} MB`;
  };

  const formatDuration = (startedAt: number): string => {
    const secs = Math.floor(Date.now() / 1000) - startedAt;
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
    return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`;
  };

  const activeRecordings = recordings.filter((r) => r.status === 'recording');
  const pastRecordings = recordings.filter((r) => r.status !== 'recording');

  return (
    <div class="bg-surface-alt border border-border rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-border">
        <h2 class="text-base font-semibold text-fg">Recordings</h2>
        <div class="flex items-center gap-2">
          <button
            onClick={refresh}
            class="px-3 py-1.5 text-sm rounded-lg bg-surface-hover border border-border hover:bg-surface-active transition-colors text-fg-secondary"
          >
            Refresh
          </button>
          <button
            onClick={handleStart}
            disabled={recording}
            class="px-3 py-1.5 text-sm rounded-lg bg-danger hover:opacity-90 disabled:bg-surface-active disabled:text-fg-faint text-white transition-colors flex items-center gap-1.5"
          >
            <span class="w-2 h-2 rounded-full bg-white animate-pulse" style={{ display: recording ? 'none' : 'block' }} />
            {recording ? 'Starting…' : 'Record'}
          </button>
        </div>
      </div>

      <div class="p-4">
        {activeRecordings.length > 0 && (
          <div class="mb-4">
            <div class="text-xs text-fg-faint uppercase tracking-wider mb-2">Active</div>
            {activeRecordings.map((r) => (
              <div
                key={r.id}
                class="flex items-center justify-between rounded-lg px-4 py-3 mb-2 border"
                style={{ backgroundColor: 'var(--danger-bg)', borderColor: 'var(--danger)' }}
              >
                <div class="flex items-center gap-3">
                  <span class="w-2 h-2 rounded-full bg-danger animate-pulse" />
                  <div>
                    <div class="text-sm text-fg">{r.filename}</div>
                    <div class="text-xs text-fg-faint">
                      {formatDuration(r.started_at)} · {r.format.toUpperCase()}
                    </div>
                  </div>
                </div>
                <button
                  onClick={() => handleStop(r.id)}
                  class="px-3 py-1 text-xs rounded bg-danger hover:opacity-90 text-white transition-colors"
                >
                  Stop
                </button>
              </div>
            ))}
          </div>
        )}

        {pastRecordings.length > 0 && (
          <div>
            <div class="text-xs text-fg-faint uppercase tracking-wider mb-2">History</div>
            <div class="space-y-1 max-h-48 overflow-y-auto">
              {pastRecordings.map((r) => (
                <div
                  key={r.id}
                  class="flex items-center justify-between bg-surface-raised rounded-lg px-4 py-2 border border-border"
                >
                  <div>
                    <div class="text-sm text-fg-secondary">{r.filename}</div>
                    <div class="text-xs text-fg-faint">
                      {r.status} · {r.format.toUpperCase()} · {formatSize(r.size_bytes)}
                    </div>
                  </div>
                  <button
                    onClick={() => handleDelete(r.id)}
                    class="px-2 py-1 text-xs rounded text-danger hover:bg-danger-bg transition-colors"
                  >
                    Delete
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        {!loading && recordings.length === 0 && (
          <div class="text-center py-6 text-fg-faint text-sm">
            No recordings. Click "Record" to start capturing the stream.
          </div>
        )}

        {loading && (
          <div class="text-center py-6 text-fg-faint text-sm animate-pulse">Loading…</div>
        )}
      </div>
    </div>
  );
}
