import { LEGAL_CONFIG } from '@/config/legal';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';
import { ZeroExfiltrationInspector } from '@/components/simulators/ZeroExfiltrationInspector';

export function PrivacyPage() {
  const { locale, t } = useI18n();

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">
            RGPD (UE 2016/679) · DIRECTIVE EPRIVACY · EU AI ACT (2024/1689) · {LEGAL_CONFIG.software.lastLegalUpdate}
          </span>
          <h1 className="display-h1">{t.routes.privacy.title}</h1>
          <p className="hero-lead">{t.routes.privacy.description}</p>
          <PrintDossierButton />
        </div>
      </section>

      <div className="container page-content">
        <ZeroExfiltrationInspector />

        <div className="summary-callout">
          <div className="eyebrow-mono">{t.common.plainSummaryTitle}</div>
          <ul className="clean-list" style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
            <li>◆ <strong>{pickBi(locale, 'Sur ce site web (hakille.ai)', 'On this website (hakille.ai)')}</strong> : {pickBi(locale, '0 cookie de traçage, 0 script d’analytics tiers, 0 appel Google Fonts ou CDN externe. Vos préférences de thème et de langue restent dans votre seul navigateur.', '0 tracking cookies, 0 third-party analytics scripts, 0 Google Fonts or external CDN calls. Your theme and language preferences stay solely inside your browser.')}</li>
            <li>◆ <strong>{pickBi(locale, 'Dans le logiciel Hakille OS', 'Inside Hakille OS software')}</strong> : {pickBi(locale, 'Architecture 100 % local-first. Votre graphe mémoire redb et votre journal audit.redb résident sur votre machine ; l’éditeur n’y a techniquement aucun accès.', '100% local-first architecture. Your redb SPO memory graph and audit.redb ledger live on your own hardware; the project maintainers have zero technical access.')}</li>
            <li>◆ <strong>{pickBi(locale, 'Conformité Règlement Européen IA (AI Act)', 'EU AI Act Compliance')}</strong> : {pickBi(locale, 'Les trames audio/vidéo brutes restent en RAM volatile (zeroize) et tout signal affectif/postural (cognition:*) subit une purge physique sans tombstone en 5 minutes.', 'Raw audio/video frames remain in volatile RAM (zeroize) and any affective/postural cue (cognition:*) undergoes tombstone-free physical purge within 5 minutes.')}</li>
          </ul>
        </div>

        <div className="legal-layout">
          <aside className="legal-toc" aria-label="Table of contents">
            <strong style={{ fontSize: 'var(--hakille-type-caption-size)' }}>Articles</strong>
            <a href="#priv-1" className="nav-link">
              1. {pickBi(locale, 'Responsable & Périmètre', 'Controller & Scope')}
            </a>
            <a href="#priv-2" className="nav-link">
              2. {pickBi(locale, 'Site Web hakille.ai', 'Website hakille.ai')}
            </a>
            <a href="#priv-3" className="nav-link">
              3. {pickBi(locale, 'Logiciel Local-First', 'Local-First OS')}
            </a>
            <a href="#priv-4" className="nav-link">
              4. {pickBi(locale, 'Conformité AI Act UE', 'EU AI Act Safeguards')}
            </a>
            <a href="#priv-5" className="nav-link">
              5. {pickBi(locale, 'Escalade BYOM (300 s)', 'BYOM Lease (300 s)')}
            </a>
            <a href="#priv-6" className="nav-link">
              6. {pickBi(locale, 'Droits RGPD (Art. 15–22)', 'GDPR Rights (Art. 15–22)')}
            </a>
          </aside>

          <div className="legal-prose">
            <article id="priv-1" className="legal-article">
              <h2>1. {pickBi(locale, 'Responsable de Traitement & Double Périmètre', 'Controller Identity & Dual Scope')}</h2>
              <p>
                {pickBi(
                  locale,
                  `La présente politique s’applique distinctement (a) à la consultation du site vitrine statique ${LEGAL_CONFIG.domains.marketing} et (b) à l’exécution locale du logiciel open-source Hakille (${LEGAL_CONFIG.software.licenseSpdx}). Contact dédié protection des données : `,
                  `This policy governs separately (a) visits to the static showcase website ${LEGAL_CONFIG.domains.marketing} and (b) local execution of the open-source Hakille cognitive OS (${LEGAL_CONFIG.software.licenseSpdx}). Data protection contact: `
                )}
                <a href={`mailto:${LEGAL_CONFIG.contacts.privacyEmail}`}>{LEGAL_CONFIG.contacts.privacyEmail}</a>.
              </p>
            </article>

            <article id="priv-2" className="legal-article">
              <h2>2. {pickBi(locale, 'Confidentialité du Site Web (Zéro Cookie, Zéro CDN Tiers)', 'Website Privacy (Zero Cookies, Zero External CDNs)')}</h2>
              <p>
                {pickBi(
                  locale,
                  'Conformément à l’article 82 de la loi Informatique et Libertés et aux lignes directrices de la CNIL relatives aux traceurs (Directive ePrivacy 2002/58/CE), hakille.ai ne dépose aucun cookie publicitaire, analytique ou de profilage. Aucune bannière de consentement trompeuse n’est requise car aucun traçage n’a lieu.',
                  'In strict compliance with the EU ePrivacy Directive (2002/58/EC) and GDPR data-minimization principles (Art. 5.1.c), hakille.ai sets zero advertising, analytics, or cross-site tracking cookies.'
                )}
              </p>
              <p>
                {pickBi(
                  locale,
                  'Toutes les polices typographiques (InterVariable.ttf, SIL OFL 1.1) et ressources graphiques sont servies directement depuis la même origine (font-src \'self\'), empêchant toute fuite d’adresse IP vers des régies ou CDN américains (jurisprudence Schrems II / LG München sur Google Fonts). Les journaux techniques HTTP du serveur statique (adresse IP, horodatage, code statut) ne sont conservés que sur la base de l’intérêt légitime de sécurité réseau (Art. 6.1.f RGPD) pour une durée maximale de 14 jours.',
                  'All web fonts (InterVariable.ttf, SIL OFL 1.1) and brand emblems are self-hosted (font-src \'self\'), eliminating third-party IP leakage (Schrems II / Munich Regional Court Google Fonts ruling). Minimal static HTTP security logs are retained strictly under GDPR Art. 6(1)(f) legitimate network defense for at most 14 days.'
                )}
              </p>
            </article>

            <article id="priv-3" className="legal-article">
              <h2>3. {pickBi(locale, 'Traitement Local-First dans Hakille OS', 'Local-First Processing in Hakille OS')}</h2>
              <p>
                {pickBi(
                  locale,
                  'Lorsque vous exécutez hakille-daemon ou hakille-desktop sur votre équipement, vous demeurez l’unique maître matériel et juridique de vos données. Votre graphe de connaissances SPO (redb), votre journal d’audit signé (audit.redb) et vos graines cryptographiques (HAKKEY02) ne quittent jamais votre appareil vers nos serveurs : Hakille n’exploite aucun cloud central de collecte.',
                  'When running hakille-daemon or hakille-desktop on your hardware, you remain the sole custodian of your data. Your SPO memory graph (redb), signed audit ledger (audit.redb), and HAKKEY02 cryptographic seeds never phone home — Hakille operates zero centralized telemetry servers.'
                )}
              </p>
            </article>

            <article id="priv-4" className="legal-article">
              <h2>4. {pickBi(locale, 'Conformité au Règlement Européen sur l’IA (AI Act UE 2024/1689)', 'EU Artificial Intelligence Act (2024/1689) Safeguards')}</h2>
              <p>
                {pickBi(
                  locale,
                  'Conformément à la Règle Constitutionnelle RULE-08 et aux restrictions de l’AI Act européen sur l’inférence émotionnelle et biométrique : (1) aucun flux audio ou vidéo brut n’est écrit sur disque (mémoire vive uniquement suivie d’un effacement zeroize), (2) la vérification vocale CAM++ s’exécute exclusivement en local pour reconnaître le propriétaire légitime, et (3) tout signal affectif ou postural (cognition:*) est soumis à une demi-vie de 5 minutes suivie d’une purge physique sans tombstone.',
                  'Under Constitutional RULE-08 and EU AI Act biometric/emotion safeguards: (1) raw audio/video streams stay exclusively in volatile RAM (zeroize), (2) 192-d CAM++ speaker verification runs strictly offline on-device, and (3) any optional affective/postural cue (cognition:*) enforces a 5-minute half-life and physical tombstone-free deletion.'
                )}
              </p>
            </article>

            <article id="priv-5" className="legal-article">
              <h2>5. {pickBi(locale, 'Escalade Réseau Volontaire BYOM (Système 2)', 'Voluntary BYOM Network Escalation (System 2)')}</h2>
              <p>
                {pickBi(
                  locale,
                  'Si vous configurez volontairement votre propre clé API ou serveur distant (« Bring Your Own Model »), aucun octet n’est transmis sans un bail Biscuit explicite limité à 300 secondes maximum (RULE-09), affichant la taille exacte de la requête et son coût estimé, et consignant le transfert dans audit.redb. La politique de confidentialité du fournisseur que vous aurez choisi s’applique alors à ce seul appel.',
                  'If you voluntarily configure a remote "Bring Your Own Model" endpoint, zero bytes leave your machine without an explicit <= 300-second Biscuit lease (RULE-09), byte/cost preview, and permanent audit record in audit.redb.'
                )}
              </p>
            </article>

            <article id="priv-6" className="legal-article" style={{ borderBlockEnd: 'none' }}>
              <h2>6. {pickBi(locale, 'Exercice Direct de Vos Droits RGPD (Articles 15 à 22)', 'Direct Exercise of GDPR Rights (Articles 15–22)')}</h2>
              <ul className="clean-list">
                <li>
                  ◆ <strong>{pickBi(locale, 'Art. 15 (Accès) & Art. 22 (Explicabilité)', 'Art. 15 (Access) & Art. 22 (Explainability)')}</strong> :{' '}
                  {pickBi(locale, 'Feuille « Pourquoi ? » à 3 niveaux et inspection directe du graphe SPO.', '3-level "Why?" explainability sheet and direct SPO graph inspection.')}
                </li>
                <li>
                  ◆ <strong>{pickBi(locale, 'Art. 16 (Rectification)', 'Art. 16 (Rectification)')}</strong> :{' '}
                  {pickBi(locale, 'Bouton instantané « Ce fait est faux » sur tout fait mémoire consulté.', 'Instant "This fact is false" correction trigger on every memory triple.')}
                </li>
                <li>
                  ◆ <strong>{pickBi(locale, 'Art. 17 (Effacement / Droit à l’oubli)', 'Art. 17 (Erasure / Right to be Forgotten)')}</strong> :{' '}
                  {pickBi(locale, 'Suppression granulaire :forget et pacte successoral « Crépuscule Numérique ».', 'Granular :forget purge and "Digital Dusk" cryptographic succession.')}
                </li>
                <li>
                  ◆ <strong>{pickBi(locale, 'Art. 20 (Portabilité)', 'Art. 20 (Data Portability)')}</strong> :{' '}
                  {pickBi(locale, 'Export intégral sans verrou propriétaire au format documenté hakille-graph-export JSON v1.', 'Lock-in-free export in documented hakille-graph-export JSON v1 format.')}
                </li>
                <li>
                  ◆ <strong>{pickBi(locale, 'Autorité de contrôle', 'Supervisory Authority')}</strong> :{' '}
                  {pickBi(locale, 'Vous disposez du droit d’introduire une réclamation auprès de la CNIL (cnil.fr) ou de votre autorité nationale européenne de protection des données.', 'You may lodge a complaint with the French CNIL (cnil.fr) or your national EU Data Protection Authority.')}
                </li>
              </ul>
            </article>
          </div>
        </div>
      </div>
    </div>
  );
}
