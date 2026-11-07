import React from 'react';
import { renderToString } from 'react-dom/server';
import { ROUTES_LIST, normalizePathname } from '@/config/routes';
import { ALL_DICTIONARIES } from '@/i18n/context';
import { LocaleCode, SUPPORTED_LOCALES } from '@/i18n/types';
import { App } from './App';

export { ROUTES_LIST, SUPPORTED_LOCALES };

export function renderRoute(rawPath: string, locale: LocaleCode = 'fr') {
  const canonicalPath = normalizePathname(rawPath);
  const matched = ROUTES_LIST.find((r) => r.path === canonicalPath) ?? ROUTES_LIST[0]!;
  const dict = ALL_DICTIONARIES[locale];
  const seo = dict.routes[matched.id];

  const appHtml = renderToString(
    <React.StrictMode>
      <App initialPath={canonicalPath} initialLocale={locale} />
    </React.StrictMode>
  );

  return {
    appHtml,
    title: seo.title,
    description: seo.description,
    canonicalPath,
  };
}
