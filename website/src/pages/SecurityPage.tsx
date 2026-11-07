import { lazy, Suspense } from 'react';
import { LEGAL_CONFIG } from '@/config/legal';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';
import { SimulatorSkeleton } from '@/components/common';

const BiscuitTokenLab = lazy(() =>
  import('@/components/simulators/BiscuitTokenLab').then((m) => ({
    default: m.BiscuitTokenLab,
  }))
);
const NotificationSecurityPolicyPlayground = lazy(() =>
  import('@/components/simulators/NotificationSecurityPolicyPlayground').then((m) => ({
    default: m.NotificationSecurityPolicyPlayground,
  }))
);

const THREAT_VECTORS = [
  {
    threatFr: 'Injection de prompt entrante (Courriel / Web / Document)',
    threatEn: 'Inbound Prompt Injection (Email / Web / Document)',
    defenseFr:
      'Tag immuable TrustLevel::Untrusted + blocage constitutionnel dur RULE-01 avant toute émission de capability.',
    defenseEn:
      'Immutable TrustLevel::Untrusted tag + Constitutional RULE-01 HardBlock prior to capability minting.',
    status: 'conformance',
  },
  {
    threatFr: 'Escalade de privilèges d’un sous-agent (Confused Deputy)',
    threatEn: 'Confused Deputy / Sub-Agent Privilege Escalation',
    defenseFr:
      'Blocs Datalog Biscuit v5 (RULE-04) : l’atténuation est cryptographiquement unidirectionnelle ; élargir un droit échoue à la vérification de signature.',
    defenseEn:
      'Biscuit v5 Datalog blocks (RULE-04): attenuation is cryptographically one-way; widening scope fails signature check.',
    status: 'hakille-auth',
  },
  {
    threatFr: 'Agent tiers malveillant ou boucle infinie CPU',
    threatEn: 'Malicious or Infinite-Loop Agent Plugin',
    defenseFr:
      'Bac à sable Wasmtime 48 zéro-import : aucun accès direct aux fichiers ou sockets hôtes, quota de fuel CPU et interruption par époque.',
    defenseEn:
      'Wasmtime 48 zero-import sandbox: zero host filesystem/socket imports, deterministic CPU fuel & epoch interruption.',
    status: 'hakille-agents',
  },
  {
    threatFr: 'Réécriture silencieuse du journal d’audit après compromission',
    threatEn: 'Silent Audit Log Rewrite After Compromise',
    defenseFr:
      'Journal append-only audit.redb chaîné par SHA-256 (H_i = SHA256(H_{i-1} || Entry_i)) et signé Ed25519.',
    defenseEn:
      'Append-only audit.redb chained via SHA-256 (H_i = SHA256(H_{i-1} || Entry_i)) and signed with Ed25519.',
    status: 'hakille-audit',
  },
  {
    threatFr: 'Action irréversible accidentelle / clic involontaire',
    threatEn: 'Unintended Irreversible Mutation / Misclick',
    defenseFr:
      'Maintien physique Braise obligatoire de 600 ms (RULE-02) + carte DryRunSimulator affichant coût, octets sortants et réversibilité.',
    defenseEn:
      'Mandatory 600 ms physical Braise hold (RULE-02) + pre-action DryRunSimulator cost/data/reversibility card.',
    status: 'hakille-design',
  },
  {
    threatFr: 'Coupure réseau mobile en plein envoi (double exécution)',
    threatEn: 'Mobile Network Drop Mid-Action (Double Execution)',
    defenseFr:
      'Durable Checkpoint v2 marque la mutation « unknown » sur disque et interdit tout rejeu aveugle automatique.',
    defenseEn:
      'Durable Checkpoint v2 marks interrupted mutations unknown and forbids automatic blind replay.',
    status: 'hakille-mobile',
  },
  {
    threatFr: 'Exfiltration prolongée vers un modèle cloud (Système 2)',
    threatEn: 'Unbounded Cloud Token Exfiltration (System 2)',
    defenseFr:
      'Bail réseau Biscuit explicite borné à <= 300 s (RULE-09) avec taximètre temps réel en kilooctets et centimes d’euro.',
    defenseEn:
      'Explicit <= 300 s Biscuit network lease (RULE-09) with live kilobyte and euro-cent taxi-meter.',
    status: 'hakille-kernel',
  },
] as const;

export function SecurityPage() {
  const { locale, t } = useI18n();

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">SECURITY.MD · RFC 9116 · BISCUIT V5 CAPABILITIES</span>
          <h1 className="display-h1">{t.routes.security.title}</h1>
          <p className="hero-lead">{t.routes.security.description}</p>
          <PrintDossierButton />
        </div>
      </section>

      <div className="container page-content">
        {/* Interactive Biscuit v5 Capability Token Inspector & Attenuator */}
        <section>
          <Suspense
            fallback={
              <SimulatorSkeleton
                title={pickBi(
                  locale,
                  'Inspecteur & Atténuateur de Tokens Biscuit v5',
                  'Biscuit v5 Capability Token Inspector & Attenuator'
                )}
                description={pickBi(
                  locale,
                  'Atténuation cryptographique Datalog, sandbox de sous-agents et preuve d’impossibilité d’escalade.',
                  'Cryptographic Datalog attenuation, sub-agent capability sandboxing, and monotonic restriction proof.'
                )}
                minHeight={580}
              />
            }
          >
            <BiscuitTokenLab />
          </Suspense>
        </section>

        {/* Constitutional Notification Security Policy Playground */}
        <section style={{ marginTop: '24px' }}>
          <Suspense
            fallback={
              <SimulatorSkeleton
                title={pickBi(
                  locale,
                  'Banc de Test des Politiques Constitutionnelles de Notification',
                  'Constitutional Notification Security Policy Playground'
                )}
                description={pickBi(
                  locale,
                  'Évaluation des règles constitutionnelles (Mineurs NoWallet, Sessions Invités, Confiance Biométrique).',
                  'Evaluation of constitutional rules (Rule 7 minors NoWallet, guest session isolation, biometric confidence).'
                )}
                minHeight={540}
              />
            }
          >
            <NotificationSecurityPolicyPlayground />
          </Suspense>
        </section>

        {/* 7-Vector Threat Matrix */}
        <section>
          <h2 className="section-h2" style={{ marginBottom: '16px' }}>
            {pickBi(
              locale,
              'Modèle de Menace à 7 Vecteurs & Défenses Structurelles',
              '7-Vector Threat Model & Structural Defenses'
            )}
          </h2>
          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th>{pickBi(locale, 'Vecteur de Menace', 'Threat Vector')}</th>
                  <th>{pickBi(locale, 'Mécanisme Structurel', 'Structural Mechanism')}</th>
                  <th>{pickBi(locale, 'Crate Vérifiée', 'Verified Crate')}</th>
                </tr>
              </thead>
              <tbody>
                {THREAT_VECTORS.map((row) => (
                  <tr key={row.threatEn}>
                    <td>
                      <strong style={{ color: 'var(--hakille-texte-1)' }}>
                        {pickBi(locale, row.threatFr, row.threatEn)}
                      </strong>
                    </td>
                    <td>{pickBi(locale, row.defenseFr, row.defenseEn)}</td>
                    <td>
                      <span className="status-tag">{row.status}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        {/* Coordinated Vulnerability Disclosure (RFC 9116) */}
        <section className="comparison-grid" style={{ marginBottom: 0 }}>
          <div className="arch-card">
            <span className="eyebrow-mono">RFC 9116 · /.well-known/security.txt</span>
            <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
              {pickBi(
                locale,
                'Divulgation Coordonnée de Vulnérabilités (SLA 72 h)',
                'Coordinated Vulnerability Disclosure (72h SLA)'
              )}
            </h2>
            <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
              {pickBi(
                locale,
                `Ne publiez jamais une faille critique dans une issue GitHub publique. Contactez directement l’équipe sécurité : accusé de réception sous ${LEGAL_CONFIG.contacts.disclosureAckHours} heures.`,
                `Never disclose a critical vulnerability in a public GitHub issue. Contact the security maintainers privately: acknowledgment within ${LEGAL_CONFIG.contacts.disclosureAckHours} hours.`
              )}
            </p>
            <div className="mono ltr-isolate" style={{ fontSize: '0.8rem', display: 'flex', flexDirection: 'column', gap: '6px' }}>
              <div>
                Email :{' '}
                <a href={`mailto:${LEGAL_CONFIG.contacts.securityEmail}`}>
                  {LEGAL_CONFIG.contacts.securityEmail}
                </a>
              </div>
              <div>
                GitHub :{' '}
                <a href={LEGAL_CONFIG.domains.securityAdvisories}>
                  Private Security Advisory ↗
                </a>
              </div>
              <div>
                Policy : <a href="/.well-known/security.txt">/.well-known/security.txt</a>
              </div>
            </div>
          </div>

          <div className="arch-card">
            <span className="eyebrow-mono">
              {pickBi(locale, 'LIMITES HONNÊTES EN v0.1', 'HONEST BOUNDARIES IN v0.1')}
            </span>
            <h2 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
              {pickBi(
                locale,
                'Ce qui N’EST PAS encore garanti en v0.1',
                'What v0.1 Does NOT Yet Guarantee'
              )}
            </h2>
            <ul className="clean-list" style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
              <li>
                —{' '}
                {pickBi(
                  locale,
                  'Les fichiers de base redb sont protégés par les permissions utilisateur de l’OS et pas encore chiffrés au repos (cible v0.2 ; les graines racines utilisent déjà le Keychain OS ou HAKKEY02 Argon2id + ChaCha20-Poly1305).',
                  'redb database files are protected by OS user permissions, not yet encrypted at rest (planned for v0.2; root seeds already use OS Keychain / HAKKEY02 Argon2id + ChaCha20-Poly1305).'
                )}
              </li>
              <li>
                —{' '}
                {pickBi(
                  locale,
                  'Aucune défense contre un noyau OS root compromis ou une attaque physique cold-boot sans chiffrement de disque système actif (LUKS / FileVault / BitLocker).',
                  'No defense against a compromised root OS kernel or physical cold-boot attack without host full-disk encryption.'
                )}
              </li>
              <li>
                —{' '}
                {pickBi(
                  locale,
                  'Aucun audit cryptographique tiers indépendant n’a encore été mené sur cet arbre pré-alpha.',
                  'Independent external cryptographic audit has not yet been conducted on this pre-alpha tree.'
                )}
              </li>
            </ul>
          </div>
        </section>
      </div>
    </div>
  );
}
