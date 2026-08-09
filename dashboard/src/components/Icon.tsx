import type { JSX } from 'preact';

export type IconName =
  | 'activity'
  | 'arrowLeft'
  | 'calendar'
  | 'check'
  | 'chevronDown'
  | 'chevronRight'
  | 'circleHelp'
  | 'clipboard'
  | 'clapperboard'
  | 'close'
  | 'copy'
  | 'edit'
  | 'external'
  | 'grid'
  | 'home'
  | 'layers'
  | 'link'
  | 'logout'
  | 'monitor'
  | 'more'
  | 'pause'
  | 'play'
  | 'plus'
  | 'radio'
  | 'refresh'
  | 'search'
  | 'settings'
  | 'sparkle'
  | 'storage'
  | 'trash'
  | 'users'
  | 'video'
  | 'warning';

interface Props {
  name: IconName;
  size?: number;
  class?: string;
  strokeWidth?: number;
}

const common = {
  fill: 'none',
  stroke: 'currentColor',
  'stroke-linecap': 'round',
  'stroke-linejoin': 'round',
} as const;

function shape(name: IconName): JSX.Element {
  switch (name) {
    case 'activity':
      return <><path {...common} d="M3 12h4l2.2-7 4.6 14 2.2-7H21" /></>;
    case 'arrowLeft':
      return <><path {...common} d="M19 12H5m7 7-7-7 7-7" /></>;
    case 'calendar':
      return <><rect {...common} x="3" y="4.5" width="18" height="16" rx="2" /><path {...common} d="M16 2.5v4M8 2.5v4M3 9.5h18" /></>;
    case 'check':
      return <path {...common} d="m5 12 4.5 4.5L19 7" />;
    case 'chevronDown':
      return <path {...common} d="m6 9 6 6 6-6" />;
    case 'chevronRight':
      return <path {...common} d="m9 6 6 6-6 6" />;
    case 'circleHelp':
      return <><circle {...common} cx="12" cy="12" r="9" /><path {...common} d="M9.7 9a2.4 2.4 0 1 1 4.1 1.7c-1.2 1.1-1.8 1.3-1.8 2.8M12 17h.01" /></>;
    case 'clipboard':
      return <><rect {...common} x="5" y="4" width="14" height="17" rx="2" /><path {...common} d="M9 4V3h6v1M8 9h8M8 13h8M8 17h5" /></>;
    case 'clapperboard':
      return <><path {...common} d="m4 7 16-3 1 5L5 12 4 7Z" /><path {...common} d="M5 12v8h15v-11M8 6l2 4M13 5l2 4M18 4l2 4" /></>;
    case 'close':
      return <path {...common} d="m6 6 12 12M18 6 6 18" />;
    case 'copy':
      return <><rect {...common} x="8" y="8" width="11" height="12" rx="2" /><path {...common} d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h2" /></>;
    case 'edit':
      return <><path {...common} d="m4 16.5-.8 4.3 4.3-.8L19.7 7.8a2.8 2.8 0 0 0-4-4L4 16.5Z" /><path {...common} d="m13.8 5.7 4.5 4.5" /></>;
    case 'external':
      return <><path {...common} d="M14 4h6v6M20 4l-9 9" /><path {...common} d="M18 13v5a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h5" /></>;
    case 'grid':
      return <><rect {...common} x="4" y="4" width="6" height="6" rx="1" /><rect {...common} x="14" y="4" width="6" height="6" rx="1" /><rect {...common} x="4" y="14" width="6" height="6" rx="1" /><rect {...common} x="14" y="14" width="6" height="6" rx="1" /></>;
    case 'home':
      return <><path {...common} d="m3 10 9-7 9 7v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-9Z" /><path {...common} d="M9 21v-7h6v7" /></>;
    case 'layers':
      return <><path {...common} d="m12 3 9 5-9 5-9-5 9-5Z" /><path {...common} d="m3 12 9 5 9-5M3 16l9 5 9-5" /></>;
    case 'link':
      return <><path {...common} d="M10 13a5 5 0 0 0 7.1.1l2-2a5 5 0 0 0-7.1-7.1l-1.1 1.1" /><path {...common} d="M14 11a5 5 0 0 0-7.1-.1l-2 2A5 5 0 0 0 12 20l1.1-1.1" /></>;
    case 'logout':
      return <><path {...common} d="M10 5H6a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h4" /><path {...common} d="m14 8 4 4-4 4M9 12h9" /></>;
    case 'monitor':
      return <><rect {...common} x="3" y="4" width="18" height="13" rx="2" /><path {...common} d="M8 21h8M12 17v4" /></>;
    case 'more':
      return <><circle cx="5" cy="12" r="1.3" fill="currentColor" /><circle cx="12" cy="12" r="1.3" fill="currentColor" /><circle cx="19" cy="12" r="1.3" fill="currentColor" /></>;
    case 'pause':
      return <><path {...common} d="M8 5v14M16 5v14" /></>;
    case 'play':
      return <path {...common} d="m9 6 9 6-9 6V6Z" />;
    case 'plus':
      return <path {...common} d="M12 5v14M5 12h14" />;
    case 'radio':
      return <><circle {...common} cx="12" cy="12" r="2" /><path {...common} d="M5.6 5.6a9 9 0 0 0 0 12.8M18.4 5.6a9 9 0 0 1 0 12.8M2.8 2.8a13 13 0 0 0 0 18.4M21.2 2.8a13 13 0 0 1 0 18.4" /></>;
    case 'refresh':
      return <><path {...common} d="M20 11a8.1 8.1 0 0 0-14.8-4L3 10" /><path {...common} d="M3 5v5h5M4 13a8.1 8.1 0 0 0 14.8 4l2-3" /><path {...common} d="M21 19v-5h-5" /></>;
    case 'search':
      return <><circle {...common} cx="10.8" cy="10.8" r="6.8" /><path {...common} d="m16 16 5 5" /></>;
    case 'settings':
      return <><path {...common} d="M12 15.2a3.2 3.2 0 1 0 0-6.4 3.2 3.2 0 0 0 0 6.4Z" /><path {...common} d="m19.4 15 .1.1a2 2 0 0 1-2.8 2.8l-.1-.1a2 2 0 0 0-3.4 1.4v.2a2 2 0 0 1-4 0v-.2a2 2 0 0 0-3.4-1.4l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A2 2 0 0 0 1.6 12h-.2a2 2 0 0 1 0-4h.2A2 2 0 0 0 3 4.6l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A2 2 0 0 0 9.2.5h.2a2 2 0 0 1 4 0v.2a2 2 0 0 0 3.4 1.4l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1A2 2 0 0 0 21 8h.2a2 2 0 0 1 0 4H21a2 2 0 0 0-1.6 3Z" /></>;
    case 'sparkle':
      return <><path {...common} d="m12 3 1.4 5.6L19 10l-5.6 1.4L12 17l-1.4-5.6L5 10l5.6-1.4L12 3Z" /><path {...common} d="m19 16 .7 2.3L22 19l-2.3.7L19 22l-.7-2.3L16 19l2.3-.7L19 16Z" /></>;
    case 'storage':
      return <><ellipse {...common} cx="12" cy="5" rx="8" ry="3" /><path {...common} d="M4 5v7c0 1.7 3.6 3 8 3s8-1.3 8-3V5M4 12v7c0 1.7 3.6 3 8 3s8-1.3 8-3v-7" /></>;
    case 'trash':
      return <><path {...common} d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" /></>;
    case 'users':
      return <><circle {...common} cx="9" cy="8" r="3" /><path {...common} d="M3 20v-1a6 6 0 0 1 12 0v1M16 5.5a3 3 0 0 1 0 5.8M18 14a5 5 0 0 1 3 4.5V20" /></>;
    case 'video':
      return <><rect {...common} x="3" y="5" width="13" height="14" rx="2" /><path {...common} d="m16 10 5-3v10l-5-3" /></>;
    case 'warning':
      return <><path {...common} d="m12 3 9 17H3L12 3Z" /><path {...common} d="M12 9v4M12 16h.01" /></>;
  }
}

export function Icon({ name, size = 18, class: className, strokeWidth = 1.7 }: Props) {
  return (
    <svg
      aria-hidden="true"
      class={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      stroke-width={strokeWidth}
      {...common}
    >
      {shape(name)}
    </svg>
  );
}
