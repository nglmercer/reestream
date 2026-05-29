import { useRef, useEffect, useState, useCallback } from 'preact/hooks';
import type { RefObject } from 'preact';

type PlayerType = 'flv' | 'hls' | 'native';

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
  const flvPlayerRef = useRef<{ destroy?: () => void } | null>(null);
  const hlsPlayerRef = useRef<{ destroy?: () => void } | null>(null);

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !opts.url) return;

    let destroyed = false;
    setError(null);

    const isFlv = opts.url.endsWith('.flv');
    const isHls = opts.url.endsWith('.m3u8');

    async function initFlv() {
      try {
        const flvjs = await import('flv.js');
        if (destroyed) return;

        if (!flvjs.default.isSupported()) {
          setError('FLV.js not supported in this browser');
          return;
        }

        const player = flvjs.default.createPlayer(
          {
            type: 'flv',
            url: opts.url,
            isLive: true,
          },
          {
            enableWorker: true,
            enableStashBuffer: false,
            stashInitialSize: 128,
            lazyLoad: false,
            lazyLoadMaxDuration: 0.2,
            deferLoadAfterSourceOpen: false,
            autoCleanupSourceBuffer: true,
            autoCleanupMaxBackwardDuration: 3,
            autoCleanupMinBackwardDuration: 1,
            fixAudioTimestampGap: true,
            seekType: 'range',
          },
        );

        player.attachMediaElement(video!);
        player.load();

        if (opts.autoplay !== false) {
          try {
            await video!.play();
            setPlaying(true);
          } catch {
            video!.muted = true;
            await video!.play().catch(() => {});
            setPlaying(true);
          }
        }

        flvPlayerRef.current = player;
        setPlayerType('flv');
      } catch (e) {
        if (!destroyed) setError(`FLV init failed: ${e}`);
      }
    }

    async function initHls() {
      try {
        const Hls = (await import('hls.js')).default;
        if (destroyed) return;

        if (Hls.isSupported()) {
          const hls = new Hls({
            lowLatencyMode: opts.lowLatency !== false,
            liveSyncDurationCount: 3,
            liveMaxLatencyDurationCount: 6,
            enableWorker: true,
          });

          hls.loadSource(opts.url);
          hls.attachMedia(video!);

          hls.on(Hls.Events.MANIFEST_PARSED, () => {
            if (opts.autoplay !== false) {
              video!.play().catch(() => {
                video!.muted = true;
                video!.play().catch(() => {});
              });
            }
          });

          hls.on(Hls.Events.ERROR, (_event, data) => {
            if (data.fatal && !destroyed) {
              setError(`HLS error: ${data.type} - ${data.details}`);
            }
          });

          hlsPlayerRef.current = hls;
          setPlayerType('hls');
        } else if (video!.canPlayType('application/vnd.apple.mpegurl')) {
          video!.src = opts.url;
          video!.load();
          if (opts.autoplay !== false) {
            video!.play().catch(() => {});
          }
          setPlayerType('native');
        } else {
          setError('HLS not supported in this browser');
        }
      } catch (e) {
        if (!destroyed) setError(`HLS init failed: ${e}`);
      }
    }

    if (isFlv) {
      initFlv();
    } else if (isHls) {
      initHls();
    } else {
      video!.src = opts.url;
      video!.load();
      if (opts.autoplay !== false) {
        video!.play().catch(() => {});
      }
      setPlayerType('native');
    }

    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onError = () => setError(`Video error: ${video!.error?.message ?? 'unknown'}`);

    video!.addEventListener('play', onPlay);
    video!.addEventListener('pause', onPause);
    video!.addEventListener('error', onError);

    const interval = setInterval(() => {
      if (destroyed || !video!.buffered.length) return;
      const behind = video!.buffered.end(video!.buffered.length - 1) - video!.currentTime;
      setLatency(Math.max(0, behind));
    }, 500);

    return () => {
      destroyed = true;
      clearInterval(interval);
      video!.removeEventListener('play', onPlay);
      video!.removeEventListener('pause', onPause);
      video!.removeEventListener('error', onError);
      video!.pause();
      video!.src = '';

      if (flvPlayerRef.current && typeof flvPlayerRef.current.destroy === 'function') {
        flvPlayerRef.current.destroy();
        flvPlayerRef.current = null;
      }
      if (hlsPlayerRef.current && typeof hlsPlayerRef.current.destroy === 'function') {
        hlsPlayerRef.current.destroy();
        hlsPlayerRef.current = null;
      }
    };
  }, [opts.url, opts.autoplay]);

  const play = useCallback(() => {
    videoRef.current?.play().catch(() => {});
  }, []);

  const pause = useCallback(() => {
    videoRef.current?.pause();
  }, []);

  const toggle = useCallback(() => {
    if (playing) pause();
    else play();
  }, [playing, play, pause]);

  return { videoRef, playing, error, latency, playerType, play, pause, toggle };
}
