import { describe, expect, it } from 'vitest';
import { renderToString } from 'react-dom/server';
import { ALL_DICTIONARIES, I18nProvider } from '@/i18n/context';
import { SUPPORTED_LOCALES, type LocaleCode } from '@/i18n/types';
import { CognitiveNotificationRouterLab } from '@/components/simulators/CognitiveNotificationRouterLab';
import { BiscuitTokenLab } from '@/components/simulators/BiscuitTokenLab';
import { DesktopHudToastSandbox } from '@/components/simulators/DesktopHudToastSandbox';
import { NotificationSecurityPolicyPlayground } from '@/components/simulators/NotificationSecurityPolicyPlayground';

function decodeHtml(html: string): string {
  return html
    .replace(/&amp;/g, '&')
    .replace(/&#x27;/g, "'")
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>');
}

describe('Milestone 3: 4 Simulator Labs Dynamic 8-Locale i18n & Parity', () => {
  describe('1. Recursive Key Completeness and Non-Empty Assertions', () => {
    it('verifies smnfLab, biscuitLab, hudToastSandbox, and policyPlayground exist on all 8 locales', () => {
      for (const loc of SUPPORTED_LOCALES) {
        const dict = ALL_DICTIONARIES[loc];
        expect(dict.smnfLab).toBeDefined();
        expect(dict.biscuitLab).toBeDefined();
        expect(dict.hudToastSandbox).toBeDefined();
        expect(dict.policyPlayground).toBeDefined();

        // Check smnfLab
        expect(dict.smnfLab.title.length).toBeGreaterThan(0);
        expect(dict.smnfLab.subtitle.length).toBeGreaterThan(0);
        expect(dict.smnfLab.requestSection.title.length).toBeGreaterThan(0);
        expect(dict.smnfLab.requestSection.fieldTitle.length).toBeGreaterThan(0);
        expect(dict.smnfLab.cognitiveSensors.title.length).toBeGreaterThan(0);
        expect(dict.smnfLab.silenceBudget.label.length).toBeGreaterThan(0);
        expect(dict.smnfLab.channels.label.length).toBeGreaterThan(0);
        expect(dict.smnfLab.visualStyles.label.length).toBeGreaterThan(0);
        expect(dict.smnfLab.decisionCard.title.length).toBeGreaterThan(0);
        expect(dict.smnfLab.auditionBtn.label.length).toBeGreaterThan(0);
        expect(dict.smnfLab.ipcPreview.title.length).toBeGreaterThan(0);

        // Check biscuitLab
        expect(dict.biscuitLab.title.length).toBeGreaterThan(0);
        expect(dict.biscuitLab.subtitle.length).toBeGreaterThan(0);
        expect(dict.biscuitLab.blockChain.title.length).toBeGreaterThan(0);
        expect(dict.biscuitLab.authorityLabels.rootAuthority.length).toBeGreaterThan(0);
        expect(dict.biscuitLab.scopes.title.length).toBeGreaterThan(0);
        expect(dict.biscuitLab.actions.rootLabel.length).toBeGreaterThan(0);
        expect(dict.biscuitLab.leaseSlider.label.length).toBeGreaterThan(0);
        expect(dict.biscuitLab.attacks.title.length).toBeGreaterThan(0);
        expect(dict.biscuitLab.verifyConsole.title.length).toBeGreaterThan(0);

        // Check hudToastSandbox
        expect(dict.hudToastSandbox.title.length).toBeGreaterThan(0);
        expect(dict.hudToastSandbox.subtitle.length).toBeGreaterThan(0);
        expect(dict.hudToastSandbox.previewStage.osBrand.length).toBeGreaterThan(0);
        expect(dict.hudToastSandbox.configurator.title.length).toBeGreaterThan(0);
        expect(dict.hudToastSandbox.actions.triggerToastBtn.length).toBeGreaterThan(0);
        expect(dict.hudToastSandbox.accessKitInspector.title.length).toBeGreaterThan(0);

        // Check policyPlayground
        expect(dict.policyPlayground.title.length).toBeGreaterThan(0);
        expect(dict.policyPlayground.subtitle.length).toBeGreaterThan(0);
        expect(dict.policyPlayground.subjectProfiles.title.length).toBeGreaterThan(0);
        expect(dict.policyPlayground.emissionRequest.title.length).toBeGreaterThan(0);
        expect(dict.policyPlayground.verdictBanner.title.length).toBeGreaterThan(0);
        expect(dict.policyPlayground.rulesCited.rule01Title.length).toBeGreaterThan(0);
      }
    });

    it('asserts zero AGI hype in all 4 simulator dictionaries across all 8 locales', () => {
      for (const loc of SUPPORTED_LOCALES) {
        const dict = ALL_DICTIONARIES[loc];
        const serialized = JSON.stringify({
          smnf: dict.smnfLab,
          biscuit: dict.biscuitLab,
          hud: dict.hudToastSandbox,
          policy: dict.policyPlayground,
        });
        expect(serialized).not.toMatch(/\bAGI\b/);
      }
    });
  });

  describe('2. Dynamic Multilingual Rendering of all 4 Simulators', () => {
    it.each(SUPPORTED_LOCALES)(
      'renders CognitiveNotificationRouterLab with locale %s and reflects translated strings',
      (locale: LocaleCode) => {
        const dict = ALL_DICTIONARIES[locale];
        const rawHtml = renderToString(
          <I18nProvider initialLocale={locale}>
            <CognitiveNotificationRouterLab />
          </I18nProvider>
        );
        const html = decodeHtml(rawHtml);

        expect(html).toContain(dict.smnfLab.title);
        expect(html).toContain(dict.smnfLab.requestSection.title);
        expect(html).toContain(dict.smnfLab.cognitiveSensors.title);
        expect(html).toContain(dict.smnfLab.decisionCard.title);
        expect(html).toContain(dict.smnfLab.auditionBtn.label);
      }
    );

    it.each(SUPPORTED_LOCALES)(
      'renders BiscuitTokenLab with locale %s and reflects translated strings',
      (locale: LocaleCode) => {
        const dict = ALL_DICTIONARIES[locale];
        const rawHtml = renderToString(
          <I18nProvider initialLocale={locale}>
            <BiscuitTokenLab />
          </I18nProvider>
        );
        const html = decodeHtml(rawHtml);

        expect(html).toContain(dict.biscuitLab.title);
        expect(html).toContain(dict.biscuitLab.blockChain.title);
        expect(html).toContain(dict.biscuitLab.scopes.title);
        expect(html).toContain(dict.biscuitLab.attacks.title);
        expect(html).toContain(dict.biscuitLab.verifyConsole.title);
      }
    );

    it.each(SUPPORTED_LOCALES)(
      'renders DesktopHudToastSandbox with locale %s and reflects translated strings',
      (locale: LocaleCode) => {
        const dict = ALL_DICTIONARIES[locale];
        const rawHtml = renderToString(
          <I18nProvider initialLocale={locale}>
            <DesktopHudToastSandbox />
          </I18nProvider>
        );
        const html = decodeHtml(rawHtml);

        expect(html).toContain(dict.hudToastSandbox.title);
        expect(html).toContain(dict.hudToastSandbox.configurator.title);
        expect(html).toContain(dict.hudToastSandbox.accessKitInspector.title);
      }
    );

    it.each(SUPPORTED_LOCALES)(
      'renders NotificationSecurityPolicyPlayground with locale %s and reflects translated strings',
      (locale: LocaleCode) => {
        const dict = ALL_DICTIONARIES[locale];
        const rawHtml = renderToString(
          <I18nProvider initialLocale={locale}>
            <NotificationSecurityPolicyPlayground />
          </I18nProvider>
        );
        const html = decodeHtml(rawHtml);

        expect(html).toContain(dict.policyPlayground.title);
        expect(html).toContain(dict.policyPlayground.subjectProfiles.title);
        expect(html).toContain(dict.policyPlayground.emissionRequest.title);
        expect(html).toContain(dict.policyPlayground.verdictBanner.title);
      }
    );
  });
});
