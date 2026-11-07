import { LEGAL_CONFIG } from '@/config/legal';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';

export function LegalNoticePage() {
  const { locale, t } = useI18n();

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">ARTICLE 6-III LCEN (LOI N° 2004-575) · OPEN-SOURCE GOVERNANCE</span>
          <h1 className="display-h1">{t.routes.legalNotice.title}</h1>
          <p className="hero-lead">{t.routes.legalNotice.description}</p>
          <PrintDossierButton />
        </div>
      </section>

      <div className="container page-content">
        <div className="legal-prose">
          <article className="legal-article">
            <h2>1. {pickBi(locale, 'Édition & Gouvernance du Projet', 'Project Publisher & Open-Source Governance')}</h2>
            <p>
              <strong>Nom du projet :</strong> {LEGAL_CONFIG.brand.formalName} (mot-symbole <code>{LEGAL_CONFIG.brand.wordmark}</code> — <em>{LEGAL_CONFIG.brand.signatureFr}</em>)
            </p>
            <p>
              <strong>Entité éditrice :</strong> {pickBi(locale, LEGAL_CONFIG.publisher.entityLabelFr, LEGAL_CONFIG.publisher.entityLabelEn)}
            </p>
            <p>
              <strong>Statut d’immatriculation :</strong> {LEGAL_CONFIG.publisher.registrationNumberPlaceholder}
            </p>
            <p>
              <strong>Domiciliation / Siège :</strong> {LEGAL_CONFIG.publisher.registeredSeatPlaceholder}
            </p>
            <p>
              <strong>Direction de la publication :</strong> {LEGAL_CONFIG.publisher.publicationGovernance} (<a href={LEGAL_CONFIG.domains.repository}>{LEGAL_CONFIG.domains.repository}</a>)
            </p>
            <p>
              <strong>Contact général :</strong> <a href={`mailto:${LEGAL_CONFIG.contacts.generalEmail}`}>{LEGAL_CONFIG.contacts.generalEmail}</a> · <strong>Sécurité :</strong> <a href={`mailto:${LEGAL_CONFIG.contacts.securityEmail}`}>{LEGAL_CONFIG.contacts.securityEmail}</a>
            </p>
          </article>

          <article className="legal-article">
            <h2>2. {pickBi(locale, 'Hébergement & Distribution du Code', 'Hosting Architecture & Source Repository')}</h2>
            <p>
              {pickBi(locale, LEGAL_CONFIG.hosting.architectureFr, LEGAL_CONFIG.hosting.architectureEn)}
            </p>
            <p>
              <strong>Hébergement du dépôt source public :</strong> {LEGAL_CONFIG.hosting.codeHostName}, {LEGAL_CONFIG.hosting.codeHostAddress} (<a href={LEGAL_CONFIG.domains.repository}>{LEGAL_CONFIG.domains.repository}</a>).
            </p>
            <p>
              {pickBi(locale, LEGAL_CONFIG.hosting.edgeNoteFr, LEGAL_CONFIG.hosting.edgeNoteEn)}
            </p>
          </article>

          <article className="legal-article">
            <h2>3. {pickBi(locale, 'Propriété Intellectuelle, Marques & Note Historique', 'Intellectual Property, Trademarks & Historical Note')}</h2>
            <p>
              {pickBi(
                locale,
                'L’intégralité du code source de Hakille et de ce site web est distribuée sous licence open-source Apache License, Version 2.0 (LICENSE-APACHE). Conformément à la section 6 de la licence Apache-2.0, la concession de licence sur le code n’emporte pas concession d’usage commercial trompeur du nom « Hakille », du mot-symbole hakille ou des emblèmes officiels situés dans /logo/ pour désigner des fourches modifiées non conformes à la Constitution v1.0.0.',
                'All Hakille source code and this website are licensed under the open-source Apache License, Version 2.0 (LICENSE-APACHE). Pursuant to Section 6 of Apache-2.0, the software license does not grant permission to use the "Hakille" name, hakille wordmark, or official /logo/ emblems to endorse modified forks that violate Constitution v1.0.0.'
              )}
            </p>
            <p>
              <strong>Note de continuité technique (docs/RENAMING_TO_HAKILLE.md) :</strong>{' '}
              {pickBi(
                locale,
                `Durant sa phase initiale de recherche interne, certains artefacts techniques portaient le nom de code provisoire « ${LEGAL_CONFIG.brand.historicalTechnicalCodename} ». Le projet opère désormais exclusivement sous l’identité unique Hakille.`,
                `During early internal R&D, certain technical artifacts used the provisional codename "${LEGAL_CONFIG.brand.historicalTechnicalCodename}". The project operates exclusively under the single unified identity Hakille.`
              )}
            </p>
          </article>
        </div>
      </div>
    </div>
  );
}
