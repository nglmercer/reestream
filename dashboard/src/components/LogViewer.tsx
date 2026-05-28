import { useState, useCallback } from 'preact/hooks';

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
  info: 'text-sky-400',
  warn: 'text-amber-400',
  error: 'text-red-400',
};

export function LogViewer({ logs, onClear }: Props) {
  return (
    <div class="bg-slate-900 border border-slate-800 rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-slate-800">
        <h2 class="text-base font-semibold">Logs</h2>
        <button
          onClick={onClear}
          class="px-3 py-1.5 text-sm rounded-lg bg-slate-800 border border-slate-700 hover:bg-slate-700 transition-colors"
        >
          Clear
        </button>
      </div>
      <div class="p-4 max-h-72 overflow-y-auto font-mono text-xs bg-slate-950">
        {logs.length === 0 ? (
          <div class="text-slate-500 text-center py-4">No logs</div>
        ) : (
          logs.map((l, i) => (
            <div key={i} class="py-0.5 border-b border-slate-900">
              <span class="text-slate-500">[{l.time}]</span>{' '}
              <span class={levelColor[l.level] ?? 'text-slate-300'}>{l.message}</span>
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
