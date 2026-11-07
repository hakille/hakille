import React from 'react';
import { createRoot, hydrateRoot } from 'react-dom/client';
import '@/styles/base.css';
import '@/styles/layout.css';
import '@/styles/simulators.css';
import '@/styles/pages.css';
import { detectInitialLocale } from '@/i18n/context';
import { App } from './App';

const container = document.getElementById('root');

if (container) {
  const hasPrerenderedMarkup =
    container.firstElementChild !== null && !container.innerHTML.includes('HAKILLE_SSR_OUTLET');
  const matchesPrerenderedLocale = detectInitialLocale() === 'fr';

  if (hasPrerenderedMarkup && matchesPrerenderedLocale) {
    hydrateRoot(
      container,
      <React.StrictMode>
        <App />
      </React.StrictMode>
    );
  } else {
    container.replaceChildren();
    createRoot(container).render(
      <React.StrictMode>
        <App />
      </React.StrictMode>
    );
  }
}
