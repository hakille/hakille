import { useState } from 'react';
import { useI18n } from '@/i18n/context';
import { pickBi } from '@/i18n/types';

export type GdprRightType = 'ACCESS_15' | 'RECTIFY_16' | 'ERASURE_17' | 'PORTABILITY_20';
export type MemoryPartition = 'personal' | 'professional' | 'shared';
export type ExportFormat = 'json' | 'csv' | 'markdown';

export function GdprRightsWizard() {
  const { locale } = useI18n();

  // Wizard State
  const [currentStep, setCurrentStep] = useState<1 | 2 | 3>(1);
  const [selectedRight, setSelectedRight] = useState<GdprRightType>('ERASURE_17');
  const [subjectId, setSubjectId] = useState('user_alice_42');
  const [partition, setPartition] = useState<MemoryPartition>('personal');
  const [predicate, setPredicate] = useState('location');
  const [disputeRationale, setDisputeRationale] = useState('Ce fait est faux : observation obsolète');
  const [erasureTopic, setErasureTopic] = useState<'cognition_affective' | 'entire_subject'>('entire_subject');
  const [exportFormat, setExportFormat] = useState<ExportFormat>('json');
  const [includeAuditSignatures, setIncludeAuditSignatures] = useState(true);
  const [copiedNotification, setCopiedNotification] = useState(false);
  const [simulationResult, setSimulationResult] = useState<string | null>(null);

  // Generate deterministic, verified CLI command
  const generatedCliCommand = (() => {
    switch (selectedRight) {
      case 'ACCESS_15':
        return `hakille-cli gdpr access --subject "${subjectId}" --partition ${partition} --format json`;
      case 'RECTIFY_16':
        return `hakille-cli gdpr rectify --subject "${subjectId}" --predicate "${predicate}" --dispute --rationale "${disputeRationale}"`;
      case 'ERASURE_17':
        if (erasureTopic === 'cognition_affective') {
          return `hakille-cli gdpr forget --subject "${subjectId}" --topic "cognition:*" --confirm-600ms`;
        }
        return `hakille-cli gdpr forget --subject "${subjectId}" --confirm-600ms`;
      case 'PORTABILITY_20':
        return `hakille-cli gdpr export --subject "${subjectId}" --format ${exportFormat}${includeAuditSignatures ? ' --with-audit-signatures' : ''}`;
    }
  })();

  const handleCopyCommand = async () => {
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      try {
        await navigator.clipboard.writeText(generatedCliCommand);
        setCopiedNotification(true);
        setTimeout(() => setCopiedNotification(false), 2400);
      } catch {
        // Fallback or ignore
      }
    }
  };

  const handleSimulateExecution = () => {
    switch (selectedRight) {
      case 'ACCESS_15':
        setSimulationResult(
          JSON.stringify(
            {
              status: '200_OK',
              partition,
              subject: subjectId,
              triplesCount: 3,
              triples: [
                { subject: subjectId, predicate: 'profile:language', object: 'fr', partition },
                { subject: subjectId, predicate: 'cognition:attention_budget', object: '3_per_day', partition },
                { subject: subjectId, predicate: 'security:biscuit_root', object: 'HAKKEY02_PINNED', partition },
              ],
              whySheetIncluded: true,
            },
            null,
            2
          )
        );
        break;
      case 'RECTIFY_16':
        setSimulationResult(
          JSON.stringify(
            {
              status: 'RECTIFIED',
              subject: subjectId,
              predicate,
              disputed: true,
              rationale: disputeRationale,
              auditCommitmentHex: '9f83ac0234a9b5f...sealed',
              effect: 'Triple immediately excluded from continuous Bayesian inference.',
            },
            null,
            2
          )
        );
        break;
      case 'ERASURE_17':
        setSimulationResult(
          JSON.stringify(
            {
              status: 'PHYSICALLY_PURGED',
              target: erasureTopic === 'cognition_affective' ? 'cognition:*' : subjectId,
              ruleEnforced: 'RULE-08',
              tombstoneRetained: false,
              zeroizeApplied: true,
              confirmationMode: '600ms_physical_hold',
              auditEntryId: 'entry_gdpr_17_purged',
            },
            null,
            2
          )
        );
        break;
      case 'PORTABILITY_20':
        setSimulationResult(
          JSON.stringify(
            {
              schema: 'hakille-graph-export-v1',
              format: exportFormat,
              subject: subjectId,
              recordsCount: 42,
              auditChainsCount: 1,
              auditRootHash: 'deadbeef11223344556677889900aabbccddeeff',
              license: 'CC0-1.0-Public-Domain-Export',
            },
            null,
            2
          )
        );
        break;
    }
  };

  return (
    <div className="sim-frame gdpr-wizard-shell">
      <div className="sim-header">
        <div style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
          <strong style={{ fontSize: '1.05rem', color: 'var(--hakille-texte-1)' }}>
            {pickBi(
              locale,
              'Assistant d’Exercice des Droits RGPD & Générateur CLI',
              'GDPR Rights Exercise Wizard & CLI Generator'
            )}
          </strong>
          <span style={{ fontSize: '0.78rem', color: 'var(--hakille-texte-2)' }}>
            {pickBi(
              locale,
              'Configurez vos paramètres et générez une commande cryptographiquement conforme pour votre terminal hakille-cli.',
              'Configure granular parameters and generate an authentic, verifiable command for hakille-cli.'
            )}
          </span>
        </div>
        <div className="wizard-step-indicator">
          <span className={`step-bubble ${currentStep === 1 ? 'active' : ''}`}>1</span>
          <span className="step-sep">―</span>
          <span className={`step-bubble ${currentStep === 2 ? 'active' : ''}`}>2</span>
          <span className="step-sep">―</span>
          <span className={`step-bubble ${currentStep === 3 ? 'active' : ''}`}>3</span>
        </div>
      </div>

      <div className="sim-body">
        {/* STEP 1: SELECT RIGHT */}
        {currentStep === 1 && (
          <div className="wizard-step-panel">
            <h3 className="wizard-step-title">
              {pickBi(locale, 'Étape 1 · Sélectionnez le droit RGPD à exercer', 'Step 1 · Select the GDPR right to exercise')}
            </h3>

            <div className="rights-cards-grid">
              <button
                type="button"
                className={`right-choice-card ${selectedRight === 'ACCESS_15' ? 'selected' : ''}`}
                onClick={() => setSelectedRight('ACCESS_15')}
              >
                <div className="right-card-header">
                  <span className="right-badge">Art. 15 RGPD</span>
                  <strong>{pickBi(locale, 'Droit d’Accès', 'Right of Access')}</strong>
                </div>
                <p>
                  {pickBi(
                    locale,
                    'Consulter tous les faits mémorisés dans le graphe redb et inspecter la feuille explicative « Pourquoi ? ».',
                    'Inspect all facts stored in the local redb memory graph and review the 3-level "Why?" explainability sheet.'
                  )}
                </p>
              </button>

              <button
                type="button"
                className={`right-choice-card ${selectedRight === 'RECTIFY_16' ? 'selected' : ''}`}
                onClick={() => setSelectedRight('RECTIFY_16')}
              >
                <div className="right-card-header">
                  <span className="right-badge">Art. 16 RGPD</span>
                  <strong>{pickBi(locale, 'Droit de Rectification', 'Right to Rectification')}</strong>
                </div>
                <p>
                  {pickBi(
                    locale,
                    'Contester un triplet (« Ce fait est faux ») pour l’exclure sans délai de toute inférence causale future.',
                    'Dispute a memory triple ("This fact is false") to immediately remove it from all future inference.'
                  )}
                </p>
              </button>

              <button
                type="button"
                className={`right-choice-card ${selectedRight === 'ERASURE_17' ? 'selected' : ''}`}
                onClick={() => setSelectedRight('ERASURE_17')}
              >
                <div className="right-card-header">
                  <span className="right-badge">Art. 17 RGPD</span>
                  <strong>{pickBi(locale, 'Droit à l’Effacement (Oubli)', 'Right to Erasure (Forgotten)')}</strong>
                </div>
                <p>
                  {pickBi(
                    locale,
                    'Purge physique immédiate sans tombstone (RULE-08), crémation cryptographique ou suppression de session.',
                    'Immediate tombstone-free physical deletion (RULE-08), cryptographic key cremation, or session wipe.'
                  )}
                </p>
              </button>

              <button
                type="button"
                className={`right-choice-card ${selectedRight === 'PORTABILITY_20' ? 'selected' : ''}`}
                onClick={() => setSelectedRight('PORTABILITY_20')}
              >
                <div className="right-card-header">
                  <span className="right-badge">Art. 20 RGPD</span>
                  <strong>{pickBi(locale, 'Droit à la Portabilité', 'Right to Portability')}</strong>
                </div>
                <p>
                  {pickBi(
                    locale,
                    'Exporter vos données au format standard hakille-graph-export JSON v1 avec chaînes d’audit signées.',
                    'Export your data in standard JSON v1 format, bundleable with cryptographic audit chains.'
                  )}
                </p>
              </button>
            </div>

            <div className="wizard-nav-row">
              <span />
              <button
                type="button"
                className="cta-button primary"
                onClick={() => setCurrentStep(2)}
              >
                {pickBi(locale, 'Configurer les Paramètres →', 'Configure Parameters →')}
              </button>
            </div>
          </div>
        )}

        {/* STEP 2: CONFIGURE PARAMETERS */}
        {currentStep === 2 && (
          <div className="wizard-step-panel">
            <h3 className="wizard-step-title">
              {pickBi(locale, 'Étape 2 · Paramétrage granulaire', 'Step 2 · Granular Configuration')}
            </h3>

            <div className="wizard-form-grid">
              <div className="form-field">
                <label htmlFor="wizard-subject-id" className="field-label">
                  {pickBi(locale, 'Identifiant du sujet / pseudonyme', 'Subject Identifier / Pseudonym')}
                </label>
                <input
                  id="wizard-subject-id"
                  type="text"
                  className="field-input mono"
                  value={subjectId}
                  onChange={(e) => setSubjectId(e.target.value)}
                />
              </div>

              {selectedRight === 'ACCESS_15' && (
                <div className="form-field">
                  <label htmlFor="wizard-partition-select" className="field-label">
                    {pickBi(locale, 'Partition mémoire cible', 'Target Memory Partition')}
                  </label>
                  <select
                    id="wizard-partition-select"
                    className="lang-select"
                    value={partition}
                    onChange={(e) => setPartition(e.target.value as MemoryPartition)}
                  >
                    <option value="personal">personal ({pickBi(locale, 'Privé / Équipement local', 'Private / Local device')})</option>
                    <option value="professional">professional ({pickBi(locale, 'Sphère travail', 'Work sphere')})</option>
                    <option value="shared">shared ({pickBi(locale, 'Sphère familiale / partagée', 'Family / Shared sphere')})</option>
                  </select>
                </div>
              )}

              {selectedRight === 'RECTIFY_16' && (
                <>
                  <div className="form-field">
                    <label htmlFor="wizard-predicate-input" className="field-label">
                      {pickBi(locale, 'Prédicat à contester', 'Predicate to Dispute')}
                    </label>
                    <input
                      id="wizard-predicate-input"
                      type="text"
                      className="field-input mono"
                      value={predicate}
                      onChange={(e) => setPredicate(e.target.value)}
                    />
                  </div>
                  <div className="form-field" style={{ gridColumn: '1 / -1' }}>
                    <label htmlFor="wizard-dispute-rationale" className="field-label">
                      {pickBi(locale, 'Motif de contestation (« Ce fait est faux »)', 'Dispute Rationale ("This fact is false")')}
                    </label>
                    <input
                      id="wizard-dispute-rationale"
                      type="text"
                      className="field-input"
                      value={disputeRationale}
                      onChange={(e) => setDisputeRationale(e.target.value)}
                    />
                  </div>
                </>
              )}

              {selectedRight === 'ERASURE_17' && (
                <div className="form-field" style={{ gridColumn: '1 / -1' }}>
                  <label htmlFor="wizard-erasure-topic" className="field-label">
                    {pickBi(locale, 'Périmètre d’effacement', 'Erasure Scope')}
                  </label>
                  <select
                    id="wizard-erasure-topic"
                    className="lang-select"
                    value={erasureTopic}
                    onChange={(e) => setErasureTopic(e.target.value as 'cognition_affective' | 'entire_subject')}
                  >
                    <option value="entire_subject">
                      {pickBi(locale, 'Effacement complet du sujet (--subject)', 'Complete subject wipe (--subject)')}
                    </option>
                    <option value="cognition_affective">
                      {pickBi(
                        locale,
                        'Purge physique des observations affectives (cognition:*)',
                        'Physical purge of affective cues (cognition:*)'
                      )}
                    </option>
                  </select>
                </div>
              )}

              {selectedRight === 'PORTABILITY_20' && (
                <>
                  <div className="form-field">
                    <label htmlFor="wizard-export-format" className="field-label">
                      {pickBi(locale, 'Format d’exportation', 'Export Format')}
                    </label>
                    <select
                      id="wizard-export-format"
                      className="lang-select"
                      value={exportFormat}
                      onChange={(e) => setExportFormat(e.target.value as ExportFormat)}
                    >
                      <option value="json">JSON v1 (hakille-graph-export)</option>
                      <option value="markdown">Markdown Documents</option>
                      <option value="csv">CSV Tables</option>
                    </select>
                  </div>
                  <div className="form-field" style={{ display: 'flex', alignItems: 'center', gap: '8px', paddingTop: '24px' }}>
                    <input
                      id="wizard-audit-sig-check"
                      type="checkbox"
                      checked={includeAuditSignatures}
                      onChange={(e) => setIncludeAuditSignatures(e.target.checked)}
                    />
                    <label htmlFor="wizard-audit-sig-check" style={{ fontSize: '0.82rem', color: 'var(--hakille-texte-1)' }}>
                      {pickBi(locale, 'Inclure les signatures Ed25519 de la chaîne d’audit', 'Include audit chain Ed25519 signatures')}
                    </label>
                  </div>
                </>
              )}
            </div>

            <div className="wizard-nav-row">
              <button
                type="button"
                className="cta-button secondary"
                onClick={() => setCurrentStep(1)}
              >
                {pickBi(locale, '← Retour', '← Back')}
              </button>
              <button
                type="button"
                className="cta-button primary"
                onClick={() => {
                  setSimulationResult(null);
                  setCurrentStep(3);
                }}
              >
                {pickBi(locale, 'Générer la Commande CLI →', 'Generate CLI Command →')}
              </button>
            </div>
          </div>
        )}

        {/* STEP 3: PREVIEW & RUN COMMAND */}
        {currentStep === 3 && (
          <div className="wizard-step-panel">
            <h3 className="wizard-step-title">
              {pickBi(locale, 'Étape 3 · Commande CLI Vérifiable & Simulation', 'Step 3 · Verifiable CLI Command & Simulation')}
            </h3>

            <div className="command-terminal-card">
              <div className="terminal-header">
                <span className="terminal-title">BASH / TERMINAL HAKILLE-CLI</span>
                <button
                  type="button"
                  className="copy-btn"
                  onClick={handleCopyCommand}
                >
                  {copiedNotification
                    ? pickBi(locale, '✓ Copié !', '✓ Copied!')
                    : pickBi(locale, 'Copier la commande', 'Copy command')}
                </button>
              </div>
              <pre className="terminal-code mono ltr-isolate">{generatedCliCommand}</pre>
            </div>

            <div className="simulation-actions-row">
              <button
                type="button"
                className="cta-button primary"
                onClick={handleSimulateExecution}
              >
                {pickBi(locale, 'Simuler l’Exécution Locale', 'Simulate Local Execution')}
              </button>
              <span style={{ fontSize: '0.78rem', color: 'var(--hakille-texte-3)' }}>
                {pickBi(
                  locale,
                  'Cette commande est 100 % conforme aux spécifications de crates/hakille-cli.',
                  'This command is 100% compliant with crates/hakille-cli specs.'
                )}
              </span>
            </div>

            {simulationResult && (
              <div className="simulation-result-box">
                <span className="eyebrow-mono">
                  {pickBi(locale, 'RÉSULTAT DE LA SIMULATION EN MÉMOIRE (REDB)', 'IN-MEMORY SIMULATION OUTPUT (REDB)')}
                </span>
                <pre className="mono ltr-isolate simulation-json">{simulationResult}</pre>
              </div>
            )}

            <div className="wizard-nav-row" style={{ marginTop: '20px' }}>
              <button
                type="button"
                className="cta-button secondary"
                onClick={() => setCurrentStep(2)}
              >
                {pickBi(locale, '← Modifier les paramètres', '← Edit parameters')}
              </button>
              <button
                type="button"
                className="cta-button secondary"
                onClick={() => {
                  setCurrentStep(1);
                  setSimulationResult(null);
                }}
              >
                {pickBi(locale, 'Recommencer un exercice', 'Start new exercise')}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
