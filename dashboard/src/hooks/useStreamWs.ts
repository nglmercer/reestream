import { useState, useEffect, useRef } from 'preact/hooks';
import type { StreamInfo } from '../api';

interface StreamEvent {
  type: 'init' | 'event';
  streams?: StreamInfo[];
  event?: {
    Started?: { id: string; name: string; input_url: string };
    Stopped?: { id: string };
    Updated?: { id: string; viewers: number; bitrate: number };
    Error?: { id: string; message: string };
  };
}

interface UseStreamWsOptions {
  onInit?: (streams: StreamInfo[]) => void;
  onStarted?: (id: string, name: string, input_url: string) => void;
  onStopped?: (id: string) => void;
  onUpdated?: (id: string, viewers: number, bitrate: number) => void;
  onError?: (id: string, message: string) => void;
  reconnectMs?: number;
}

export function useStreamWs(opts: UseStreamWsOptions) {
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const optsRef = useRef(opts);
  optsRef.current = opts;

  useEffect(() => {
    let destroyed = false;

    function connect() {
      if (destroyed) return;

      const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${proto}//${window.location.host}/ws/streams`);
      wsRef.current = ws;

      ws.onopen = () => {
        if (!destroyed) setConnected(true);
      };

      ws.onmessage = (ev) => {
        try {
          const data: StreamEvent = JSON.parse(ev.data);
          const o = optsRef.current;

          if (data.type === 'init' && data.streams && o.onInit) {
            o.onInit(data.streams);
          } else if (data.type === 'event' && data.event) {
            const e = data.event;
            if (e.Started && o.onStarted) {
              o.onStarted(e.Started.id, e.Started.name, e.Started.input_url);
            }
            if (e.Stopped && o.onStopped) {
              o.onStopped(e.Stopped.id);
            }
            if (e.Updated && o.onUpdated) {
              o.onUpdated(e.Updated.id, e.Updated.viewers, e.Updated.bitrate);
            }
            if (e.Error && o.onError) {
              o.onError(e.Error.id, e.Error.message);
            }
          }
        } catch {
          // ignore parse errors
        }
      };

      ws.onclose = () => {
        if (!destroyed) {
          setConnected(false);
          const delay = optsRef.current.reconnectMs ?? 3000;
          reconnectRef.current = setTimeout(connect, delay);
        }
      };

      ws.onerror = () => {
        ws.close();
      };
    }

    connect();

    return () => {
      destroyed = true;
      if (reconnectRef.current) clearTimeout(reconnectRef.current);
      if (wsRef.current) wsRef.current.close();
    };
  }, []);

  return { connected };
}
