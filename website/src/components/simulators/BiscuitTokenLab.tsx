import { useMemo, useState } from 'react';
import {
  ALL_13_CANONICAL_SCOPES,
  ALL_5_ACTIONS,
  AttenuationResult,
  BiscuitBlock,
  CapabilityAction,
  CapabilityScope,
  MAX_DELEGATION_DEPTH,
  PINNED_ROOT_PUBLIC_KEY,
  attemptPrivilegeEscalationAttack,
  attenuateToken,
  buildRootToken,
} from '@/engine/biscuitEngine';
import { useI18n } from '@/i18n/context';

export function BiscuitTokenLab() {
  const { t } = useI18n();

  // Root configuration
  const [rootScopes, setRootScopes] = useState<CapabilityScope[]>([
    'Notification',
    'Filesystem',
    'ModelInference',
    'Audio',
  ]);
  const [rootActions, setRootActions] = useState<CapabilityAction[]>(['Read', 'Write']);
  const [ttlSeconds, setTtlSeconds] = useState(300);

  // Sub-agent attenuation configuration
  const [subAgentScopes, setSubAgentScopes] = useState<CapabilityScope[]>([
    'Notification',
    'Filesystem',
  ]);
  const [subAgentActions, setSubAgentActions] = useState<CapabilityAction[]>(['Read']);

  // Active attack simulation
  const [activeAttack, setActiveAttack] = useState<
    'none' | 'widen_scope' | 'add_action' | 'remove_caveat' | 'forge_root'
  >('none');

  // Build the base token chain
  const baseChain = useMemo(() => {
    const root = buildRootToken(rootScopes, rootActions, ttlSeconds);
    const subResult = attenuateToken(root, subAgentScopes, subAgentActions);
    if (subResult.valid) {
      // Create Block 1 from attenuation result
      const block1: BiscuitBlock = {
        index: 1,
        label: `Block 1: ${t.biscuitLab.authorityLabels.subAgentAttenuation}`,
        authority: 'SubAgentAttenuation',
        facts: [
          ...subAgentScopes.map((s) => `allowed_scope("${s.toLowerCase()}");`),
          ...subAgentActions.map((a) => `allowed_action("${a.toLowerCase()}");`),
        ],
        checks: [
          'check if operation($scope, $action), allowed_scope($scope), allowed_action($action);',
        ],
        signatureHex: 'sig_subagent_7a4f91e25519',
      };
      return [...root, block1];
    }
    return root;
  }, [rootScopes, rootActions, ttlSeconds, subAgentScopes, subAgentActions, t]);

  // Compute evaluation result based on current state or active attack
  const evaluationResult: AttenuationResult = useMemo(() => {
    if (activeAttack !== 'none') {
      return attemptPrivilegeEscalationAttack(baseChain, activeAttack);
    }
    return attenuateToken(baseChain.slice(0, 1), subAgentScopes, subAgentActions);
  }, [baseChain, activeAttack, subAgentScopes, subAgentActions]);

  const toggleRootScope = (scope: CapabilityScope) => {
    setRootScopes((prev) =>
      prev.includes(scope) ? prev.filter((s) => s !== scope) : [...prev, scope]
    );
  };

  const toggleSubScope = (scope: CapabilityScope) => {
    setSubAgentScopes((prev) =>
      prev.includes(scope) ? prev.filter((s) => s !== scope) : [...prev, scope]
    );
  };

  return (
    <div className="sim-shell biscuit-lab">
      <div className="sim-header">
        <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' }}>
          <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
            {t.biscuitLab.subtitle}
          </span>
          <strong>{t.biscuitLab.title}</strong>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span className="status-tag">
            {t.biscuitLab.blockChain.depthLabel}
            {baseChain.length - 1} / {MAX_DELEGATION_DEPTH}
          </span>
          <span
            className="status-tag"
            style={{
              borderColor: evaluationResult.valid ? 'var(--hakille-lueur)' : 'var(--hakille-danger)',
              color: evaluationResult.valid ? 'var(--hakille-texte-1)' : 'var(--hakille-danger)',
            }}
          >
            {evaluationResult.valid
              ? t.biscuitLab.verifyConsole.monotonicPassBadge
              : t.biscuitLab.verifyConsole.escalationBlockedBadge}
          </span>
        </div>
      </div>

      <div className="sim-body biscuit-grid">
        {/* Left Column: Visual Chain & Controls */}
        <div className="biscuit-controls-column">
          {/* Visual Block Chain */}
          <div className="lab-panel">
            <h3 className="panel-title">
              {t.biscuitLab.blockChain.title}
            </h3>
            <div className="blocks-chain-flow">
              <div className="token-block-card root-card">
                <div className="block-badge">{t.biscuitLab.blockChain.rootBadge}</div>
                <div className="block-detail">
                  <span className="mono" style={{ fontSize: '0.74rem', color: 'var(--hakille-lueur)' }}>
                    {t.biscuitLab.blockChain.rootKeyLabel} {PINNED_ROOT_PUBLIC_KEY.slice(0, 20)}...
                  </span>
                  <div className="tag-cloud" style={{ marginTop: '6px' }}>
                    {rootScopes.map((s) => (
                      <span key={s} className="scope-chip">
                        {s}
                      </span>
                    ))}
                  </div>
                </div>
              </div>

              <div className="chain-connector">↓ {t.biscuitLab.blockChain.attenuationConnector}</div>

              <div className="token-block-card child-card">
                <div className="block-badge">{t.biscuitLab.blockChain.childBadge}</div>
                <div className="block-detail">
                  <span className="mono" style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>
                    {t.biscuitLab.blockChain.childDetail}
                  </span>
                  <div className="tag-cloud" style={{ marginTop: '6px' }}>
                    {subAgentScopes.map((s) => (
                      <span key={s} className="scope-chip child-chip">
                        {s}
                      </span>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          </div>

          {/* Scope and Action Matrix */}
          <div className="lab-panel" style={{ marginTop: '16px' }}>
            <h3 className="panel-title">
              {t.biscuitLab.scopes.title}
            </h3>

            <div className="scopes-matrix-grid">
              {ALL_13_CANONICAL_SCOPES.map((scope) => {
                const inRoot = rootScopes.includes(scope);
                const inSub = subAgentScopes.includes(scope);
                return (
                  <div key={scope} className="scope-row-item">
                    <span className="scope-name">{scope}</span>
                    <div style={{ display: 'flex', gap: '6px' }}>
                      <button
                        type="button"
                        className={`mini-pill ${inRoot ? 'active-root' : ''}`}
                        onClick={() => toggleRootScope(scope)}
                        title="Block 0 Root"
                      >
                        {t.biscuitLab.scopes.rootPill}
                      </button>
                      <button
                        type="button"
                        className={`mini-pill ${inSub ? 'active-sub' : ''}`}
                        onClick={() => toggleSubScope(scope)}
                        title="Block 1 Sub-Agent"
                      >
                        {t.biscuitLab.scopes.subPill}
                      </button>
                    </div>
                  </div>
                );
              })}
            </div>

            {/* Actions & TTL */}
            <div style={{ marginTop: '14px', borderTop: '1px solid var(--hakille-ligne)', paddingTop: '10px' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <span className="field-label">{t.biscuitLab.actions.rootLabel}</span>
                <div style={{ display: 'flex', gap: '6px' }}>
                  {ALL_5_ACTIONS.map((action) => (
                    <button
                      key={action}
                      type="button"
                      className={`mini-pill ${rootActions.includes(action) ? 'active-root' : ''}`}
                      onClick={() =>
                        setRootActions((prev) =>
                          prev.includes(action) ? prev.filter((a) => a !== action) : [...prev, action]
                        )
                      }
                    >
                      {action}
                    </button>
                  ))}
                </div>
              </div>

              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginTop: '8px' }}>
                <span className="field-label">{t.biscuitLab.actions.subLabel}</span>
                <div style={{ display: 'flex', gap: '6px' }}>
                  {ALL_5_ACTIONS.map((action) => (
                    <button
                      key={action}
                      type="button"
                      className={`mini-pill ${subAgentActions.includes(action) ? 'active-sub' : ''}`}
                      onClick={() =>
                        setSubAgentActions((prev) =>
                          prev.includes(action) ? prev.filter((a) => a !== action) : [...prev, action]
                        )
                      }
                    >
                      {action}
                    </button>
                  ))}
                </div>
              </div>

              <div className="slider-box" style={{ marginTop: '12px' }}>
                <div className="slider-header">
                  <span>{t.biscuitLab.leaseSlider.label}</span>
                  <span className="mono">{ttlSeconds} {t.biscuitLab.leaseSlider.unit}</span>
                </div>
                <input
                  type="range"
                  min="30"
                  max="3600"
                  step="30"
                  className="sim-slider"
                  value={ttlSeconds}
                  onChange={(e) => setTtlSeconds(Number.parseInt(e.target.value, 10))}
                />
              </div>
            </div>
          </div>

          {/* Privilege Escalation Attack Sandbox */}
          <div className="lab-panel" style={{ marginTop: '16px' }}>
            <h3 className="panel-title">
              {t.biscuitLab.attacks.title}
            </h3>
            <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-2)', margin: '4px 0 12px 0' }}>
              {t.biscuitLab.attacks.description}
            </p>

            <div className="attack-btn-grid">
              <button
                type="button"
                className={`ghost-btn attack-btn ${activeAttack === 'widen_scope' ? 'active-attack' : ''}`}
                onClick={() => setActiveAttack('widen_scope')}
              >
                ⚡ {t.biscuitLab.attacks.widenScopeBtn}
              </button>
              <button
                type="button"
                className={`ghost-btn attack-btn ${activeAttack === 'add_action' ? 'active-attack' : ''}`}
                onClick={() => setActiveAttack('add_action')}
              >
                ⚡ {t.biscuitLab.attacks.addActionBtn}
              </button>
              <button
                type="button"
                className={`ghost-btn attack-btn ${activeAttack === 'remove_caveat' ? 'active-attack' : ''}`}
                onClick={() => setActiveAttack('remove_caveat')}
              >
                ⚡ {t.biscuitLab.attacks.removeCaveatBtn}
              </button>
              <button
                type="button"
                className={`ghost-btn attack-btn ${activeAttack === 'forge_root' ? 'active-attack' : ''}`}
                onClick={() => setActiveAttack('forge_root')}
              >
                ⚡ {t.biscuitLab.attacks.forgeRootBtn}
              </button>
            </div>

            {activeAttack !== 'none' && (
              <div style={{ marginTop: '12px' }}>
                <button
                  type="button"
                  className="ghost-btn"
                  style={{ width: '100%', borderColor: 'var(--hakille-lueur)' }}
                  onClick={() => setActiveAttack('none')}
                >
                  ↺ {t.biscuitLab.attacks.resetBtn}
                </button>
              </div>
            )}
          </div>
        </div>

        {/* Right Column: Live Datalog Console & Authorizer Verdict */}
        <div className="biscuit-console-column">
          <div className="lab-panel datalog-console-card">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <h3 className="panel-title" style={{ margin: 0 }}>
                {t.biscuitLab.verifyConsole.title}
              </h3>
              <span className="mono" style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>
                {t.biscuitLab.verifyConsole.rfcBadge}
              </span>
            </div>

            {/* Verdict Banner */}
            <div
              className={`verdict-banner ${evaluationResult.valid ? 'verdict-pass' : 'verdict-block'}`}
              style={{ marginTop: '12px' }}
            >
              <strong>
                {evaluationResult.valid
                  ? t.biscuitLab.verifyConsole.validBanner
                  : t.biscuitLab.verifyConsole.rejectedBanner}
              </strong>
              {evaluationResult.violationReason && (
                <p style={{ margin: '6px 0 0 0', fontSize: '0.8rem' }}>
                  {evaluationResult.violationReason}
                </p>
              )}
            </div>

            {/* Live Datalog Syntax Preview */}
            <div style={{ marginTop: '14px' }}>
              <div className="eyebrow-mono" style={{ marginBottom: '6px' }}>
                {t.biscuitLab.verifyConsole.datalogSourceEyebrow}
              </div>
              <pre className="datalog-syntax-block">{evaluationResult.datalogCode}</pre>
            </div>

            {/* Invariant Explanation */}
            <div className="datalog-axiom-card" style={{ marginTop: '14px' }}>
              <strong style={{ fontSize: '0.82rem', color: 'var(--hakille-texte-1)' }}>
                {t.biscuitLab.verifyConsole.theoremTitle}
              </strong>
              <p style={{ margin: '4px 0 0 0', fontSize: '0.78rem', color: 'var(--hakille-texte-2)' }}>
                {t.biscuitLab.verifyConsole.theoremBody}
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
