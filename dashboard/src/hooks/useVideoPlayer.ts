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
  const flvPlayerRef = useRef<any>(null);
  const hlsPlayerRef = useRef<any>(null);
  const initIdRef = useRef(0);

  useEffect(() => {
    const video = videoRef.current;
    if (!video || !opts.url) return;

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

    function resetVideo() {
      video.pause();
      video.removeAttribute('src');
      while (video.firstChild) {
        video.removeChild(video.firstChild);
      }
      video.load();
    }

    async function initFlv() {
      try {
        const flvjs = await import('flv.js');
        if (currentInitId !== initIdRef.current) return;

        const flvModule = flvjs.default || flvjs;
        if (!flvModule.isSupported()) {
          setError('FLV.js not supported in this browser');
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

        player.attachMediaElement(video);
        player.load();

        if (opts.autoplay !== false) {
          try {
            await video.play();
            setPlaying(true);
          } catch {
            video.muted = true;
            await video.play().catch(() => {});
            setPlaying(true);
          }
        }

        flvPlayerRef.current = player;
        setPlayerType('flv');
      } catch (e) {
        if (currentInitId === initIdRef.current) {
          setError(`FLV init failed: ${e}`);
        }
      }
    }

    async function initHls() {
      try {
        const Hls = (await import('hls.js')).default;
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
          hls.attachMedia(video);

          hls.on(Hls.Events.MANIFEST_PARSED, () => {
            if (currentInitId !== initIdRef.current) return;
            if (opts.autoplay !== false) {
              video.play().catch(() => {
                video.muted = true;
                video.play().catch(() => {});
              });
            }
          });

          hls.on(Hls.Events.ERROR, (_event, data) => {
            if (data.fatal && currentInitId === initIdRef.current) {
              setError(`HLS error: ${data.type} - ${data.details}`);
            }
          });

          hlsPlayerRef.current = hls;
          setPlayerType('hls');
        } else if (video.canPlayType('application/vnd.apple.mpegurl')) {
          video.src = opts.url;
          video.load();
          if (opts.autoplay !== false) {
            video.play().catch(() => {});
          }
          setPlayerType('native');
        } else {
          setError('HLS not supported in this browser');
        }
      } catch (e) {
        if (currentInitId === initIdRef.current) {
          setError(`HLS init failed: ${e}`);
        }
      }
    }

    destroyFlv();
    destroyHls();
    resetVideo();

    requestAnimationFrame(() => {
      if (currentInitId !== initIdRef.current) return;

      if (isFlv) {
        initFlv();
      } else if (isHls) {
        initHls();
      } else {
        video.src = opts.url;
        video.load();
        if (opts.autoplay !== false) {
          video.play().catch(() => {});
        }
        setPlayerType('native');
      }
    });

    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onError = () => setError(`Video error: ${video.error?.message ?? 'unknown'}`);

    video.addEventListener('play', onPlay);
    video.addEventListener('pause', onPause);
    video.addEventListener('error', onError);

    const interval = setInterval(() => {
      if (currentInitId !== initIdRef.current || !video.buffered.length) return;
      const behind = video.buffered.end(video.buffered.length - 1) - video.currentTime;
      setLatency(Math.max(0, behind));
    }, 500);

    return () => {
      initIdRef.current++;
      clearInterval(interval);
      video.removeEventListener('play', onPlay);
      video.removeEventListener('pause', onPause);
      video.removeEventListener('error', onError);
      destroyFlv();
      destroyHls();
      video.pause();
      video.removeAttribute('src');
      video.load();
    };
  }, [opts.url, opts.autoplay]);

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
