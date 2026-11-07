import { useState } from 'react';
import { LEGAL_CONFIG } from '@/config/legal';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';

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
  const [attenuatedReadOnly, setAttenuatedReadOnly] = useState(true);
  const [attemptedEscalation, setAttemptedEscalation] = useState(false);

  const biscuitValid = !attemptedEscalation;

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
        {/* Interactive Biscuit Attenuation Verifier */}
        <section className="sim-shell">
          <div className="sim-header">
            <strong>
              {pickBi(
                locale,
                'Vérificateur Interactif d’Atténuation Monotone Biscuit (RULE-04)',
                'Interactive Biscuit Monotonic Attenuation Verifier (RULE-04)'
              )}
            </strong>
            <span className="status-tag">
              {biscuitValid
                ? pickBi(locale, 'DATALOG VALIDE · AUTORISÉ', 'DATALOG VERIFICATION: PASS')
                : pickBi(locale, 'BLOCAGE DUR · ESCALADE REJETÉE', 'HARD BLOCK · ESCALATION REJECTED')}
            </span>
          </div>
          <div className="sim-body comparison-grid" style={{ marginBottom: 0 }}>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
              <div className="eyebrow-mono">
                Block 0 (Authority Root) + Block 1 (Sub-Agent Attenuation)
              </div>
              <pre
                style={{
                  padding: '14px',
                  borderRadius: '8px',
                  background: 'var(--hakille-surface-0)',
                  border: '1px solid var(--hakille-ligne)',
                  overflowX: 'auto',
                  fontSize: '0.78rem',
                }}
              >{`// Block 0: Root User Seed (Ed25519)
right("mail:inbox", "read");
right("mail:drafts", "write");
check if time($t), $t <= "2026-11-06T09:35:00Z"; // 300s TTL

// Block 1: Delegated to Summarizer Sub-Agent
${attenuatedReadOnly ? 'check if operation("read"); // Narrowed to read-only' : '// No read-only restriction'}
${attemptedEscalation ? 'right("wallet:transfer", "execute"); // ILLEGAL ESCALATION ATTEMPT!' : '// Zero privilege escalation'}`}</pre>
            </div>

            <div style={{ display: 'flex', flexDirection: 'column', gap: '12px', justifyContent: 'center' }}>
              <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <input
                  type="checkbox"
                  checked={attenuatedReadOnly}
                  onChange={(e) => setAttenuatedReadOnly(e.target.checked)}
                />
                <span>
                  {pickBi(
                    locale,
                    'Restreindre le sous-agent strictement à operation("read")',
                    'Attenuate sub-agent strictly to operation("read")'
                  )}
                </span>
              </label>
              <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <input
                  type="checkbox"
                  checked={attemptedEscalation}
                  onChange={(e) => setAttemptedEscalation(e.target.checked)}
                />
                <span>
                  {pickBi(
                    locale,
                    'Simuler un sous-agent tentant d’ajouter right("wallet:transfer")',
                    'Simulate sub-agent appending right("wallet:transfer")'
                  )}
                </span>
              </label>

              <div className="arch-card" style={{ padding: '16px' }}>
                <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)' }}>
                  {biscuitValid
                    ? pickBi(
                        locale,
                        'Jeton Biscuit valide : le Bloc 1 ajoute uniquement des prédicats restrictifs check if signés le long de la chaîne.',
                        'Valid Biscuit token: Block 1 only adds restrictive check if predicates signed along the chain.'
                      )
                    : pickBi(
                        locale,
                        'Rejeté par hakille-auth : en Datalog Biscuit, tout fait d’autorité right(...) hors du Bloc 0 est ignoré/rejeté. Un sous-agent ne peut mathématiquement JAMAIS élargir ses propres droits.',
                        'Rejected by hakille-auth: In Biscuit Datalog, authority facts right(...) outside Block 0 are rejected. A sub-agent can mathematically NEVER widen its own permissions.'
                      )}
                </p>
              </div>
            </div>
          </div>
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
            <div className="mono" style={{ fontSize: '0.8rem', display: 'flex', flexDirection: 'column', gap: '6px' }}>
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
