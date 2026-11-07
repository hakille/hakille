import { useState } from 'react';
import {
  CognitiveState,
  NotificationRequestInput,
  SupportedSubstrate,
  evaluateNotificationRouting,
  formatSubstrateIpcPreview,
} from '@/engine/notificationRouter';
import { hakilleAudio } from '@/engine/audioPlayer';
import { useI18n } from '@/i18n/context';

export function CognitiveNotificationRouterLab() {
  const { t } = useI18n();

  // Notification Request State
  const [title, setTitle] = useState(t.smnfLab.requestSection.sampleTitle);
  const [body, setBody] = useState(t.smnfLab.requestSection.sampleBody);
  const [urgency, setUrgency] = useState<'Low' | 'Normal' | 'High' | 'Critical'>('Normal');
  const [category, setCategory] = useState<'System' | 'Cognitive' | 'Agent' | 'Security' | 'Wallet'>('Agent');
  const [hasDestructiveAction, setHasDestructiveAction] = useState(false);

  // Cognitive Environment State
  const [userLookingAtScreen, setUserLookingAtScreen] = useState(true);
  const [perclosDrowsinessScore, setPerclosDrowsinessScore] = useState(0.04);
  const [ambientNoiseDb, setAmbientNoiseDb] = useState(42);
  const [inMeeting, setInMeeting] = useState(false);
  const [thirdPartiesPresent, setThirdPartiesPresent] = useState(false);
  const [userWearingHeadphones, setUserWearingHeadphones] = useState(false);
  const [isNightTime, setIsNightTime] = useState(false);

  // Daily Silence Budget (3 suggestions / day)
  const [silenceCredits, setSilenceCredits] = useState(3);
  const [activeSubstrate, setActiveSubstrate] = useState<SupportedSubstrate>('linux');

  const cognitiveState: CognitiveState = {
    userLookingAtScreen,
    perclosDrowsinessScore,
    ambientNoiseDb,
    inMeeting,
    thirdPartiesPresent,
    userWearingHeadphones,
    isNightTime,
  };

  const requestInput: NotificationRequestInput = {
    title,
    body,
    urgency,
    category,
    hasDestructiveAction,
  };

  const decision = evaluateNotificationRouting(requestInput, cognitiveState, silenceCredits);
  const substrateCode = formatSubstrateIpcPreview(activeSubstrate, requestInput, decision);

  const handleAudition = () => {
    if (decision.visualStyle === 'BraiseActionRequired' || urgency === 'Critical') {
      hakilleAudio.playStateSignal('attention', true, 0.7);
    } else if (decision.channel === 'SpatialChime' || decision.channel === 'SubstrateNativeDispatch') {
      hakilleAudio.playStateSignal('eveil', true, 0.5);
    } else {
      hakilleAudio.playStateSignal('fait', true, 0.4);
    }
  };

  return (
    <div className="sim-shell router-lab">
      <div className="sim-header">
        <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' }}>
          <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
            {t.smnfLab.subtitle}
          </span>
          <strong>{t.smnfLab.title}</strong>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span className="status-tag">
            {decision.deferred
              ? t.smnfLab.visualStyles.deferredBadge
              : t.smnfLab.visualStyles.immediateBadge}
          </span>
          <span
            className="status-tag"
            {...(decision.visualStyle === 'BraiseActionRequired'
              ? { 'data-braise-context': 'decision-ring' as const }
              : {})}
          >
            {decision.visualStyle}
          </span>
        </div>
      </div>

      <div className="sim-body router-grid">
        {/* Left Column: Request Builder + Cognitive Sensors */}
        <div className="router-column">
          <div className="lab-panel">
            <h3 className="panel-title">{t.smnfLab.requestSection.title}</h3>

            <div className="field-group">
              <label className="field-label">{t.smnfLab.requestSection.fieldTitle}</label>
              <input
                type="text"
                className="sim-input"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            </div>

            <div className="field-group">
              <label className="field-label">{t.smnfLab.requestSection.fieldBody}</label>
              <input
                type="text"
                className="sim-input"
                value={body}
                onChange={(e) => setBody(e.target.value)}
              />
            </div>

            <div className="field-row">
              <div className="field-group" style={{ flex: 1 }}>
                <label className="field-label">{t.smnfLab.requestSection.fieldUrgency}</label>
                <select
                  className="sim-select"
                  value={urgency}
                  onChange={(e) => setUrgency(e.target.value as typeof urgency)}
                >
                  <option value="Low">Low ({t.smnfLab.requestSection.urgencyLow})</option>
                  <option value="Normal">Normal ({t.smnfLab.requestSection.urgencyNormal})</option>
                  <option value="High">High ({t.smnfLab.requestSection.urgencyHigh})</option>
                  <option value="Critical">Critical ({t.smnfLab.requestSection.urgencyCritical})</option>
                </select>
              </div>

              <div className="field-group" style={{ flex: 1 }}>
                <label className="field-label">{t.smnfLab.requestSection.fieldCategory}</label>
                <select
                  className="sim-select"
                  value={category}
                  onChange={(e) => setCategory(e.target.value as typeof category)}
                >
                  <option value="Agent">Agent ({t.smnfLab.requestSection.categoryAgent})</option>
                  <option value="Cognitive">Cognitive ({t.smnfLab.requestSection.categoryCognitive})</option>
                  <option value="System">System ({t.smnfLab.requestSection.categorySystem})</option>
                  <option value="Security">Security ({t.smnfLab.requestSection.categorySecurity})</option>
                  <option value="Wallet">Wallet ({t.smnfLab.requestSection.categoryWallet})</option>
                </select>
              </div>
            </div>

            <label className="checkbox-row" style={{ marginTop: '8px' }}>
              <input
                type="checkbox"
                checked={hasDestructiveAction}
                onChange={(e) => setHasDestructiveAction(e.target.checked)}
              />
              <span>
                <strong>{t.smnfLab.requestSection.destructiveLabel}</strong>{' '}
                <span style={{ color: 'var(--hakille-texte-3)', fontSize: '0.78rem' }}>
                  ({t.smnfLab.requestSection.destructiveHint})
                </span>
              </span>
            </label>
          </div>

          <div className="lab-panel" style={{ marginTop: '16px' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <h3 className="panel-title">{t.smnfLab.cognitiveSensors.title}</h3>
              <span className="mono" style={{ fontSize: '0.74rem', color: 'var(--hakille-lueur)' }}>
                {t.smnfLab.silenceBudget.label}
                <strong>{silenceCredits} / 3</strong>
              </span>
            </div>

            {/* Gaze tracking */}
            <label className="checkbox-row">
              <input
                type="checkbox"
                checked={userLookingAtScreen}
                onChange={(e) => setUserLookingAtScreen(e.target.checked)}
              />
              <span>{t.smnfLab.cognitiveSensors.gazeScreen}</span>
            </label>

            {/* PERCLOS Drowsiness */}
            <div className="slider-box" style={{ marginTop: '10px' }}>
              <div className="slider-header">
                <span>{t.smnfLab.cognitiveSensors.perclosLabel}</span>
                <span className="mono" style={{ color: perclosDrowsinessScore >= 0.15 ? 'var(--hakille-lueur)' : 'inherit' }}>
                  {(perclosDrowsinessScore * 100).toFixed(0)} %{' '}
                  {perclosDrowsinessScore >= 0.15
                    ? t.smnfLab.cognitiveSensors.perclosAlert
                    : perclosDrowsinessScore >= 0.08
                      ? t.smnfLab.cognitiveSensors.perclosModerate
                      : t.smnfLab.cognitiveSensors.perclosNominal}
                </span>
              </div>
              <input
                type="range"
                min="0.00"
                max="0.30"
                step="0.01"
                className="sim-slider"
                value={perclosDrowsinessScore}
                onChange={(e) => setPerclosDrowsinessScore(Number.parseFloat(e.target.value))}
              />
            </div>

            {/* Ambient Noise dB */}
            <div className="slider-box" style={{ marginTop: '10px' }}>
              <div className="slider-header">
                <span>{t.smnfLab.cognitiveSensors.noiseLabel}</span>
                <span className="mono">
                  {ambientNoiseDb} dB{' '}
                  {ambientNoiseDb > 65
                    ? t.smnfLab.cognitiveSensors.noiseHigh
                    : ambientNoiseDb < 32
                      ? t.smnfLab.cognitiveSensors.noiseQuiet
                      : t.smnfLab.cognitiveSensors.noiseOffice}
                </span>
              </div>
              <input
                type="range"
                min="20"
                max="85"
                step="1"
                className="sim-slider"
                value={ambientNoiseDb}
                onChange={(e) => setAmbientNoiseDb(Number.parseInt(e.target.value, 10))}
              />
            </div>

            {/* Context toggles */}
            <div className="toggle-grid" style={{ marginTop: '12px' }}>
              <label className="checkbox-row">
                <input
                  type="checkbox"
                  checked={inMeeting}
                  onChange={(e) => setInMeeting(e.target.checked)}
                />
                <span>{t.smnfLab.cognitiveSensors.inMeeting}</span>
              </label>

              <label className="checkbox-row">
                <input
                  type="checkbox"
                  checked={thirdPartiesPresent}
                  onChange={(e) => setThirdPartiesPresent(e.target.checked)}
                />
                <span>{t.smnfLab.cognitiveSensors.thirdParties}</span>
              </label>

              <label className="checkbox-row">
                <input
                  type="checkbox"
                  checked={userWearingHeadphones}
                  onChange={(e) => setUserWearingHeadphones(e.target.checked)}
                />
                <span>{t.smnfLab.cognitiveSensors.headphones}</span>
              </label>

              <label className="checkbox-row">
                <input
                  type="checkbox"
                  checked={isNightTime}
                  onChange={(e) => setIsNightTime(e.target.checked)}
                />
                <span>{t.smnfLab.cognitiveSensors.nightMode}</span>
              </label>
            </div>

            {/* Silence Budget Reset Simulation */}
            <div style={{ marginTop: '14px', display: 'flex', gap: '8px' }}>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => setSilenceCredits((c) => Math.max(0, c - 1))}
                disabled={silenceCredits <= 0}
              >
                {t.smnfLab.silenceBudget.spendCredit}
              </button>
              <button
                type="button"
                className="ghost-btn"
                onClick={() => setSilenceCredits(3)}
              >
                {t.smnfLab.silenceBudget.resetDaily}
              </button>
            </div>
          </div>
        </div>

        {/* Right Column: Live Decision + Audition Cue + IPC Preview */}
        <div className="router-column">
          <div className="lab-panel decision-card">
            <h3 className="panel-title" style={{ display: 'flex', justifyContent: 'space-between' }}>
              <span>{t.smnfLab.decisionCard.title}</span>
              <button
                type="button"
                className="ghost-btn"
                style={{ padding: '2px 8px', fontSize: '0.74rem' }}
                onClick={handleAudition}
              >
                🔊 {t.smnfLab.auditionBtn.label}
              </button>
            </h3>

            <div className="metric-row">
              <span className="metric-label">{t.smnfLab.channels.label}</span>
              <strong className="mono" style={{ color: 'var(--hakille-lueur)' }}>
                {decision.channel}
              </strong>
            </div>

            <div className="metric-row">
              <span className="metric-label">{t.smnfLab.visualStyles.label}</span>
              <span
                className="mono"
                {...(decision.visualStyle === 'BraiseActionRequired'
                  ? { 'data-braise-context': 'decision-ring' as const }
                  : {})}
              >
                {decision.visualStyle}
              </span>
            </div>

            <div className="metric-row">
              <span className="metric-label">{t.smnfLab.decisionCard.hapticLabel}</span>
              <span className="mono">{decision.hapticFeedback ?? t.smnfLab.decisionCard.hapticNone}</span>
            </div>

            <div className="metric-row">
              <span className="metric-label">{t.smnfLab.decisionCard.costLabel}</span>
              <span className="mono">{decision.creditCost} {t.smnfLab.silenceBudget.remainingUnit}</span>
            </div>

            <div className="rationale-box">
              <strong style={{ fontSize: '0.8rem', color: 'var(--hakille-texte-1)' }}>
                {t.smnfLab.decisionCard.rationaleLabel}
              </strong>
              <p style={{ margin: '4px 0 0 0', fontSize: '0.82rem', color: 'var(--hakille-texte-2)' }}>
                {decision.rationale}
              </p>
            </div>
          </div>

          {/* Substrate IPC Preview */}
          <div className="lab-panel" style={{ marginTop: '16px' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: '8px' }}>
              <h3 className="panel-title" style={{ margin: 0 }}>
                {t.smnfLab.ipcPreview.title}
              </h3>
              <div className="tab-pills">
                {(['linux', 'macos', 'windows', 'mobile', 'redox', 'web'] as const).map((sub) => (
                  <button
                    key={sub}
                    type="button"
                    className={`tab-pill ${activeSubstrate === sub ? 'active' : ''}`}
                    onClick={() => setActiveSubstrate(sub)}
                  >
                    {t.smnfLab.substrates[sub]}
                  </button>
                ))}
              </div>
            </div>

            <pre className="ipc-code-block">{substrateCode}</pre>
          </div>
        </div>
      </div>
    </div>
  );
}
