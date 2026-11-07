import { useMemo, useState } from 'react';
import { CapabilityAction, CapabilityScope } from '@/engine/biscuitEngine';
import {
  BIOMETRIC_STEP_UP_THRESHOLDS,
  PolicySubject,
  evaluateSecurityPolicy,
} from '@/engine/notificationPolicyEngine';
import { useI18n } from '@/i18n/context';

export function NotificationSecurityPolicyPlayground() {
  const { t } = useI18n();

  // Subject Profile State
  const [role, setRole] = useState<PolicySubject['role']>('Owner');
  const [age, setAge] = useState<number>(32);
  const [biometricConfidence, setBiometricConfidence] = useState<number>(0.95);
  const [originTrust, setOriginTrust] = useState<PolicySubject['originTrust']>('TrustedHuman');

  // Emission Request State
  const [targetScope, setTargetScope] = useState<CapabilityScope>('Notification');
  const [action, setAction] = useState<CapabilityAction>('Write');
  const [isDestructive, setIsDestructive] = useState<boolean>(false);

  const subject: PolicySubject = useMemo(
    () => ({
      role,
      age: role === 'Minor' ? age : undefined,
      biometricConfidence,
      originTrust,
    }),
    [role, age, biometricConfidence, originTrust]
  );

  const verdict = useMemo(
    () => evaluateSecurityPolicy(subject, targetScope, action, isDestructive),
    [subject, targetScope, action, isDestructive]
  );

  const handleRoleChange = (newRole: PolicySubject['role']) => {
    setRole(newRole);
    if (newRole === 'Minor') {
      setAge(14);
      setBiometricConfidence(0.95);
      setOriginTrust('TrustedHuman');
    } else if (newRole === 'Guest') {
      setAge(28);
      setBiometricConfidence(0.85);
      setOriginTrust('LocalDaemon');
    } else if (newRole === 'UntrustedAgent') {
      setBiometricConfidence(0.5);
      setOriginTrust('UntrustedExternal');
    } else {
      setAge(35);
      setBiometricConfidence(0.95);
      setOriginTrust('TrustedHuman');
    }
  };

  return (
    <div className="sim-shell policy-playground">
      <div className="sim-header">
        <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' }}>
          <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
            {t.policyPlayground.subtitle}
          </span>
          <strong>{t.policyPlayground.title}</strong>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span
            className="status-tag"
            style={{
              borderColor:
                verdict.outcome === 'ALLOW'
                  ? 'var(--hakille-lueur)'
                  : verdict.outcome === 'HARD_BLOCK'
                    ? 'var(--hakille-danger)'
                    : 'var(--hakille-texte-2)',
              color:
                verdict.outcome === 'ALLOW'
                  ? 'var(--hakille-texte-1)'
                  : verdict.outcome === 'HARD_BLOCK'
                    ? 'var(--hakille-danger)'
                    : 'var(--hakille-lueur)',
            }}
          >
            {verdict.outcome}
          </span>
        </div>
      </div>

      <div className="sim-body policy-grid">
        {/* Left Column: Subject Profile & Emission Request */}
        <div className="policy-column">
          <div className="lab-panel">
            <h3 className="panel-title">
              {t.policyPlayground.subjectProfiles.title}
            </h3>

            <div className="field-row">
              <div className="field-group" style={{ flex: 1 }}>
                <label className="field-label">{t.policyPlayground.subjectProfiles.roleLabel}</label>
                <select
                  className="sim-select"
                  value={role}
                  onChange={(e) => handleRoleChange(e.target.value as PolicySubject['role'])}
                >
                  <option value="Owner">{t.policyPlayground.subjectProfiles.roleOwner}</option>
                  <option value="Minor">{t.policyPlayground.subjectProfiles.roleMinor}</option>
                  <option value="Guest">{t.policyPlayground.subjectProfiles.roleGuest}</option>
                  <option value="UntrustedAgent">{t.policyPlayground.subjectProfiles.roleUntrustedAgent}</option>
                </select>
              </div>

              {role === 'Minor' && (
                <div className="field-group" style={{ width: '100px' }}>
                  <label className="field-label">{t.policyPlayground.subjectProfiles.ageLabel}</label>
                  <input
                    type="number"
                    className="sim-input"
                    min={1}
                    max={17}
                    value={age}
                    onChange={(e) => setAge(Number.parseInt(e.target.value, 10))}
                  />
                </div>
              )}
            </div>

            <div className="field-group" style={{ marginTop: '10px' }}>
              <label className="field-label">{t.policyPlayground.subjectProfiles.trustLabel}</label>
              <select
                className="sim-select"
                value={originTrust}
                onChange={(e) => setOriginTrust(e.target.value as PolicySubject['originTrust'])}
              >
                <option value="TrustedHuman">{t.policyPlayground.subjectProfiles.trustHuman}</option>
                <option value="LocalDaemon">{t.policyPlayground.subjectProfiles.trustDaemon}</option>
                <option value="UntrustedExternal">{t.policyPlayground.subjectProfiles.trustExternal}</option>
              </select>
            </div>

            {/* Continuous Biometric Slider */}
            <div className="slider-box" style={{ marginTop: '12px' }}>
              <div className="slider-header">
                <span>{t.policyPlayground.subjectProfiles.biometricLabel}</span>
                <span className="mono" style={{ color: 'var(--hakille-lueur)' }}>
                  {biometricConfidence.toFixed(2)}
                </span>
              </div>
              <input
                type="range"
                min="0.10"
                max="1.00"
                step="0.02"
                className="sim-slider"
                value={biometricConfidence}
                onChange={(e) => setBiometricConfidence(Number.parseFloat(e.target.value))}
              />
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.72rem', color: 'var(--hakille-texte-3)', marginTop: '2px' }}>
                <span>{t.policyPlayground.subjectProfiles.bioAbsent}</span>
                <span>{t.policyPlayground.subjectProfiles.bioPresence}</span>
                <span>{t.policyPlayground.subjectProfiles.bioFaceVoice}</span>
                <span>{t.policyPlayground.subjectProfiles.bioCertified}</span>
              </div>
            </div>
          </div>

          <div className="lab-panel" style={{ marginTop: '16px' }}>
            <h3 className="panel-title">
              {t.policyPlayground.emissionRequest.title}
            </h3>

            <div className="field-row">
              <div className="field-group" style={{ flex: 1 }}>
                <div style={{ display: 'flex', justifyContent: 'space-between' }}>
                  <label className="field-label">{t.policyPlayground.emissionRequest.targetScopeLabel}</label>
                  <span className="mono" style={{ fontSize: '0.7rem', color: 'var(--hakille-lueur)' }}>
                    {t.policyPlayground.emissionRequest.thresholdLabel}
                    {(isDestructive
                      ? Math.max(0.85, BIOMETRIC_STEP_UP_THRESHOLDS[targetScope])
                      : BIOMETRIC_STEP_UP_THRESHOLDS[targetScope]
                    ).toFixed(2)}
                  </span>
                </div>
                <select
                  className="sim-select"
                  value={targetScope}
                  onChange={(e) => setTargetScope(e.target.value as CapabilityScope)}
                >
                  <option value="Notification">Notification (Seuil: 0.50)</option>
                  <option value="Wallet">Wallet (Seuil: 0.90 · NoWallet mineurs)</option>
                  <option value="Filesystem">Filesystem (Seuil: 0.50)</option>
                  <option value="ModelInference">ModelInference (Seuil: 0.70)</option>
                  <option value="Biometrics">Biometrics (Seuil: 0.70)</option>
                  <option value="Camera">Camera (Seuil: 0.40)</option>
                  <option value="Audio">Audio (Seuil: 0.40)</option>
                  <option value="FullSystem">FullSystem (Seuil: 0.90 · Admin)</option>
                </select>
              </div>

              <div className="field-group" style={{ flex: 1 }}>
                <label className="field-label">{t.policyPlayground.emissionRequest.actionLabel}</label>
                <select
                  className="sim-select"
                  value={action}
                  onChange={(e) => setAction(e.target.value as CapabilityAction)}
                >
                  <option value="Read">{t.policyPlayground.emissionRequest.actionRead}</option>
                  <option value="Write">{t.policyPlayground.emissionRequest.actionWrite}</option>
                  <option value="Execute">{t.policyPlayground.emissionRequest.actionExecute}</option>
                  <option value="Delete">{t.policyPlayground.emissionRequest.actionDelete}</option>
                  <option value="Admin">{t.policyPlayground.emissionRequest.actionAdmin}</option>
                </select>
              </div>
            </div>

            <label className="checkbox-row" style={{ marginTop: '10px' }}>
              <input
                type="checkbox"
                checked={isDestructive}
                onChange={(e) => setIsDestructive(e.target.checked)}
              />
              <span>
                <strong>{t.policyPlayground.emissionRequest.destructiveLabel}</strong>{' '}
                <span style={{ color: 'var(--hakille-texte-3)', fontSize: '0.76rem' }}>
                  ({t.policyPlayground.emissionRequest.destructiveHint})
                </span>
              </span>
            </label>
          </div>
        </div>

        {/* Right Column: Live Verdict & Constitutional Citations */}
        <div className="policy-column">
          <div className="lab-panel verdict-display-panel">
            <h3 className="panel-title">
              {t.policyPlayground.verdictBanner.title}
            </h3>

            {/* Verdict Box */}
            <div
              className={`verdict-banner ${
                verdict.outcome === 'ALLOW'
                  ? 'verdict-pass'
                  : verdict.outcome === 'HARD_BLOCK'
                    ? 'verdict-block'
                    : 'verdict-stepup'
              }`}
              style={{ marginTop: '10px' }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <strong style={{ fontSize: '1.05rem' }}>
                  {verdict.outcome === 'ALLOW' && t.policyPlayground.verdictBanner.outcomeAllow}
                  {verdict.outcome === 'HARD_BLOCK' && t.policyPlayground.verdictBanner.outcomeHardBlock}
                  {verdict.outcome === 'STEP_UP_REQUIRED' && t.policyPlayground.verdictBanner.outcomeStepUp}
                </strong>
                <span className="mono" style={{ fontSize: '0.74rem' }}>
                  {verdict.applicableLawOrRule}
                </span>
              </div>

              {verdict.ruleViolated && (
                <div style={{ marginTop: '8px', color: 'var(--hakille-danger)', fontWeight: 600, fontSize: '0.85rem' }}>
                  {verdict.ruleViolated}
                </div>
              )}

              <p style={{ margin: '8px 0 0 0', fontSize: '0.82rem', lineHeight: 1.45 }}>
                {verdict.rationale}
              </p>
            </div>

            {/* Constitutional Rules Breakdown */}
            <div className="rules-legend-grid" style={{ marginTop: '16px' }}>
              <div className="rule-badge-card">
                <strong>{t.policyPlayground.rulesCited.rule01Title}</strong>
                <p>{t.policyPlayground.rulesCited.rule01Desc}</p>
              </div>

              <div className="rule-badge-card">
                <strong>{t.policyPlayground.rulesCited.rule07Title}</strong>
                <p>{t.policyPlayground.rulesCited.rule07Desc}</p>
              </div>

              <div className="rule-badge-card">
                <strong>{t.policyPlayground.rulesCited.adr013Title}</strong>
                <p>{t.policyPlayground.rulesCited.adr013Desc}</p>
              </div>

              <div className="rule-badge-card">
                <strong>{t.policyPlayground.rulesCited.isoStepUpTitle}</strong>
                <p>{t.policyPlayground.rulesCited.isoStepUpDesc}</p>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
