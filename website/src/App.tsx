import React, { useEffect, useState } from 'react';
import { isKnownRoute } from '@/config/routes';
import { LocaleCode } from '@/i18n/types';
import { I18nProvider } from '@/i18n/context';
import { PreferencesProvider } from '@/context/PreferencesContext';
import { RouterProvider, useRouter } from '@/context/RouterContext';
import { SiteHeader } from '@/components/layout/SiteHeader';
import { SiteFooter } from '@/components/layout/SiteFooter';
import { AccessibilityDrawer } from '@/components/layout/AccessibilityDrawer';
import { PageMeta } from '@/components/layout/PageMeta';
import { SpotlightHudModal } from '@/components/simulators/SpotlightHudModal';

import { HomePage } from '@/pages/HomePage';
import { ArchitecturePage } from '@/pages/ArchitecturePage';
import { HonestStatusPage } from '@/pages/HonestStatusPage';
import { SecurityPage } from '@/pages/SecurityPage';
import { ConstitutionPage } from '@/pages/ConstitutionPage';
import { InstallPage } from '@/pages/InstallPage';
import { PrivacyPage } from '@/pages/PrivacyPage';
import { LegalNoticePage } from '@/pages/LegalNoticePage';
import { TermsPage } from '@/pages/TermsPage';
import { LicensesPage } from '@/pages/LicensesPage';
import { AccessibilityPage } from '@/pages/AccessibilityPage';
import { NotFoundPage } from '@/pages/NotFoundPage';

function AppShell({ initialRawPath }: { initialRawPath?: string }) {
  const { currentPath } = useRouter();
  const [spotlightOpen, setSpotlightOpen] = useState(false);
  const [a11yOpen, setA11yOpen] = useState(false);

  // Global keyboard invocation matching `hakille-ui-desktop`: Alt+Space or Ctrl/Cmd+K
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (
        (e.altKey && e.code === 'Space') ||
        ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'k')
      ) {
        e.preventDefault();
        setSpotlightOpen((prev) => !prev);
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  const rawCheck =
    initialRawPath ??
    (typeof window !== 'undefined' ? window.location.pathname : currentPath);
  const is404 = rawCheck === '/404' || !isKnownRoute(rawCheck);

  let pageView: React.ReactNode;
  if (is404 && currentPath === '/' && rawCheck !== '/') {
    pageView = <NotFoundPage onOpenSpotlight={() => setSpotlightOpen(true)} />;
  } else {
    switch (currentPath) {
      case '/architecture':
        pageView = <ArchitecturePage />;
        break;
      case '/etat-reel':
        pageView = <HonestStatusPage />;
        break;
      case '/securite':
        pageView = <SecurityPage />;
        break;
      case '/constitution':
        pageView = <ConstitutionPage />;
        break;
      case '/installer':
        pageView = <InstallPage />;
        break;
      case '/confidentialite':
        pageView = <PrivacyPage />;
        break;
      case '/mentions-legales':
        pageView = <LegalNoticePage />;
        break;
      case '/conditions':
        pageView = <TermsPage />;
        break;
      case '/licences':
        pageView = <LicensesPage />;
        break;
      case '/accessibilite':
        pageView = <AccessibilityPage />;
        break;
      case '/':
      default:
        pageView = <HomePage onOpenSpotlight={() => setSpotlightOpen(true)} />;
        break;
    }
  }

  return (
    <>
      <PageMeta />
      <SiteHeader
        onOpenSpotlight={() => setSpotlightOpen(true)}
        onOpenA11y={() => setA11yOpen(true)}
      />
      <main id="main-content" style={{ flex: 1 }}>
        {pageView}
      </main>
      <SiteFooter />
      <SpotlightHudModal open={spotlightOpen} onClose={() => setSpotlightOpen(false)} />
      <AccessibilityDrawer open={a11yOpen} onClose={() => setA11yOpen(false)} />
    </>
  );
}

export function App({
  initialPath,
  initialLocale,
}: {
  initialPath?: string;
  initialLocale?: LocaleCode;
}) {
  return (
    <PreferencesProvider>
      <I18nProvider initialLocale={initialLocale}>
        <RouterProvider initialPath={initialPath}>
          <AppShell initialRawPath={initialPath} />
        </RouterProvider>
      </I18nProvider>
    </PreferencesProvider>
  );
}
