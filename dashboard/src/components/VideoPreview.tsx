import { useState } from 'preact/hooks';
import { useVideoPlayer } from '../hooks/useVideoPlayer';

interface Props {
  streams: Array<{ id: string; name: string; status: string }>;
}

type StreamSource = 'flv' | 'hls';

export function VideoPreview({ streams }: Props) {
  const [source, setSource] = useState<StreamSource>('flv');
  const [selectedStream, setSelectedStream] = useState<string>('');

  const liveStream = streams.find(
    (s) => s.status === 'Live' || (typeof s.status === 'object' && 'Live' in s.status),
  );

  const streamToWatch = selectedStream || liveStream?.id || '';

  const url = streamToWatch
    ? source === 'flv'
      ? '/stream.flv'
      : '/stream.m3u8'
    : '';

  const { videoRef, playing, error, latency, playerType, toggle } = useVideoPlayer({
    url,
    autoplay: true,
    muted: true,
    lowLatency: true,
  });

  const hasLive = !!liveStream;

  return (
    <div class="bg-slate-900 border border-slate-800 rounded-xl mb-6">
      <div class="flex items-center justify-between px-5 py-4 border-b border-slate-800">
        <h2 class="text-base font-semibold">Stream Preview</h2>
        <div class="flex items-center gap-3">
          <div class="flex items-center gap-1 bg-slate-800 rounded-lg p-0.5">
            <button
              onClick={() => setSource('flv')}
              class={`px-2.5 py-1 text-xs rounded-md transition-colors ${
                source === 'flv'
                  ? 'bg-sky-600 text-white'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              FLV (low latency)
            </button>
            <button
              onClick={() => setSource('hls')}
              class={`px-2.5 py-1 text-xs rounded-md transition-colors ${
                source === 'hls'
                  ? 'bg-sky-600 text-white'
                  : 'text-slate-400 hover:text-slate-200'
              }`}
            >
              HLS
            </button>
          </div>
          {streams.length > 1 && (
            <select
              value={selectedStream}
              onChange={(e) => setSelectedStream((e.target as HTMLSelectElement).value)}
              class="bg-slate-800 border border-slate-700 rounded px-2 py-1 text-xs text-slate-300"
            >
              <option value="">Auto ({liveStream?.name ?? 'none'})</option>
              {streams.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          )}
        </div>
      </div>

      <div class="p-4">
        {!hasLive && !url ? (
          <div class="flex items-center justify-center h-64 bg-slate-950 rounded-lg border border-slate-800">
            <div class="text-center">
              <svg
                class="mx-auto mb-3 w-12 h-12 text-slate-600"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  stroke-width="1.5"
                  d="M15 10l4.553-2.276A1 1 0 0121 8.618v6.764a1 1 0 01-1.447.894L15 14M5 18h8a2 2 0 002-2V8a2 2 0 00-2-2H5a2 2 0 00-2 2v8a2 2 0 002 2z"
                />
              </svg>
              <p class="text-slate-500 text-sm">No live stream to preview</p>
              <p class="text-slate-600 text-xs mt-1">
                Start a stream to see the preview here
              </p>
            </div>
          </div>
        ) : (
          <div class="relative">
            <video
              ref={videoRef}
              class="w-full rounded-lg bg-black"
              style={{ maxHeight: '400px' }}
              muted
              playsinline
              onClick={toggle}
            />

            {/* Overlay controls */}
            <div class="absolute bottom-0 left-0 right-0 bg-gradient-to-t from-black/80 to-transparent p-3 rounded-b-lg">
              <div class="flex items-center justify-between">
                <div class="flex items-center gap-3">
                  <button
                    onClick={toggle}
                    class="w-8 h-8 flex items-center justify-center rounded-full bg-white/20 hover:bg-white/30 transition-colors"
                  >
                    {playing ? (
                      <svg class="w-4 h-4 text-white" fill="currentColor" viewBox="0 0 20 20">
                        <path
                          fill-rule="evenodd"
                          d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zM7 8a1 1 0 012 0v4a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v4a1 1 0 102 0V8a1 1 0 00-1-1z"
                          clip-rule="evenodd"
                        />
                      </svg>
                    ) : (
                      <svg class="w-4 h-4 text-white" fill="currentColor" viewBox="0 0 20 20">
                        <path
                          fill-rule="evenodd"
                          d="M10 18a8 8 0 100-16 8 8 0 000 16zM9.555 7.168A1 1 0 008 8v4a1 1 0 001.555.832l3-2a1 1 0 000-1.664l-3-2z"
                          clip-rule="evenodd"
                        />
                      </svg>
                    )}
                  </button>
                  <span class="text-white text-xs font-mono">
                    {playing ? 'LIVE' : 'PAUSED'}
                  </span>
                </div>
                <div class="flex items-center gap-3">
                  <span class="text-xs text-slate-300">
                    {playerType === 'flv' ? 'FLV' : 'HLS'}
                  </span>
                  <span
                    class={`text-xs font-mono ${
                      latency < 1 ? 'text-emerald-400' : latency < 3 ? 'text-amber-400' : 'text-red-400'
                    }`}
                  >
                    {latency.toFixed(1)}s lag
                  </span>
                </div>
              </div>
            </div>

            {/* Error overlay */}
            {error && (
              <div class="absolute inset-0 flex items-center justify-center bg-black/60 rounded-lg">
                <div class="text-center">
                  <svg
                    class="mx-auto mb-2 w-8 h-8 text-red-400"
                    fill="none"
                    viewBox="0 0 24 24"
                    stroke="currentColor"
                  >
                    <path
                      stroke-linecap="round"
                      stroke-linejoin="round"
                      stroke-width="2"
                      d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-2.5L13.732 4c-.77-.833-1.964-.833-2.732 0L4.082 16.5c-.77.833.192 2.5 1.732 2.5z"
                    />
                  </svg>
                  <p class="text-red-400 text-sm">{error}</p>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
