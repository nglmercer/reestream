import { useState, useEffect, useCallback, useContext } from 'preact/hooks';
import { createContext } from 'preact';
import type { ComponentChildren, Context } from 'preact';
import { resolve, type Locale, type TranslationKey, localeNames } from '../i18n';

interface LocaleContextValue {
  locale: Locale;
  set: (l: Locale) => void;
  t: (key: TranslationKey, params?: Record<string, string | number>) => string;
  localeNames: Record<Locale, string>;
}

const LocaleContext: Context<LocaleContextValue> = createContext<LocaleContextValue>({
  locale: 'en',
  set: () => {},
  t: (key) => key,
  localeNames,
});

const STORAGE_KEY = 'reestream-locale';

function getInitialLocale(): Locale {
  if (typeof window === 'undefined') return 'en';
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === 'en' || stored === 'es') return stored;
  const browserLang = navigator.language.split('-')[0];
  if (browserLang === 'es') return 'es';
  return 'en';
}

export function LocaleProvider({ children }: { children: ComponentChildren }) {
  const [locale, setLocale] = useState<Locale>(getInitialLocale);

  useEffect(() => {
    localStorage.setItem(STORAGE_KEY, locale);
    document.documentElement.setAttribute('lang', locale);
  }, [locale]);

  const set = useCallback((l: Locale) => setLocale(l), []);

  const t = useCallback(
    (key: TranslationKey, params?: Record<string, string | number>) => resolve(key, params, locale),
    [locale],
  );

  return (
    <LocaleContext.Provider value={{ locale, set, t, localeNames }}>
      {children}
    </LocaleContext.Provider>
  );
}

export function useLocale(): LocaleContextValue {
  return useContext(LocaleContext);
}
