import { useState, useEffect, useCallback } from 'preact/hooks';
import { apiV1 } from '../api';
import type { ManualRecording } from '../api';
import { useLocale } from '../hooks/useLocale';

interface Props {
  addLog: (msg: string, level?: 'info' | 'warn' | 'error') => void;
}

export function RecordingControls({ addLog }: Props) {
  const { t } = useLocale();
  const [recordings, setRecordings] = useState<ManualRecording[]>([]);
  const [loading, setLoading] = useState(true);
  const [recording, setRecording] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setRecordings(await apiV1.getManualRecordings());
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
      const result = await apiV1.startManualRecording('live', 'rtmp://localhost:1935/live');
      addLog(t('log.recordingStarted', { id: result.id }));
      refresh();
    } catch (e) {
      addLog(t('log.recordingError', { error: String(e) }), 'error');
    } finally {
      setRecording(false);
    }
  }, [addLog, refresh]);

  const handleStop = useCallback(
    async (id: string) => {
      try {
        await apiV1.stopManualRecording(id);
        addLog(t('log.recordingStopped'));
        refresh();
      } catch (error) {
        addLog(t('log.stopFailed', { error: error instanceof Error ? error.message : String(error) }), 'error');
      }
    },
    [addLog, refresh],
  );

  const handleDelete = useCallback(
    async (id: string) => {
      if (!confirm(t('recording.confirmDelete'))) return;
      try {
        await apiV1.deleteManualRecording(id);
        addLog(t('log.recordingDeleted'));
        refresh();
      } catch (error) {
        addLog(t('log.deleteFailed', { error: error instanceof Error ? error.message : String(error) }), 'error');
      }
    },
    [addLog, refresh],
  );

  const formatSize = (bytes: number): string => {
    const units = [' B', ' KB', ' MB'];
    if (bytes < 1024) return `${bytes}${units[0]}`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)}${units[1]}`;
    return `${(bytes / 1048576).toFixed(1)}${units[2]}`;
  };

  const formatDuration = (startedAt: number): string => {
    const secs = Math.floor(Date.now() / 1000) - startedAt;
    if (secs < 60) return t('time.seconds', { s: secs });
    if (secs < 3600) return t('time.minutesSeconds', { m: Math.floor(secs / 60), s: secs % 60 });
    return t('time.hoursMinutes', { h: Math.floor(secs / 3600), m: Math.floor((secs % 3600) / 60) });
  };

  const activeRecordings = recordings.filter((r) => r.status === 'recording');
  const pastRecordings = recordings.filter((r) => r.status !== 'recording');

  return (
    <div class="bg-surface-alt border border-border rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-border">
        <h2 class="text-base font-semibold text-fg">{t('recording.title')}</h2>
        <div class="flex items-center gap-2">
          <button
            onClick={refresh}
            class="px-3 py-1.5 text-sm rounded-lg bg-surface-hover border border-border hover:bg-surface-active transition-colors text-fg-secondary"
          >
            {t('recording.refresh')}
          </button>
          <button
            onClick={handleStart}
            disabled={recording}
            class="px-3 py-1.5 text-sm rounded-lg bg-danger hover:opacity-90 disabled:bg-surface-active disabled:text-fg-faint text-white transition-colors flex items-center gap-1.5"
          >
            <span class="w-2 h-2 rounded-full bg-white animate-pulse" style={{ display: recording ? 'none' : 'block' }} />
            {recording ? t('recording.starting') : t('recording.record')}
          </button>
        </div>
      </div>

      <div class="p-4">
        {activeRecordings.length > 0 && (
          <div class="mb-4">
            <div class="text-xs text-fg-faint uppercase tracking-wider mb-2">{t('recording.active')}</div>
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
                      {formatDuration(r.startedAt)} · {r.format.toUpperCase()}
                    </div>
                  </div>
                </div>
                <button
                  onClick={() => handleStop(r.id)}
                  class="px-3 py-1 text-xs rounded bg-danger hover:opacity-90 text-white transition-colors"
                >
                  {t('recording.stop')}
                </button>
              </div>
            ))}
          </div>
        )}

        {pastRecordings.length > 0 && (
          <div>
            <div class="text-xs text-fg-faint uppercase tracking-wider mb-2">{t('recording.history')}</div>
            <div class="space-y-1 max-h-48 overflow-y-auto">
              {pastRecordings.map((r) => (
                <div
                  key={r.id}
                  class="flex items-center justify-between bg-surface-raised rounded-lg px-4 py-2 border border-border"
                >
                  <div>
                    <div class="text-sm text-fg-secondary">{r.filename}</div>
                    <div class="text-xs text-fg-faint">
                      {r.status} · {r.format.toUpperCase()} · {formatSize(r.sizeBytes)}
                    </div>
                  </div>
                  <button
                    onClick={() => handleDelete(r.id)}
                    class="px-2 py-1 text-xs rounded text-danger hover:bg-danger-bg transition-colors"
                  >
                    {t('recording.delete')}
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        {!loading && recordings.length === 0 && (
          <div class="text-center py-6 text-fg-faint text-sm">
            {t('recording.empty')}
          </div>
        )}

        {loading && (
          <div class="text-center py-6 text-fg-faint text-sm animate-pulse">{t('recording.loading')}</div>
        )}
      </div>
    </div>
  );
}
