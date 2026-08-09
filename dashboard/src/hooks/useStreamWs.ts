import { useState, useEffect, useRef } from 'preact/hooks';
import { apiV1 } from '../api';
import type { Event } from '../api';

interface StreamingFrame {
  type: 'init' | 'event';
  events?: Event[];
  notification?: {
    event: string;
    eventId: string;
    payload: Event;
    timestamp: number;
  };
}

interface UseStreamWsOptions {
  onInit?: (events: Event[]) => void;
  onEvent?: (event: Event, eventName: string) => void;
  reconnectMs?: number;
}

/** Subscribe to the versioned event stream and reconcile live events in the UI. */
export function useStreamWs(opts: UseStreamWsOptions, enabled = true) {
  const [connected, setConnected] = useState(false);
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const optsRef = useRef(opts);
  optsRef.current = opts;

  useEffect(() => {
    let destroyed = false;

    if (!enabled) {
      setConnected(false);
      return () => {
        destroyed = true;
        if (reconnectRef.current) clearTimeout(reconnectRef.current);
        wsRef.current?.close();
        wsRef.current = null;
      };
    }

    function connect() {
      if (destroyed) return;

      const ws = apiV1.openStreamingSocket();
      wsRef.current = ws;

      ws.onopen = () => {
        if (!destroyed) setConnected(true);
      };

      ws.onmessage = (message) => {
        try {
          const frame = JSON.parse(message.data) as StreamingFrame;
          const current = optsRef.current;
          if (frame.type === 'init' && frame.events) {
            current.onInit?.(frame.events);
          } else if (frame.type === 'event' && frame.notification?.payload) {
            current.onEvent?.(frame.notification.payload, frame.notification.event);
          }
        } catch {
          // Ignore malformed or provider-specific frames and keep the socket alive.
        }
      };

      ws.onclose = () => {
        if (!destroyed) {
          setConnected(false);
          reconnectRef.current = setTimeout(
            connect,
            optsRef.current.reconnectMs ?? 3_000,
          );
        }
      };

      ws.onerror = () => ws.close();
    }

    connect();

    return () => {
      destroyed = true;
      if (reconnectRef.current) clearTimeout(reconnectRef.current);
      wsRef.current?.close();
    };
  }, [enabled]);

  return { connected };
}
