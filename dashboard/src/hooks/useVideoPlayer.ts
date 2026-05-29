import { useRef, useEffect, useState, useCallback } from 'preact/hooks';
import type { RefObject } from 'preact';
import { useLocale } from './useLocale';

type PlayerType = 'flv' | 'hls' | 'native';

interface FlvPlayer {
  attachMediaElement(el: HTMLMediaElement): void;
  load(): void;
  unload(): void;
  detachMediaElement(): void;
  destroy(): void;
}

interface FlvModule {
  isSupported(): boolean;
  createPlayer(
    mediaDataSource: { type: string; isLive: boolean; url: string },
    config?: Record<string, unknown>,
  ): FlvPlayer;
}

interface HlsPlayer {
  loadSource(url: string): void;
  attachMedia(el: HTMLMediaElement): void;
  destroy(): void;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  on(event: string, callback: (...args: any[]) => void): void;
}

interface HlsModule {
  isSupported(): boolean;
  Events: { MANIFEST_PARSED: string; ERROR: string };
  new (config?: Record<string, unknown>): HlsPlayer;
}

interface UsePlayerOptions {
  url: string;
  autoplay?: boolean;
  muted?: boolean;
  lowLatency?: boolean;
}

interface UsePlayerReturn {
  videoRef: RefObject<HTMLVideoElement>;
  playing: boolean;
  error: string | null;
  latency: number;
  playerType: PlayerType;
  play: () => void;
  pause: () => void;
  toggle: () => void;
}

export function useVideoPlayer(opts: UsePlayerOptions): UsePlayerReturn {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [playing, setPlaying] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [latency, setLatency] = useState(0);
  const [playerType, setPlayerType] = useState<PlayerType>('native');
  const flvPlayerRef = useRef<FlvPlayer | null>(null);
  const { t } = useLocale();
  const hlsPlayerRef = useRef<HlsPlayer | null>(null);
  const initIdRef = useRef(0);

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !opts.url) return;

    const el = video;
    const currentInitId = ++initIdRef.current;
    setError(null);
    setLatency(0);
    setPlaying(false);

    const isFlv = opts.url.endsWith('.flv');
    const isHls = opts.url.endsWith('.m3u8');

    function destroyFlv() {
      if (flvPlayerRef.current) {
        try {
          if (typeof flvPlayerRef.current.detachMediaElement === 'function') {
            flvPlayerRef.current.detachMediaElement();
          }
          if (typeof flvPlayerRef.current.destroy === 'function') {
            flvPlayerRef.current.destroy();
          }
        } catch {}
        flvPlayerRef.current = null;
      }
    }

    function destroyHls() {
      if (hlsPlayerRef.current) {
        try {
          if (typeof hlsPlayerRef.current.destroy === 'function') {
            hlsPlayerRef.current.destroy();
          }
        } catch {}
        hlsPlayerRef.current = null;
      }
    }

    function resetVideo(): Promise<void> {
      return new Promise((resolve) => {
        el.pause();
        el.removeAttribute('src');
        while (el.firstChild) {
          el.removeChild(el.firstChild);
        }
        const onEmptied = () => {
          el.removeEventListener('emptied', onEmptied);
          resolve();
        };
        el.addEventListener('emptied', onEmptied);
        el.load();
      });
    }

    async function initFlv() {
      try {
        const flvjs = await import('flv.js');
        if (currentInitId !== initIdRef.current) return;

        const flvModule = (flvjs.default || flvjs) as FlvModule;
        if (!flvModule.isSupported()) {
          setError(t('error.flvNotSupported'));
          return;
        }

        const player = flvModule.createPlayer(
          {
            type: 'flv',
            isLive: true,
            url: opts.url,
          },
          {
            enableWorker: false,
            enableStashBuffer: false,
            stashInitialSize: 128,
            lazyLoad: false,
            lazyLoadMaxDuration: 0.2,
            deferLoadAfterSourceOpen: false,
            autoCleanupSourceBuffer: true,
            autoCleanupMaxBackwardDuration: 3,
            autoCleanupMinBackwardDuration: 1,
            fixAudioTimestampGap: true,
            seekType: 'param',
          },
        );

        if (currentInitId !== initIdRef.current) {
          try { player.destroy(); } catch {}
          return;
        }

        player.attachMediaElement(el);
        player.load();

        if (opts.autoplay !== false) {
          try {
            await el.play();
            setPlaying(true);
          } catch {
            el.muted = true;
            await el.play().catch(() => {});
            setPlaying(true);
          }
        }

        flvPlayerRef.current = player;
        setPlayerType('flv');
      } catch (e) {
        if (currentInitId === initIdRef.current) {
          setError(t('error.flvInitFailed', { error: String(e) }));
        }
      }
    }

    async function initHls() {
      try {
        const Hls = (await import('hls.js')).default as HlsModule;
        if (currentInitId !== initIdRef.current) return;

        if (Hls.isSupported()) {
          const hls = new Hls({
            lowLatencyMode: opts.lowLatency !== false,
            liveSyncDurationCount: 3,
            liveMaxLatencyDurationCount: 6,
            enableWorker: true,
          });

          if (currentInitId !== initIdRef.current) {
            try { hls.destroy(); } catch {}
            return;
          }

          hls.loadSource(opts.url);
          hls.attachMedia(el);

          hls.on(Hls.Events.MANIFEST_PARSED, () => {
            if (currentInitId !== initIdRef.current) return;
            if (opts.autoplay !== false) {
              el.play().catch(() => {
                el.muted = true;
                el.play().catch(() => {});
              });
            }
          });

          hls.on(Hls.Events.ERROR, (_event: unknown, data: { fatal: boolean; type: string; details: string }) => {
            if (data.fatal && currentInitId === initIdRef.current) {
              setError(t('error.hlsError', { type: data.type, details: data.details }));
            }
          });

          hlsPlayerRef.current = hls;
          setPlayerType('hls');
        } else if (el.canPlayType('application/vnd.apple.mpegurl')) {
          el.src = opts.url;
          el.load();
          if (opts.autoplay !== false) {
            el.play().catch(() => {});
          }
          setPlayerType('native');
        } else {
          setError(t('error.hlsNotSupported'));
        }
      } catch (e) {
        if (currentInitId === initIdRef.current) {
          setError(t('error.hlsInitFailed', { error: String(e) }));
        }
      }
    }

    (async () => {
      destroyFlv();
      destroyHls();
      await resetVideo();

      if (currentInitId !== initIdRef.current) return;

      requestAnimationFrame(() => {
        if (currentInitId !== initIdRef.current) return;

        if (isFlv) {
          initFlv();
        } else if (isHls) {
          initHls();
        } else {
          el.src = opts.url;
          el.load();
          if (opts.autoplay !== false) {
            el.play().catch(() => {});
          }
          setPlayerType('native');
        }
      });
    })();

    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onError = () => setError(t('error.videoError', { message: el.error?.message ?? t('error.unknown') }));

    el.addEventListener('play', onPlay);
    el.addEventListener('pause', onPause);
    el.addEventListener('error', onError);

    const interval = setInterval(() => {
      if (currentInitId !== initIdRef.current || !el.buffered.length) return;
      const behind = el.buffered.end(el.buffered.length - 1) - el.currentTime;
      setLatency(Math.max(0, behind));
    }, 500);

    return () => {
      initIdRef.current++;
      clearInterval(interval);
      el.removeEventListener('play', onPlay);
      el.removeEventListener('pause', onPause);
      el.removeEventListener('error', onError);
      destroyFlv();
      destroyHls();
      el.pause();
      el.removeAttribute('src');
      while (el.firstChild) {
        el.removeChild(el.firstChild);
      }
      el.load();
    };
  }, [opts.url, opts.autoplay, t]);

  const play = useCallback(() => {
    videoRef.current?.play().catch(() => {});
  }, []);

  const pause = useCallback(() => {
    videoRef.current?.pause();
  }, []);

  const playingRef = useRef(playing);
  playingRef.current = playing;

  const toggle = useCallback(() => {
    if (playingRef.current) pause();
    else play();
  }, [play, pause]);

  return { videoRef, playing, error, latency, playerType, play, pause, toggle };
}
