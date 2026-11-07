import { useCallback, useEffect, useRef, useState } from 'react';
import { BraiseHoldButton } from '@/components/primitives/BraiseHoldButton';
import { hakilleAudio } from '@/engine/audioPlayer';
import { useI18n } from '@/i18n/context';

export function DesktopHudToastSandbox() {
  const { t } = useI18n();

  // Toast Configurator State
  const [title, setTitle] = useState(t.hudToastSandbox.configurator.sampleTitle);
  const [body, setBody] = useState(t.hudToastSandbox.configurator.sampleBody);
  const [urgency, setUrgency] = useState<'Low' | 'Normal' | 'High' | 'Critical'>('Normal');
  const [category, setCategory] = useState<'System' | 'Cognitive' | 'Agent' | 'Security' | 'Wallet'>('Security');
  const [isDestructive, setIsDestructive] = useState(false);
  const [durationMs, setDurationMs] = useState<number>(3000); // 1500, 3000, 6000, 0 = persistent

  // Toast Live State
  const [toastVisible, setToastVisible] = useState(true);
  const [remainingProgress, setRemainingProgress] = useState(1.0); // 1.0 -> 0.0
  const [confirmedAction, setConfirmedAction] = useState(false);

  const timerRef = useRef<number | null>(null);
  const startTimestampRef = useRef<number | null>(null);

  const stopEvaporationTimer = useCallback(() => {
    if (timerRef.current !== null) {
      cancelAnimationFrame(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  useEffect(() => stopEvaporationTimer, [stopEvaporationTimer]);

  const triggerToast = useCallback(() => {
    stopEvaporationTimer();
    setToastVisible(true);
    setConfirmedAction(false);
    setRemainingProgress(1.0);

    // Play corresponding Karplus-Strong chime
    if (urgency === 'Critical' || isDestructive) {
      hakilleAudio.playStateSignal('attention', true, 0.7);
    } else if (urgency === 'High') {
      hakilleAudio.playStateSignal('eveil', true, 0.6);
    } else {
      hakilleAudio.playStateSignal('fait', true, 0.45);
    }

    if (durationMs > 0) {
      const startMs = performance.now();
      startTimestampRef.current = startMs;

      const step = (now: number) => {
        const elapsed = now - startMs;
        const progress = Math.max(0, 1.0 - elapsed / durationMs);
        setRemainingProgress(progress);

        if (progress > 0) {
          timerRef.current = requestAnimationFrame(step);
        } else {
          setToastVisible(false);
          stopEvaporationTimer();
        }
      };
      timerRef.current = requestAnimationFrame(step);
    }
  }, [durationMs, isDestructive, stopEvaporationTimer, urgency]);

  const handleDismiss = () => {
    stopEvaporationTimer();
    setToastVisible(false);
  };

  const handleBraiseConfirmed = () => {
    setConfirmedAction(true);
    window.setTimeout(() => {
      setToastVisible(false);
      setConfirmedAction(false);
    }, 1200);
  };

  const isBraiseStyled = urgency === 'Critical' || isDestructive;

  return (
    <div className="sim-shell hud-sandbox">
      <div className="sim-header">
        <div style={{ display: 'flex', alignItems: 'center', gap: '10px', flexWrap: 'wrap' }}>
          <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
            {t.hudToastSandbox.subtitle}
          </span>
          <strong>{t.hudToastSandbox.title}</strong>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span className="status-tag">
            {toastVisible
              ? durationMs === 0
                ? t.hudToastSandbox.previewStage.statusSticky
                : t.hudToastSandbox.previewStage.statusEvaporating
              : t.hudToastSandbox.previewStage.statusEvaporated}
          </span>
        </div>
      </div>

      <div className="sim-body hud-grid">
        {/* Left Column: Virtual Desktop Stage */}
        <div className="hud-desktop-column">
          <div className="virtual-desktop-stage">
            {/* Top Bar of Simulated OS Desktop */}
            <div className="desktop-top-bar">
              <span className="desktop-logo">{t.hudToastSandbox.previewStage.osBrand}</span>
              <div style={{ display: 'flex', gap: '14px', alignItems: 'center' }}>
                <span className="mono" style={{ fontSize: '0.72rem' }}>{t.hudToastSandbox.previewStage.sovereignBadge}</span>
                <span className="mono" style={{ fontSize: '0.72rem' }}>14:32</span>
              </div>
            </div>

            {/* Desktop Wallpaper Canvas Area */}
            <div className="desktop-canvas-area">
              {toastVisible ? (
                <div
                  className="hud-acrylic-window"
                  {...(isBraiseStyled ? { 'data-braise-context': 'decision-ring' as const } : {})}
                >
                  <div className="hud-toast-content">
                    <div className="hud-toast-header">
                      <span className="hud-category-chip">{category}</span>
                      <div style={{ display: 'flex', gap: '6px', alignItems: 'center' }}>
                        {confirmedAction && (
                          <span className="mono" style={{ fontSize: '0.72rem', color: 'var(--hakille-lueur)' }}>
                            ✓ {t.hudToastSandbox.previewStage.sealedBadge}
                          </span>
                        )}
                        <span className="hud-urgency-tag">{urgency}</span>
                      </div>
                    </div>

                    <h4 className="hud-toast-title">{title}</h4>
                    <p className="hud-toast-body">{body}</p>

                    <div className="hud-toast-actions">
                      {isDestructive ? (
                        <BraiseHoldButton
                          idleLabel={t.hudToastSandbox.actions.braiseIdle}
                          holdingLabel={t.hudToastSandbox.actions.braiseHolding}
                          armedLabel={t.hudToastSandbox.actions.braiseArmed}
                          confirmedLabel={t.hudToastSandbox.actions.braiseConfirmed}
                          cancelledHint={t.hudToastSandbox.actions.braiseCancelled}
                          accessibleTwoStep={false}
                          onConfirmed={handleBraiseConfirmed}
                        />
                      ) : (
                        <button
                          type="button"
                          className="ghost-btn"
                          style={{ padding: '4px 12px', fontSize: '0.8rem' }}
                          onClick={handleDismiss}
                        >
                          {t.hudToastSandbox.actions.dismissBtn}
                        </button>
                      )}
                    </div>
                  </div>

                  {/* Evaporation Progress Bar at bottom */}
                  {durationMs > 0 && (
                    <div className="hud-evap-bar-track">
                      <div
                        className="hud-evap-bar-fill"
                        style={{ width: `${(remainingProgress * 100).toFixed(1)}%` }}
                      />
                    </div>
                  )}
                </div>
              ) : (
                <div className="desktop-idle-placeholder">
                  <span className="mono" style={{ color: 'var(--hakille-texte-3)', fontSize: '0.82rem' }}>
                    {t.hudToastSandbox.previewStage.idleZeroFps}
                  </span>
                </div>
              )}
            </div>
          </div>

          {/* AccessKit Node Tree Inspector */}
          <div className="lab-panel" style={{ marginTop: '16px' }}>
            <h3 className="panel-title">
              {t.hudToastSandbox.accessKitInspector.title}
            </h3>
            <pre className="accesskit-code-block">
              {JSON.stringify(
                {
                  role: 'Role::Alert',
                  name: title,
                  description: body,
                  live: urgency === 'Critical' ? 'assertive' : 'polite',
                  children: isDestructive
                    ? [{ role: 'Role::Button', name: 'Valider (Hold 600ms)', braiseContext: 'validation' }]
                    : [{ role: 'Role::Button', name: 'Ignorer' }],
                  state: {
                    visible: toastVisible,
                    opacity: toastVisible ? 0.95 : 0.0,
                  },
                },
                null,
                2
              )}
            </pre>
          </div>
        </div>

        {/* Right Column: Configurator & Triggers */}
        <div className="hud-config-column">
          <div className="lab-panel">
            <h3 className="panel-title">
              {t.hudToastSandbox.configurator.title}
            </h3>

            <div className="field-group">
              <label className="field-label">{t.hudToastSandbox.configurator.fieldTitle}</label>
              <input
                type="text"
                className="sim-input"
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            </div>

            <div className="field-group">
              <label className="field-label">{t.hudToastSandbox.configurator.fieldBody}</label>
              <input
                type="text"
                className="sim-input"
                value={body}
                onChange={(e) => setBody(e.target.value)}
              />
            </div>

            <div className="field-row">
              <div className="field-group" style={{ flex: 1 }}>
                <label className="field-label">{t.hudToastSandbox.configurator.fieldUrgency}</label>
                <select
                  className="sim-select"
                  value={urgency}
                  onChange={(e) => setUrgency(e.target.value as typeof urgency)}
                >
                  <option value="Low">Low</option>
                  <option value="Normal">Normal</option>
                  <option value="High">High</option>
                  <option value="Critical">Critical (Braise)</option>
                </select>
              </div>

              <div className="field-group" style={{ flex: 1 }}>
                <label className="field-label">{t.hudToastSandbox.configurator.fieldCategory}</label>
                <select
                  className="sim-select"
                  value={category}
                  onChange={(e) => setCategory(e.target.value as typeof category)}
                >
                  <option value="Security">Security</option>
                  <option value="System">System</option>
                  <option value="Cognitive">Cognitive</option>
                  <option value="Agent">Agent</option>
                  <option value="Wallet">Wallet</option>
                </select>
              </div>
            </div>

            <div className="field-group">
              <label className="field-label">{t.hudToastSandbox.configurator.fieldDuration}</label>
              <select
                className="sim-select"
                value={durationMs}
                onChange={(e) => setDurationMs(Number.parseInt(e.target.value, 10))}
              >
                <option value={1500}>{t.hudToastSandbox.configurator.duration1500}</option>
                <option value={3000}>{t.hudToastSandbox.configurator.duration3000}</option>
                <option value={6000}>{t.hudToastSandbox.configurator.duration6000}</option>
                <option value={0}>{t.hudToastSandbox.configurator.durationPersistent}</option>
              </select>
            </div>

            <label className="checkbox-row" style={{ marginTop: '8px' }}>
              <input
                type="checkbox"
                checked={isDestructive}
                onChange={(e) => setIsDestructive(e.target.checked)}
              />
              <span>
                <strong>{t.hudToastSandbox.configurator.destructiveLabel}</strong>{' '}
                <span style={{ color: 'var(--hakille-texte-3)', fontSize: '0.76rem' }}>
                  ({t.hudToastSandbox.configurator.destructiveHint})
                </span>
              </span>
            </label>

            <div style={{ marginTop: '16px' }}>
              <button
                type="button"
                className="action-btn"
                style={{ width: '100%' }}
                onClick={triggerToast}
              >
                ▶ {t.hudToastSandbox.actions.triggerToastBtn}
              </button>
            </div>
          </div>

          <div className="lab-panel" style={{ marginTop: '16px' }}>
            <h3 className="panel-title">
              {t.hudToastSandbox.configurator.specsTitle}
            </h3>
            <ul className="clean-list" style={{ fontSize: '0.8rem', color: 'var(--hakille-texte-2)' }}>
              <li>
                <strong>{t.hudToastSandbox.configurator.specWidth}</strong> 720 px (Spec §12)
              </li>
              <li>
                <strong>{t.hudToastSandbox.configurator.specBlur}</strong> 30 px (DirectComposition / wlr-layer-shell)
              </li>
              <li>
                <strong>{t.hudToastSandbox.configurator.specRadius}</strong> 12 px
              </li>
              <li>
                <strong>{t.hudToastSandbox.configurator.specHold}</strong> 600 ms (RULE-02)
              </li>
              <li>
                <strong>{t.hudToastSandbox.configurator.specIdle}</strong> 0.0 fps
              </li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  );
}
