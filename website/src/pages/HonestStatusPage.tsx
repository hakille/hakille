import { useState } from 'react';
import { MODELS_CATALOG, ModelHonestState } from '@/data/modelsCatalog';
import { WORKSPACE_METRICS } from '@/data/cratesCatalog';
import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { PrintDossierButton } from '@/components/primitives/PrintDossierButton';

export function HonestStatusPage() {
  const { locale, t } = useI18n();
  const [filter, setFilter] = useState<'all' | ModelHonestState>('all');

  const rows =
    filter === 'all'
      ? MODELS_CATALOG
      : MODELS_CATALOG.filter((m) => m.honestState === filter);

  return (
    <div>
      <section className="page-hero">
        <div className="container" style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          <span className="eyebrow-mono">
            {pickBi(
              locale,
              'HONNÊTETÉ D’INGÉNIERIE RADICALE · v0.1.0-PRÉ-ALPHA',
              'RADICAL ENGINEERING HONESTY · v0.1.0-PRE-ALPHA'
            )}
          </span>
          <h1 className="display-h1">{t.routes.honestStatus.title}</h1>
          <p className="hero-lead">{t.routes.honestStatus.description}</p>
          <PrintDossierButton />
        </div>
      </section>

      <div className="container page-content">
        <div className="comparison-grid" style={{ marginBottom: 0 }}>
          <div className="arch-card" data-emphasis="lueur">
            <span className="status-tag">
              {pickBi(
                locale,
                'RÉEL & TESTÉ DE BOUT EN BOUT EN v0.1',
                'REAL & GOLDEN-TESTED IN v0.1'
              )}
            </span>
            <ul className="clean-list" style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
              <li>
                ◆{' '}
                <strong className="ltr-isolate">
                  {pickBi(
                    locale,
                    `${WORKSPACE_METRICS.totalAutomatedTests.toLocaleString('fr-FR')} tests automatisés`,
                    `${WORKSPACE_METRICS.totalAutomatedTests.toLocaleString('en-US')} automated tests`
                  )}
                </strong>{' '}
                <span className="ltr-isolate">
                  {pickBi(
                    locale,
                    `(${WORKSPACE_METRICS.conformanceFastTests} rapides + ${WORKSPACE_METRICS.conformanceSlowTests} conformité longue) validés sur ${WORKSPACE_METRICS.totalPackages} crates Cargo.`,
                    `(${WORKSPACE_METRICS.conformanceFastTests} fast + ${WORKSPACE_METRICS.conformanceSlowTests} slow conformance suites) passing across ${WORKSPACE_METRICS.totalPackages} packages.`
                  )}
                </span>
              </li>
              <li>
                ◆{' '}
                <strong>
                  {pickBi(locale, 'Inférence CPU Locale Réelle', 'Real CPU Inference')}
                </strong>
                :{' '}
                {pickBi(
                  locale,
                  'Whisper Tiny 99 langues accéléré ×46 via FP32 GEMM (~1,4 s), Qwen 2.5 0.5B GGUF via Candle, Silero VAD, CAM++ 192-d, MiniLM-L6 384-d HNSW et codec neuronal Kyutai Mimi 24 kHz.',
                  'Whisper Tiny 99-lang accelerated ×46 via FP32 GEMM (~1.4 s), Qwen 2.5 0.5B GGUF via Candle, Silero VAD, CAM++ 192-d, MiniLM-L6 384-d HNSW, Kyutai Mimi 24 kHz codec.'
                )}
              </li>
              <li>
                ◆{' '}
                <strong>
                  {pickBi(locale, 'Audio Pur Rust Sans Poids (0 Mo)', 'Pure-Rust Zero-Weight Audio')}
                </strong>
                :{' '}
                {pickBi(
                  locale,
                  'Synthèse vocale Klatt à 5 formants (2 511 lignes Rust + 11 visèmes), extracteur YIN F0 et cordes Kora 48 kHz Karplus-Strong.',
                  'Klatt 5-Formant TTS (2,511 LoC + 11 visemes), YIN F0 tracker, and 48 kHz Karplus-Strong UI signals.'
                )}
              </li>
              <li>
                ◆{' '}
                <strong>
                  {pickBi(locale, 'Noyau Cryptographique', 'Cryptographic Core')}
                </strong>
                :{' '}
                {pickBi(
                  locale,
                  'Atténuation Biscuit v5, chaîne audit.redb SHA-256 + Ed25519, coffre HAKKEY02 Argon2id + ChaCha20-Poly1305 et bac à sable Wasmtime 48 zéro-import (echo-agent, count-agent).',
                  'Biscuit v5 attenuation, audit.redb SHA-256 + Ed25519 chain, HAKKEY02 Argon2id + ChaCha20-Poly1305 keystore, Wasmtime 48 zero-import (echo-agent, count-agent).'
                )}
              </li>
              <li>
                ◆{' '}
                <strong>
                  {pickBi(locale, 'Surfaces Natives', 'Native Shells')}
                </strong>
                :{' '}
                {pickBi(
                  locale,
                  'Shell desktop GPU winit + wgpu + AccessKit (0,0 fps au repos) et compagnons iOS SwiftUI + Android Compose via UniFFI 0.32.2 avec Checkpoint v2.',
                  'winit + wgpu + AccessKit desktop HUD + iOS SwiftUI & Android Compose via UniFFI 0.32.2 with Checkpoint v2.'
                )}
              </li>
            </ul>
          </div>

          <div className="arch-card">
            <span className="status-tag">
              {pickBi(
                locale,
                'SIMULÉ OU NON PROVISIONNÉ EN v0.1',
                'SIMULATED OR UNPROVISIONED IN v0.1'
              )}
            </span>
            <ul className="clean-list" style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
              <li>
                —{' '}
                <strong>
                  {pickBi(locale, 'Modèle vocal Moshi 7.6B', 'Moshi 7.6B Spoken Dialogue LLM')}
                </strong>
                :{' '}
                {pickBi(
                  locale,
                  'Le codec neuronal Mimi 24 kHz tourne réellement, mais le LLM vocal full-duplex Moshi 7.6B n’est PAS encore provisionné.',
                  'Mimi neural codec runs for real, but the 7.6B full-duplex Moshi dialogue model is NOT provisioned yet.'
                )}
              </li>
              <li>
                —{' '}
                <strong>
                  {pickBi(locale, '5 Agents Métiers Domaine', '5 Domain Business Agents')}
                </strong>
                :{' '}
                {pickBi(
                  locale,
                  'mail, calendar, code, home et wallet-readonly livrent leurs manifestes de capabilities et sondes WASM ; l’exécution SMTP/CalDAV/bancaire réelle est bridée vers outbox.json durable.',
                  'mail, calendar, code, home, and wallet-readonly ship declarative capability manifests and probe WASMs; real SMTP/CalDAV/bank execution is gated to durable outbox.json.'
                )}
              </li>
              <li>
                —{' '}
                <strong>
                  {pickBi(locale, 'Chiffrement redb au repos', 'Storage Encryption at Rest')}
                </strong>
                :{' '}
                {pickBi(
                  locale,
                  'Les graines HAKKEY02 sont chiffrées (Argon2id / Keychain OS), tandis que le chiffrement transparent intégral des fichiers redb cible la v0.2.',
                  'HAKKEY02 seeds are encrypted/keychain-backed, whereas full-file redb transparent disk encryption targets v0.2.'
                )}
              </li>
              <li>
                —{' '}
                <strong>
                  {pickBi(locale, 'Attestation Matérielle & Image Redox', 'Hardware Attestation')}
                </strong>
                :{' '}
                {pickBi(
                  locale,
                  'L’attestation TPM 2.0 / Secure Enclave et les images bootables Redox OS relèvent des Phases 2 et 3.',
                  'TPM 2.0 / Apple Secure Enclave hardware attestation and bootable Redox OS appliance images are Phase 2–3 milestones.'
                )}
              </li>
            </ul>
          </div>
        </div>

        <section>
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              flexWrap: 'wrap',
              gap: '12px',
              marginBottom: '16px',
            }}
          >
            <h2 className="section-h2">
              {pickBi(
                locale,
                'Catalogue Complet des 15 Moteurs DSP & Modèles IA (catalog.rs)',
                'Complete 15-Engine DSP & Neural Catalog (catalog.rs)'
              )}
            </h2>
            <div style={{ display: 'flex', gap: '6px', flexWrap: 'wrap' }}>
              {(
                [
                  ['all', t.common.filterAll],
                  ['embedded-rust-dsp', pickBi(locale, 'DSP Pur Rust (2)', 'Pure Rust DSP (2)')],
                  ['verified-local-inference', pickBi(locale, 'CPU Vérifié (7)', 'Verified CPU (7)')],
                  ['cataloged-partial', pickBi(locale, 'Catalogué Partiel (5)', 'Cataloged Partial (5)')],
                  ['unprovisioned-roadmap', pickBi(locale, 'Feuille de route (1)', 'Roadmap (1)')],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  className="state-pill"
                  aria-pressed={filter === id}
                  onClick={() => setFilter(id)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th>{pickBi(locale, 'ID / Modèle', 'ID / Model')}</th>
                  <th>{pickBi(locale, 'Rôle', 'Role')}</th>
                  <th>{pickBi(locale, 'Exécution & Format', 'Runtime & Format')}</th>
                  <th>{pickBi(locale, 'Disque / RAM', 'Disk / RAM')}</th>
                  <th>{pickBi(locale, 'Statut v0.1', 'Honest v0.1 Status')}</th>
                  <th>{pickBi(locale, 'Notes d’Ingénierie', 'Engineering Notes')}</th>
                </tr>
              </thead>
              <tbody>
                {rows.map((m) => (
                  <tr key={m.id}>
                    <td>
                      <strong style={{ color: 'var(--hakille-texte-1)', display: 'block' }}>
                        {m.name}
                      </strong>
                      <code style={{ color: 'var(--hakille-texte-3)', fontSize: '0.72rem' }}>
                        {m.id}
                      </code>
                    </td>
                    <td>{pickBi(locale, m.roleFr, m.roleEn)}</td>
                    <td className="mono ltr-isolate">
                      {m.runtime} · {m.format}
                    </td>
                    <td className="mono tabular ltr-isolate">
                      {m.diskSizeLabel} · {m.ramMb} MB
                    </td>
                    <td>
                      <span className="status-tag">{m.honestState}</span>
                    </td>
                    <td>{pickBi(locale, m.honestNoteFr, m.honestNoteEn)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>
      </div>
    </div>
  );
}
