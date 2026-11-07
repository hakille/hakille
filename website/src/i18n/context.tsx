import React, { createContext, useContext, useEffect, useMemo, useState } from 'react';
import { LOCALE_METADATA, LocaleCode, SUPPORTED_LOCALES, TranslationSchema } from './types';
import { frLocale } from './locales/fr';
import { enLocale } from './locales/en';
import { esLocale } from './locales/es';
import { deLocale } from './locales/de';
import { ptLocale } from './locales/pt';
import { arLocale } from './locales/ar';
import { swLocale } from './locales/sw';
import { jaLocale } from './locales/ja';

export const ALL_DICTIONARIES: Readonly<Record<LocaleCode, TranslationSchema>> = {
  fr: frLocale,
  en: enLocale,
  es: esLocale,
  de: deLocale,
  pt: ptLocale,
  ar: arLocale,
  sw: swLocale,
  ja: jaLocale,
} as const;

const STORAGE_KEY = 'hakille.locale';

export function isValidLocale(code: string | null | undefined): code is LocaleCode {
  return Boolean(code && (SUPPORTED_LOCALES as readonly string[]).includes(code));
}

export function detectInitialLocale(ssrLocale?: LocaleCode): LocaleCode {
  if (ssrLocale && isValidLocale(ssrLocale)) return ssrLocale;
  if (typeof window === 'undefined') return 'fr';

  try {
    const params = new URLSearchParams(window.location.search);
    const queryLang = params.get('lang');
    if (isValidLocale(queryLang)) return queryLang;

    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (isValidLocale(saved)) return saved;
  } catch {
    // Ignore storage errors in hardened browsers
  }
  return 'fr';
}

interface I18nContextValue {
  readonly locale: LocaleCode;
  readonly dir: 'ltr' | 'rtl';
  readonly t: TranslationSchema;
  readonly setLocale: (next: LocaleCode) => void;
}

const I18nContext = createContext<I18nContextValue>({
  locale: 'fr',
  dir: 'ltr',
  t: frLocale,
  setLocale: () => undefined,
});

export function I18nProvider({
  initialLocale,
  children,
}: {
  initialLocale?: LocaleCode;
  children: React.ReactNode;
}) {
  const [locale, setLocaleState] = useState<LocaleCode>(() => detectInitialLocale(initialLocale));

  const setLocale = (next: LocaleCode) => {
    setLocaleState(next);
    if (typeof window !== 'undefined') {
      try {
        window.localStorage.setItem(STORAGE_KEY, next);
        const url = new URL(window.location.href);
        if (next === 'fr') {
          url.searchParams.delete('lang');
        } else {
          url.searchParams.set('lang', next);
        }
        window.history.replaceState({}, '', url.toString());
      } catch {
        // Ignore storage restrictions
      }
    }
  };

  const meta = LOCALE_METADATA[locale];

  useEffect(() => {
    if (typeof document !== 'undefined') {
      document.documentElement.lang = meta.htmlLang;
      document.documentElement.dir = meta.dir;
    }
  }, [meta.dir, meta.htmlLang]);

  const value = useMemo<I18nContextValue>(
    () => ({
      locale,
      dir: meta.dir,
      t: ALL_DICTIONARIES[locale],
      setLocale,
    }),
    [locale, meta.dir]
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  return useContext(I18nContext);
}
