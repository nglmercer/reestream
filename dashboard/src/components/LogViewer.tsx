import { useState, useCallback } from 'preact/hooks';
import { useLocale } from '../hooks/useLocale';

interface LogEntry {
  time: string;
  message: string;
  level: 'info' | 'warn' | 'error';
}

interface Props {
  logs: LogEntry[];
  onClear: () => void;
}

const levelColor: Record<string, string> = {
  info: 'text-accent',
  warn: 'text-warning',
  error: 'text-danger',
};

export function LogViewer({ logs, onClear }: Props) {
  const { t } = useLocale();

  return (
    <div class="bg-surface-alt border border-border rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-border">
        <h2 class="text-base font-semibold text-fg">{t('logs.title')}</h2>
        <button
          onClick={onClear}
          class="px-3 py-1.5 text-sm rounded-lg bg-surface-hover border border-border hover:bg-surface-active transition-colors text-fg-secondary"
        >
          {t('logs.clear')}
        </button>
      </div>
      <div class="p-4 max-h-72 overflow-y-auto font-mono text-xs bg-surface">
        {logs.length === 0 ? (
          <div class="text-fg-faint text-center py-4">{t('logs.empty')}</div>
        ) : (
          logs.map((l, i) => (
            <div key={i} class="py-0.5 border-b border-border">
              <span class="text-fg-faint">[{l.time}]</span>{' '}
              <span class={levelColor[l.level] ?? 'text-fg-secondary'}>{l.message}</span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

export function useLogger() {
  const [logs, setLogs] = useState<LogEntry[]>([]);

  const addLog = useCallback((message: string, level: LogEntry['level'] = 'info') => {
    const time = new Date().toLocaleTimeString();
    setLogs((prev) => [...prev.slice(-199), { time, message, level }]);
  }, []);

  const clearLogs = useCallback(() => setLogs([]), []);

  return { logs, addLog, clearLogs };
}
