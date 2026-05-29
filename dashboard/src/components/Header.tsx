import { useTheme } from '../hooks/useTheme';
import { useLocale } from '../hooks/useLocale';

interface Props {
  version: string;
  onSettings: () => void;
  wsConnected?: boolean;
}

export function Header({ version, onSettings, wsConnected }: Props) {
  const { theme, toggle } = useTheme();
  const { t } = useLocale();

  return (
    <header class="bg-surface-alt border-b border-border px-6 py-4 flex items-center justify-between">
      <h1 class="text-lg font-bold text-accent">{t('header.title')}</h1>
      <div class="flex items-center gap-3">
        {wsConnected !== undefined && (
          <span
            class={`w-2 h-2 rounded-full ${wsConnected ? 'bg-success' : 'bg-warning animate-pulse'}`}
            title={wsConnected ? t('header.connected') : t('header.reconnecting')}
          />
        )}
        <span class="text-sm text-fg-faint">{t('header.version', { version })}</span>
        <button
          onClick={toggle}
          class="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-surface-hover text-fg-muted hover:text-fg transition-colors"
          title={t('header.switchTheme', { mode: theme === 'dark' ? 'light' : 'dark' })}
        >
          {theme === 'dark' ? (
            <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z"
              />
            </svg>
          ) : (
            <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z"
              />
            </svg>
          )}
        </button>
        <button
          onClick={onSettings}
          class="w-8 h-8 flex items-center justify-center rounded-lg hover:bg-surface-hover text-fg-muted hover:text-fg transition-colors"
          title={t('header.settings')}
        >
          <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              stroke-width="2"
              d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
            />
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
        </button>
      </div>
    </header>
  );
}
