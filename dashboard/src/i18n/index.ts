import { en, type TranslationKey } from './en';
import { es } from './es';

export type Locale = 'en' | 'es';

export const locales: Record<Locale, Record<TranslationKey, string>> = {
  en,
  es,
};

export const localeNames: Record<Locale, string> = {
  en: 'English',
  es: 'Espa\u00f1ol',
};

export type { TranslationKey };

export function resolve(key: TranslationKey, params?: Record<string, string | number>, locale: Locale = 'en'): string {
  const template = locales[locale]?.[key] ?? en[key] ?? key;
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (_, name: string) =>
    params[name] !== undefined ? String(params[name]) : `{${name}}`,
  );
}
