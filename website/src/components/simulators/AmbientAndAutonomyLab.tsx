import { useMemo, useState } from 'react';
import {
  AccessibilityProfile,
  DAILY_SILENCE_BUDGET_MAX,
  DomainAutonomyRecord,
  SilenceBudgetState,
  acceptCrankPromotion,
  consumeProactiveCredit,
  dismissProactiveSuggestion,
  recordHumanUndo,
  recordValidatedAction,
  selectMultimodalChannel,
} from '@/engine/cognitiveSimulators';
import { useI18n } from '@/i18n/context';

export function AmbientAndAutonomyLab() {
  const { t } = useI18n();

  // Channel Selector State
  const [ambientDb, setAmbientDb] = useState(42);
  const [inMeeting, setInMeeting] = useState(false);
  const [thirdPartyPresent, setThirdPartyPresent] = useState(false);
  const [headphonesConnected, setHeadphonesConnected] = useState(false);
  const [a11yProfile, setA11yProfile] = useState<AccessibilityProfile>('Standard');

  // Silence Budget State
  const [silenceState, setSilenceState] = useState<SilenceBudgetState>({
    remainingCredits: DAILY_SILENCE_BUDGET_MAX,
    snoozedTopics: {},
  });

  // Autonomy Crank State
  const [autonomy, setAutonomy] = useState<DomainAutonomyRecord>({
    domainId: 'mail',
    crank: 1,
    streakWithoutUndo: 15,
    promotionEligible: false,
  });

  const decision = useMemo(
    () =>
      selectMultimodalChannel({
        ambientDb,
        inMeeting,
        thirdPartyPresent,
        headphonesConnected,
        accessibilityProfile: a11yProfile,
      }),
    [a11yProfile, ambientDb, headphonesConnected, inMeeting, thirdPartyPresent]
  );

  const snoozedDays = silenceState.snoozedTopics['digest'] ?? 0;

  return (
    <div className="comparison-grid" style={{ marginBottom: 0 }}>
      {/* LEFT: Multimodal Channel Selector & Daily Silence Budget */}
      <div className="sim-frame">
        <div className="sim-topbar">
          <strong style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
            {t.ambientLab.channelHeading}
          </strong>
          <span className="status-tag">{decision.channel}</span>
        </div>

        <div className="sim-body" style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div>
            <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '6px' }}>
              <label htmlFor="ambient-db-slider" style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
                {t.ambientLab.noiseLabel}
              </label>
              <span className="mono tabular">{ambientDb} dB</span>
            </div>
            <input
              id="ambient-db-slider"
              type="range"
              min={20}
              max={85}
              value={ambientDb}
              onChange={(e) => setAmbientDb(Number(e.target.value))}
              className="range-input"
            />
          </div>

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '8px' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: 'var(--hakille-type-caption-size)' }}>
              <input type="checkbox" checked={inMeeting} onChange={(e) => setInMeeting(e.target.checked)} />
              <span>{t.ambientLab.meetingToggle}</span>
            </label>
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: 'var(--hakille-type-caption-size)' }}>
              <input
                type="checkbox"
                checked={thirdPartyPresent}
                onChange={(e) => setThirdPartyPresent(e.target.checked)}
              />
              <span>{t.ambientLab.thirdPartyToggle}</span>
            </label>
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px', fontSize: 'var(--hakille-type-caption-size)' }}>
              <input
                type="checkbox"
                checked={headphonesConnected}
                onChange={(e) => setHeadphonesConnected(e.target.checked)}
              />
              <span>{t.ambientLab.headphonesToggle}</span>
            </label>
            <select
              className="lang-select"
              aria-label="Accessibility profile"
              value={a11yProfile}
              onChange={(e) => setA11yProfile(e.target.value as AccessibilityProfile)}
            >
              <option value="Standard">Profile: Standard</option>
              <option value="VisualOnly">Profile: VisualOnly</option>
              <option value="HapticAssisted">Profile: HapticAssisted</option>
              <option value="AudioFirst">Profile: AudioFirst</option>
            </select>
          </div>

          <div
            className="mono"
            style={{
              padding: '12px',
              borderRadius: '8px',
              background: 'var(--hakille-surface-0)',
              border: '1px solid var(--hakille-ligne)',
              fontSize: '0.78rem',
            }}
          >
            <div>channel    : {decision.channel}</div>
            <div>voice_ok   : {String(decision.voicePermitted)} (gain {Math.round(decision.recommendedVolume * 100)}%)</div>
            <div>rationale  : {decision.rationaleCode}</div>
          </div>

          <div style={{ borderTop: '1px solid var(--hakille-ligne)', paddingTop: '16px', display: 'flex', flexDirection: 'column', gap: '10px' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <strong style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
                {t.ambientLab.silenceBudgetHeading}
              </strong>
              <span className="status-tag">
                {silenceState.remainingCredits} / {DAILY_SILENCE_BUDGET_MAX} credits
                {snoozedDays > 0 ? ` · Snoozed ${snoozedDays}d` : ''}
              </span>
            </div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
              <button
                type="button"
                className="ghost-btn"
                disabled={silenceState.remainingCredits <= 0 || snoozedDays > 0}
                onClick={() => {
                  const res = consumeProactiveCredit(silenceState, 'digest');
                  setSilenceState(res.next);
                }}
              >
                {t.ambientLab.triggerNudgeBtn}
              </button>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => setSilenceState((s) => dismissProactiveSuggestion(s, 'digest'))}
              >
                {t.ambientLab.dismissTopicBtn}
              </button>
              <button
                type="button"
                className="ghost-btn"
                onClick={() =>
                  setSilenceState({ remainingCredits: DAILY_SILENCE_BUDGET_MAX, snoozedTopics: {} })
                }
              >
                {t.ambientLab.resetDayBtn}
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* RIGHT: Per-Domain Autonomy Crank Ladder (0 -> 4) */}
      <div className="sim-frame">
        <div className="sim-topbar">
          <strong style={{ fontSize: 'var(--hakille-type-caption-size)' }}>
            {t.ambientLab.autonomyHeading}
          </strong>
          <span className="status-tag">agent-mail · Crank {autonomy.crank}</span>
        </div>

        <div className="sim-body" style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div className="crank-track">
            {([0, 1, 2, 3, 4] as const).map((step) => (
              <div key={step} className="crank-step" data-active={autonomy.crank === step ? 'true' : 'false'}>
                <div>Crank {step}</div>
              </div>
            ))}
          </div>

          <div
            className="mono"
            style={{
              padding: '12px',
              borderRadius: '8px',
              background: 'var(--hakille-surface-0)',
              border: '1px solid var(--hakille-ligne)',
              fontSize: '0.78rem',
            }}
          >
            <div>current_crank       : {autonomy.crank} / 4</div>
            <div>validated_streak    : {autonomy.streakWithoutUndo} / 20 required for promotion</div>
            <div>promotion_available : {String(autonomy.promotionEligible)}</div>
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
            <button
              type="button"
              className="ghost-btn"
              onClick={() =>
                setAutonomy((prev) => {
                  let cur = prev;
                  for (let i = 0; i < 5; i++) cur = recordValidatedAction(cur);
                  return cur;
                })
              }
            >
              {t.ambientLab.validate20Btn}
            </button>

            {autonomy.promotionEligible && (
              <button
                type="button"
                className="primary-lueur-btn"
                onClick={() => setAutonomy((prev) => acceptCrankPromotion(prev))}
              >
                {t.ambientLab.promoteCrankBtn}
              </button>
            )}

            <button
              type="button"
              className="ghost-btn"
              onClick={() => setAutonomy((prev) => recordHumanUndo(prev))}
            >
              {t.ambientLab.humanUndoBtn}
            </button>
          </div>

          <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
            Crank 0 = Observe only · Crank 1 = Suggest · Crank 2 = Act on reversible · Crank 3 = Prepare irreversible (600 ms hold) · Crank 4 = Scoped standing mandate.
          </p>
        </div>
      </div>
    </div>
  );
}
