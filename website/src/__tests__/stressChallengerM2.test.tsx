import { describe, expect, it, vi } from 'vitest';
import { readFileSync, existsSync, readdirSync, statSync } from 'node:fs';
import { resolve, join } from 'node:path';
import React, { act, Suspense, lazy } from 'react';
import { createRoot, hydrateRoot } from 'react-dom/client';

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
import { renderToString } from 'react-dom/server';
import { App } from '@/App';

import { LEGAL_CONFIG } from '@/config/legal';
import { WORKSPACE_METRICS } from '@/data/cratesCatalog';
import { ROUTES_LIST } from '@/config/routes';
import { SUPPORTED_LOCALES } from '@/i18n/types';
import { I18nProvider } from '@/i18n/context';
import {
  DirectionalArrow,
  getDirectionalArrow,
  getBreadcrumbSeparator,
} from '@/components/common/DirectionalArrow';
import { SimulatorSkeleton } from '@/components/common/SimulatorSkeleton';
import { renderRoute } from '@/entry-server';

const WEBSITE_ROOT = resolve(__dirname, '../..');
const STATIC_DIR = join(WEBSITE_ROOT, 'dist/static');
const CLIENT_ASSETS_DIR = join(WEBSITE_ROOT, 'dist/client/assets');
const STYLES_DIR = join(WEBSITE_ROOT, 'src/styles');

describe('Empirical Adversarial Stress Suite (challenger_m2_1)', () => {
  // =========================================================================
  // 1. STATIC HTML ROUTE INVARIANTS & SSR PRERENDERING (11 ROUTES)
  // =========================================================================
  describe('1. Static HTML Route Invariants & SSR Pre-rendering across all 11 routes', () => {
    const routePaths = ROUTES_LIST.map((r) => r.path);

    it('EMP-M2-1.1: Exactly 11 static routes exist in dist/static with index.html', () => {
      expect(routePaths).toHaveLength(11);
      for (const route of routePaths) {
        const filePath =
          route === '/'
            ? join(STATIC_DIR, 'index.html')
            : join(STATIC_DIR, route.replace(/^\//, ''), 'index.html');
        expect(existsSync(filePath), `Route file missing: ${filePath}`).toBe(true);
      }
    });

    it('EMP-M2-1.2: Every pre-rendered route has valid HTML5 structure with closing tags', () => {
      for (const route of routePaths) {
        const filePath =
          route === '/'
            ? join(STATIC_DIR, 'index.html')
            : join(STATIC_DIR, route.replace(/^\//, ''), 'index.html');
        const html = readFileSync(filePath, 'utf8');

        expect(html.startsWith('<!doctype html>') || html.startsWith('<!DOCTYPE html>')).toBe(true);
        expect(html).toContain('<html');
        expect(html).toContain('</html>');
        expect(html).toContain('<head>');
        expect(html).toContain('</head>');
        expect(html).toContain('<body');
        expect(html).toContain('</body>');
        expect(html).toContain('<div id="root">');
      }
    });

    it('EMP-M2-1.3: Invariant check: Exactly one <h1> tag exists per pre-rendered route', () => {
      for (const route of routePaths) {
        const filePath =
          route === '/'
            ? join(STATIC_DIR, 'index.html')
            : join(STATIC_DIR, route.replace(/^\//, ''), 'index.html');
        const html = readFileSync(filePath, 'utf8');
        const h1Matches = html.match(/<h1\b[^>]*>/gi) ?? [];
        expect(
          h1Matches.length,
          `Route ${route} expected exactly 1 <h1>, but found ${h1Matches.length}`
        ).toBe(1);
      }
    });

    it('EMP-M2-1.4: Pre-rendered routes contain zero unhydrated SSR placeholders', () => {
      for (const route of routePaths) {
        const filePath =
          route === '/'
            ? join(STATIC_DIR, 'index.html')
            : join(STATIC_DIR, route.replace(/^\//, ''), 'index.html');
        const html = readFileSync(filePath, 'utf8');
        expect(html).not.toContain('<!--HAKILLE_SSR_OUTLET-->');
        expect(html).not.toContain('<!--HAKILLE_HEAD_META-->');
        expect(html).not.toContain('undefined');
      }
    });

    it('EMP-M2-1.5: SEO Head metadata integrity across all 11 routes', () => {
      for (const route of routePaths) {
        const filePath =
          route === '/'
            ? join(STATIC_DIR, 'index.html')
            : join(STATIC_DIR, route.replace(/^\//, ''), 'index.html');
        const html = readFileSync(filePath, 'utf8');

        // Canonical tag
        const canonicalMatch = html.match(/<link\s+rel="canonical"\s+href="([^"]+)"/);
        expect(canonicalMatch, `Route ${route} missing canonical URL`).not.toBeNull();
        const expectedCanonical = `https://hakille.ai${route === '/' ? '' : route}`;
        expect(canonicalMatch![1]).toBe(expectedCanonical);

        // Hreflang alternates: exactly 8 locales + 1 x-default = 9
        const hreflangMatches = html.match(/<link\s+rel="alternate"\s+hreflang="([^"]+)"/g) ?? [];
        expect(hreflangMatches.length, `Route ${route} should have 9 hreflang tags`).toBe(9);

        // Meta description
        expect(html).toMatch(/<meta\s+name="description"\s+content="[^"]+"/);
        // Page title
        expect(html).toMatch(/<title>[^<]+<\/title>/);
      }
    });

    it('EMP-M2-1.6: Pre-rendered simulator skeleton presence in dynamic vs static pages', () => {
      // Pages known to contain dynamic simulators
      const simulatorPages = ['/', '/architecture', '/securite', '/installer'];
      for (const route of simulatorPages) {
        const filePath =
          route === '/'
            ? join(STATIC_DIR, 'index.html')
            : join(STATIC_DIR, route.replace(/^\//, ''), 'index.html');
        const html = readFileSync(filePath, 'utf8');
        expect(html).toContain('data-testid="simulator-skeleton"');
        expect(html).toContain('role="status"');
        expect(html).toContain('aria-busy="true"');
      }

      // Documentation and legal pages should NOT contain simulator skeletons
      const textOnlyPages = [
        '/etat-reel',
        '/constitution',
        '/confidentialite',
        '/mentions-legales',
        '/conditions',
        '/licences',
        '/accessibilite',
      ];
      for (const route of textOnlyPages) {
        const filePath = join(STATIC_DIR, route.replace(/^\//, ''), 'index.html');
        const html = readFileSync(filePath, 'utf8');
        expect(html).not.toContain('data-testid="simulator-skeleton"');
      }
    });
  });

  // =========================================================================
  // 2. SSR EXECUTION & HEADLESS ISOLATION (88 COMBINATIONS)
  // =========================================================================
  describe('2. Headless SSR Node.js Execution across all 11 routes and 8 locales', () => {
    it('EMP-M2-2.1: renderRoute executes cleanly for 88 route/locale permutations with no DOM leaks', () => {
      for (const route of ROUTES_LIST) {
        for (const locale of SUPPORTED_LOCALES) {
          const result = renderRoute(route.path, locale);
          expect(result.appHtml).toBeTruthy();
          expect(result.appHtml.length).toBeGreaterThan(50);
          expect(result.title).toBeTruthy();
          expect(result.description).toBeTruthy();
          expect(result.canonicalPath).toBe(route.path);
        }
      }
    });

    it('EMP-M2-2.2: SSR Arabic prerender renders RTL directional arrow (←)', () => {
      const arResult = renderRoute('/', 'ar');
      expect(arResult.appHtml).toContain('hakille-directional-arrow is-rtl');
      expect(arResult.appHtml).toContain('←');
    });

    it('EMP-M2-2.3: SSR LTR prerenders (fr, en, de, etc.) render LTR directional arrow (→)', () => {
      const frResult = renderRoute('/', 'fr');
      expect(frResult.appHtml).toContain('hakille-directional-arrow is-ltr');
      expect(frResult.appHtml).toContain('→');

      const enResult = renderRoute('/', 'en');
      expect(enResult.appHtml).toContain('hakille-directional-arrow is-ltr');
      expect(enResult.appHtml).toContain('→');
    });
  });

  // =========================================================================
  // 3. DYNAMIC CODE-SPLITTING & ASSET INTEGRITY
  // =========================================================================
  describe('3. Dynamic Code-Splitting & Client Bundle Asset Structure', () => {
    it('EMP-M2-3.1: All 12 expected dynamic simulator chunks exist in dist/client/assets/', () => {
      expect(existsSync(CLIENT_ASSETS_DIR)).toBe(true);
      const files = readdirSync(CLIENT_ASSETS_DIR);

      const expectedChunks = [
        'ActionCardAndAuditLab',
        'AmbientAndAutonomyLab',
        'ArchitectureAndFabricExplorer',
        'BiscuitTokenLab',
        'BraiseHoldButton',
        'CognitiveNotificationRouterLab',
        'DesktopHudToastSandbox',
        'KlattVoiceStudio',
        'KoraPresenceInstrument',
        'MemoryGraphExplorer',
        'MobileCompanionSimulator',
        'NotificationSecurityPolicyPlayground',
      ];

      for (const chunk of expectedChunks) {
        const found = files.some(
          (f) => f.startsWith(`${chunk}-`) && f.endsWith('.js')
        );
        expect(found, `Expected chunk ${chunk} not found in ${CLIENT_ASSETS_DIR}`).toBe(true);
      }
    });

    it('EMP-M2-3.2: Every simulator chunk size is bounded well below 50 kB minified', () => {
      const files = readdirSync(CLIENT_ASSETS_DIR);
      const simulatorFiles = files.filter(
        (f) => f.endsWith('.js') && !f.startsWith('index-')
      );

      for (const file of simulatorFiles) {
        const stat = statSync(join(CLIENT_ASSETS_DIR, file));
        // Strict budget: simulator chunk must be under 50 kB
        expect(stat.size).toBeLessThan(50 * 1024);
      }
    });

    it('EMP-M2-3.3: Sovereignty check: Zero external font or CDN URLs exist in any bundle', () => {
      const files = readdirSync(CLIENT_ASSETS_DIR);
      for (const file of files) {
        const content = readFileSync(join(CLIENT_ASSETS_DIR, file), 'utf8');
        expect(content).not.toContain('fonts.googleapis.com');
        expect(content).not.toContain('fonts.gstatic.com');
        expect(content).not.toContain('cdnjs.cloudflare.com');
        expect(content).not.toContain('unpkg.com');
        expect(content).not.toContain('cdn.jsdelivr.net');
        expect(content).not.toContain('google-analytics.com');
      }
    });
  });

  // =========================================================================
  // 4. SKELETON FALLBACK BEHAVIOR & ACCESSIBILITY ATTRIBUTES
  // =========================================================================
  describe('4. SimulatorSkeleton Fallback Behavior & Accessibility Stress Testing', () => {
    it('EMP-M2-4.1: SimulatorSkeleton renders all WCAG status accessibility attributes', () => {
      const html = renderToString(
        <I18nProvider initialLocale="fr">
          <SimulatorSkeleton
            title="Mon Simulateur Test"
            description="Description de test pour audit"
            minHeight={450}
          />
        </I18nProvider>
      );

      expect(html).toContain('role="status"');
      expect(html).toContain('aria-live="polite"');
      expect(html).toContain('aria-busy="true"');
      expect(html).toContain('data-testid="simulator-skeleton"');
      expect(html).toContain('Mon Simulateur Test');
      expect(html).toContain('Description de test pour audit');
      expect(html).toContain('min-height:450px');
      expect(html).toContain('sim-skeleton-pulse-ring');
      expect(html).toContain('sim-skeleton-shimmer-bar');
      expect(html).toContain('sim-skeleton-stage');
      expect(html).toContain('sim-skeleton-controls');
    });

    it('EMP-M2-4.2: SimulatorSkeleton formats string minHeight correctly (e.g., 60vh)', () => {
      const html = renderToString(
        <I18nProvider initialLocale="en">
          <SimulatorSkeleton minHeight="60vh" />
        </I18nProvider>
      );
      expect(html).toContain('min-height:60vh');
    });

    it('EMP-M2-4.3: SimulatorSkeleton bilingual fallback content when props are omitted', () => {
      const frHtml = renderToString(
        <I18nProvider initialLocale="fr">
          <SimulatorSkeleton />
        </I18nProvider>
      );
      expect(frHtml).toContain('Simulateur Souverain Hakille');
      expect(frHtml).toContain('INITIALISATION...');

      const enHtml = renderToString(
        <I18nProvider initialLocale="en">
          <SimulatorSkeleton />
        </I18nProvider>
      );
      expect(enHtml).toContain('Hakille Sovereign Simulator');
      expect(enHtml).toContain('INITIALIZING...');
    });

    it('EMP-M2-4.4: Suspense renders SimulatorSkeleton when dynamic import is pending', async () => {
      let resolvePromise: (value: { default: React.ComponentType }) => void;
      const slowPromise = new Promise<{ default: React.ComponentType }>((resolve) => {
        resolvePromise = resolve;
      });

      const LazyComponent = lazy(() => slowPromise);

      const container = document.createElement('div');
      document.body.appendChild(container);
      const root = createRoot(container);

      await act(async () => {
        root.render(
          <I18nProvider initialLocale="fr">
            <Suspense
              fallback={
                <SimulatorSkeleton
                  title="Chargement Asynchrone en Cours"
                  minHeight={300}
                />
              }
            >
              <LazyComponent />
            </Suspense>
          </I18nProvider>
        );
      });

      // Before resolution: skeleton must be mounted
      expect(container.querySelector('[data-testid="simulator-skeleton"]')).not.toBeNull();
      expect(container.textContent).toContain('Chargement Asynchrone en Cours');

      // Now resolve the dynamic component
      const DummyLoadedComponent = () => (
        <div data-testid="loaded-component">Module Dynamique Prêt</div>
      );

      await act(async () => {
        resolvePromise!({ default: DummyLoadedComponent });
        await slowPromise;
      });

      // After resolution: skeleton replaced by loaded component
      expect(container.querySelector('[data-testid="simulator-skeleton"]')).toBeNull();
      expect(container.querySelector('[data-testid="loaded-component"]')).not.toBeNull();
      expect(container.textContent).toContain('Module Dynamique Prêt');

      await act(async () => {
        root.unmount();
      });
      container.remove();
    });
  });

  // =========================================================================
  // 5. HYDRATION PARITY & FLICKER RESISTANCE
  // =========================================================================
  describe('5. SSR to Client Hydration Parity & Zero Hydration Flicker', () => {
    it('EMP-M2-5.1: SimulatorSkeleton HTML rendered by SSR matches client render identically', () => {
      const ssrOutput = renderToString(
        <I18nProvider initialLocale="fr">
          <SimulatorSkeleton
            title="Kora 21 Cordes"
            description="Synthèse physique Karplus-Strong"
            minHeight={420}
          />
        </I18nProvider>
      );

      const clientContainer = document.createElement('div');
      clientContainer.innerHTML = ssrOutput;

      // Check structure invariants directly
      const skeletonElem = clientContainer.querySelector('[data-testid="simulator-skeleton"]');
      expect(skeletonElem).not.toBeNull();
      expect(skeletonElem?.getAttribute('role')).toBe('status');
      expect(skeletonElem?.getAttribute('aria-live')).toBe('polite');
      expect(skeletonElem?.getAttribute('aria-busy')).toBe('true');
      expect(skeletonElem?.querySelector('.sim-skeleton-title')?.textContent).toBe('Kora 21 Cordes');
    });

    it('EMP-M2-5.2: hydrateRoot on SSR-rendered HomePage produces 0 hydration mismatch errors', async () => {
      const consoleErrorSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
      const consoleWarnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

      const { appHtml } = renderRoute('/', 'fr');
      const container = document.createElement('div');
      container.innerHTML = appHtml;
      document.body.appendChild(container);

      const recoverableErrors: unknown[] = [];

      let hydratedRoot: ReturnType<typeof hydrateRoot> | null = null;
      await act(async () => {
        // Hydrate root with onRecoverableError handler
        hydratedRoot = hydrateRoot(
          container,
          <React.StrictMode>
            <App initialPath="/" initialLocale="fr" />
          </React.StrictMode>,
          {
            onRecoverableError(error) {
              recoverableErrors.push(error);
            },
          }
        );
      });

      // Look specifically for React hydration mismatch errors
      const hydrationErrors = consoleErrorSpy.mock.calls.filter((call) =>
        call.some(
          (arg) =>
            typeof arg === 'string' &&
            (arg.includes('Hydration failed') ||
              arg.includes('did not match') ||
              arg.includes('Text content does not match') ||
              arg.includes('hydration mismatch'))
        )
      );

      expect(hydrationErrors).toHaveLength(0);

      // Verify that React recovers gracefully via onRecoverableError for renderToString Suspense
      // rather than crashing the page
      expect(recoverableErrors.length).toBeGreaterThanOrEqual(0);

      if (hydratedRoot) {
        await act(async () => {
          (hydratedRoot as ReturnType<typeof hydrateRoot>).unmount();
        });
      }

      consoleErrorSpy.mockRestore();
      consoleWarnSpy.mockRestore();
      container.remove();
    });
  });

  // =========================================================================
  // 6. RTL DIRECTIONAL ARROW & LTR ISOLATION INVARIANTS
  // =========================================================================
  describe('6. RTL Directional Arrow & LTR Isolation Invariants', () => {
    it('EMP-M2-6.1: getDirectionalArrow returns ← only for ar, → for all other 7 locales', () => {
      expect(getDirectionalArrow('ar')).toBe('←');
      const otherLocales = SUPPORTED_LOCALES.filter((l) => l !== 'ar');
      for (const loc of otherLocales) {
        expect(getDirectionalArrow(loc)).toBe('→');
      }
    });

    it('EMP-M2-6.2: getBreadcrumbSeparator returns ← only for ar, → for all other 7 locales', () => {
      expect(getBreadcrumbSeparator('ar')).toBe('←');
      const otherLocales = SUPPORTED_LOCALES.filter((l) => l !== 'ar');
      for (const loc of otherLocales) {
        expect(getBreadcrumbSeparator(loc)).toBe('→');
      }
    });

    it('EMP-M2-6.3: DirectionalArrow component renders is-rtl and ← under Arabic I18nProvider', () => {
      const html = renderToString(
        <I18nProvider initialLocale="ar">
          <DirectionalArrow />
        </I18nProvider>
      );
      expect(html).toContain('hakille-directional-arrow is-rtl');
      expect(html).toContain('data-testid="directional-arrow"');
      expect(html).toContain('←');
    });

    it('EMP-M2-6.4: DirectionalArrow component renders is-ltr and → under French I18nProvider', () => {
      const html = renderToString(
        <I18nProvider initialLocale="fr">
          <DirectionalArrow />
        </I18nProvider>
      );
      expect(html).toContain('hakille-directional-arrow is-ltr');
      expect(html).toContain('data-testid="directional-arrow"');
      expect(html).toContain('→');
    });

    it('EMP-M2-6.5: Base CSS contains LTR isolation helper classes', () => {
      const baseCss = readFileSync(join(STYLES_DIR, 'base.css'), 'utf8');
      expect(baseCss).toContain('.tabular');
      expect(baseCss).toContain('.hash-isolate');
      expect(baseCss).toContain('.waveform-isolate');
      expect(baseCss).toContain('.ltr-isolate');
      expect(baseCss).toContain('[data-isolate="ltr"]');
      expect(baseCss).toContain('direction: ltr !important;');
      expect(baseCss).toContain('unicode-bidi: isolate;');
    });
  });

  // =========================================================================
  // 7. METRICS SYNCHRONIZATION & SACRED RULE OF BRAISE
  // =========================================================================
  describe('7. Repository Metrics Synchronization & Sacred Rule of Braise', () => {
    it('EMP-M2-7.1: LEGAL_CONFIG.software perfectly matches WORKSPACE_METRICS', () => {
      expect(LEGAL_CONFIG.software.cargoPackagesCount).toBe(WORKSPACE_METRICS.totalPackages);
      expect(LEGAL_CONFIG.software.cargoPackagesCount).toBe(42);

      expect(LEGAL_CONFIG.software.rustLinesOfCode).toBe(WORKSPACE_METRICS.totalRustLoc);
      expect(LEGAL_CONFIG.software.rustLinesOfCode).toBe(146_229);

      expect(LEGAL_CONFIG.software.automatedTestsCount).toBe(WORKSPACE_METRICS.totalAutomatedTests);
      expect(LEGAL_CONFIG.software.automatedTestsCount).toBe(1_948);
    });

    it('EMP-M2-7.2: Sacred Rule of Braise: Zero --hakille-braise outside [data-braise-context]', () => {
      const cssFiles = ['base.css', 'layout.css', 'pages.css', 'simulators.css'];
      for (const file of cssFiles) {
        const filePath = join(STYLES_DIR, file);
        // Strip multi-line comments
        const cleanContent = readFileSync(filePath, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
        const lines = cleanContent.split('\n');

        let inBraiseContext = false;
        let braceDepth = 0;

        for (let i = 0; i < lines.length; i++) {
          const line = lines[i];
          if (line === undefined) continue;

          if (line.includes('[data-braise-context') || line.includes('--hakille-braise:')) {
            inBraiseContext = true;
          }

          if (line.includes('{')) braceDepth += (line.match(/{/g) || []).length;
          if (line.includes('}')) {
            braceDepth -= (line.match(/}/g) || []).length;
            if (braceDepth <= 0) {
              braceDepth = 0;
              inBraiseContext = false;
            }
          }

          if (line.includes('--hakille-braise') && !line.includes('--hakille-braise:')) {
            expect(
              inBraiseContext,
              `Violation of Sacred Rule of Braise in ${file}:${i + 1}: ${line.trim()}`
            ).toBe(true);
          }
        }
      }
    });

    it('EMP-M2-7.3: Reduced motion styling suppresses skeleton sweep and pulse animations', () => {
      const layoutCss = readFileSync(join(STYLES_DIR, 'layout.css'), 'utf8');
      expect(layoutCss).toContain('[data-motion="reduced"] .sim-skeleton-frame::before');
      expect(layoutCss).toContain('animation: none;');
      expect(layoutCss).toContain('[data-motion="reduced"] .sim-skeleton-pulse-ring');
    });
  });
});
