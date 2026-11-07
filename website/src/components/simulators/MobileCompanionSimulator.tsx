import { useState } from 'react';
import { useI18n } from '@/i18n/context';

type MobileTab = 'voix' | 'memoire' | 'activite' | 'hub';
type VoiceMode = 'dictee' | 'conversation';
type CheckpointState = 'idle' | 'preparing' | 'in_flight' | 'confirmed' | 'unknown';

export function MobileCompanionSimulator() {
  const { locale } = useI18n();
  const isFr = locale === 'fr';

  const [tab, setTab] = useState<MobileTab>('voix');
  const [voiceMode, setVoiceMode] = useState<VoiceMode>('dictee');
  const [conversationConsent, setConversationConsent] = useState<boolean>(false);
  const [micState, setMicState] = useState<'stopped' | 'listening' | 'draft_ready'>('stopped');
  const [draftText, setDraftText] = useState<string>(
    'Rappelle-moi de vérifier le bail Biscuit du serveur local demain à 9h.'
  );
  const [checkpoint, setCheckpoint] = useState<CheckpointState>('idle');

  const startMutation = () => {
    setCheckpoint('preparing');
    window.setTimeout(() => {
      setCheckpoint((prev) => (prev === 'preparing' ? 'in_flight' : prev));
    }, 320);
  };

  const simulateCrashInFlight = () => {
    setCheckpoint('unknown');
  };

  const confirmNormalCompletion = () => {
    setCheckpoint('confirmed');
  };

  const acknowledgeUnknown = () => {
    setCheckpoint('idle');
  };

  return (
    <div className="sim-shell">
      <div className="sim-header">
        <div>
          <h3 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
            {isFr
              ? 'Compagnon Mobile Natif & Protocole Durable Checkpoint v2'
              : 'Native Mobile Companion & Durable Checkpoint v2 Protocol'}
          </h3>
          <p style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
            {isFr
              ? 'Spécification DESIGN_MOBILE_2026-11-04.md : iOS SwiftUI + Android Compose via UniFFI 0.32.2. Navigation Voix · Mémoire · Activité · Hub et refus absolu du rejeu aveugle après crash.'
              : 'Per DESIGN_MOBILE_2026-11-04.md: iOS SwiftUI + Android Compose over UniFFI 0.32.2. Canonical Voix · Mémoire · Activité · Hub navigation and zero blind mutation replay on crash.'}
          </p>
        </div>
        <span className="status-tag">Palier 3 · Invité Honnête (UniFFI 0.32.2)</span>
      </div>

      <div className="sim-body mobile-sim-layout">
        {/* Interactive Tactile Phone Device Frame */}
        <div className="phone-device-frame" aria-label="Simulated Hakille Mobile App">
          <div className="phone-top-bar">
            <span className="hakille-wordmark" style={{ fontSize: '0.95rem' }}>
              hakille
            </span>
            <span className="status-tag" style={{ fontSize: '0.68rem' }}>
              {checkpoint === 'unknown'
                ? isFr
                  ? '⚠ Résultat non confirmé'
                  : '⚠ Unconfirmed outcome'
                : isFr
                ? '● Hub LAN chiffré'
                : '● Encrypted LAN Hub'}
            </span>
          </div>

          <div className="phone-screen-body">
            {checkpoint === 'unknown' && (
              <div className="phone-unknown-banner" role="alert">
                <strong style={{ fontSize: '0.82rem', color: 'var(--hakille-texte-1)' }}>
                  {isFr
                    ? 'Checkpoint v2 : Le résultat n’est pas confirmé (unknown)'
                    : 'Checkpoint v2: Outcome is unconfirmed (unknown)'}
                </strong>
                <p style={{ fontSize: '0.76rem', color: 'var(--hakille-texte-2)' }}>
                  {isFr
                    ? 'L’application a été interrompue pendant un envoi en vol. Hakille ne rejoue jamais automatiquement une mutation incertaine.'
                    : 'The OS killed or disconnected the transport mid-flight. Hakille never replays an unconfirmed mutation automatically.'}
                </p>
                <button
                  type="button"
                  className="ghost-btn"
                  onClick={acknowledgeUnknown}
                  style={{ alignSelf: 'flex-start', minHeight: '32px', fontSize: '0.75rem' }}
                >
                  {isFr ? 'Prendre connaissance au Journal' : 'Acknowledge in Audit Log'}
                </button>
              </div>
            )}

            {tab === 'voix' && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
                <div className="segmented-row">
                  <button
                    type="button"
                    className="ghost-btn"
                    aria-pressed={voiceMode === 'dictee'}
                    onClick={() => setVoiceMode('dictee')}
                  >
                    {isFr ? 'Mode Dictée (défaut)' : 'Dictation (default)'}
                  </button>
                  <button
                    type="button"
                    className="ghost-btn"
                    aria-pressed={voiceMode === 'conversation'}
                    onClick={() => setVoiceMode('conversation')}
                  >
                    {isFr ? 'Mode Conversation' : 'Conversation'}
                  </button>
                </div>

                {voiceMode === 'conversation' && !conversationConsent && (
                  <div
                    style={{
                      padding: '10px 12px',
                      borderRadius: '8px',
                      border: '1px solid var(--hakille-texte-3)',
                      background: 'var(--hakille-surface-2)',
                      fontSize: '0.76rem',
                      display: 'flex',
                      flexDirection: 'column',
                      gap: '8px',
                    }}
                  >
                    <span>
                      {isFr
                        ? 'Consentement requis : la transcription finale sera transmise à votre Hub choisi et sa réponse corrélée pourra être lue à voix haute.'
                        : 'Explicit consent required: final transcript will be sent to your chosen Hub and its correlated reply may be spoken aloud.'}
                    </span>
                    <button
                      type="button"
                      className="ghost-btn"
                      onClick={() => setConversationConsent(true)}
                      style={{ alignSelf: 'flex-start', minHeight: '32px' }}
                    >
                      {isFr ? 'Autoriser pour cette session' : 'Authorize for this session'}
                    </button>
                  </div>
                )}

                <div
                  style={{
                    padding: '16px',
                    borderRadius: '12px',
                    border: '1px solid var(--hakille-ligne)',
                    background: 'var(--hakille-surface-0)',
                    textAlign: 'center',
                    display: 'flex',
                    flexDirection: 'column',
                    alignItems: 'center',
                    gap: '8px',
                  }}
                >
                  <div
                    style={{
                      width: '38px',
                      height: '38px',
                      borderRadius: '999px',
                      border:
                        micState === 'listening'
                          ? '2px solid var(--hakille-lueur)'
                          : '1px solid var(--hakille-ligne)',
                      display: 'grid',
                      placeItems: 'center',
                    }}
                  >
                    <span className="mono" style={{ fontSize: '0.8rem' }}>
                      {micState === 'listening' ? '◉' : '○'}
                    </span>
                  </div>
                  <strong style={{ fontSize: '0.86rem' }}>
                    {micState === 'stopped'
                      ? isFr
                        ? 'Le micro est arrêté'
                        : 'Microphone is stopped'
                      : micState === 'listening'
                      ? isFr
                        ? 'Je t’écoute (tour unique)'
                        : 'Listening (single turn)'
                      : isFr
                      ? 'Relis avant d’envoyer'
                      : 'Review draft before sending'}
                  </strong>
                  <span style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>
                    {isFr
                      ? 'Aucune écoute permanente du mot « Hakille » en arrière-plan.'
                      : 'Zero permanent background wake-word eavesdropping.'}
                  </span>
                  <div style={{ display: 'flex', gap: '8px', marginTop: '4px' }}>
                    <button
                      type="button"
                      className="primary-lueur-btn"
                      style={{ minHeight: '36px', paddingInline: '14px', fontSize: '0.8rem' }}
                      onClick={() =>
                        setMicState((s) =>
                          s === 'stopped' ? 'listening' : s === 'listening' ? 'draft_ready' : 'stopped'
                        )
                      }
                    >
                      {micState === 'listening'
                        ? isFr
                          ? 'Arrêter'
                          : 'Stop'
                        : isFr
                        ? 'Parler'
                        : 'Speak'}
                    </button>
                    <button
                      type="button"
                      className="ghost-btn"
                      style={{ minHeight: '36px', fontSize: '0.8rem' }}
                      onClick={() => setTab('activite')}
                    >
                      {isFr ? 'Journal' : 'Journal'}
                    </button>
                  </div>
                </div>

                <label style={{ display: 'flex', flexDirection: 'column', gap: '4px' }}>
                  <span className="eyebrow-mono" style={{ fontSize: '0.68rem' }}>
                    {isFr ? 'Brouillon local éditable' : 'Editable local draft'}
                  </span>
                  <textarea
                    rows={2}
                    value={draftText}
                    onChange={(e) => setDraftText(e.target.value)}
                    className="klatt-input"
                    style={{ fontSize: '0.8rem', resize: 'none' }}
                  />
                </label>
              </div>
            )}

            {tab === 'memoire' && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', fontSize: '0.8rem' }}>
                <span className="eyebrow-mono">
                  {isFr ? 'Faits consultables (plafond 50 / page)' : 'Inspectable SPO Facts (50 max)'}
                </span>
                <div className="data-card-compact">
                  <strong className="mono">(owner → hub_transport → TLS_Pinned)</strong>
                  <span style={{ color: 'var(--hakille-texte-3)', fontSize: '0.74rem' }}>
                    Cloison : Personnel · Frais (0,99)
                  </span>
                </div>
                <div className="data-card-compact">
                  <strong className="mono">(mobile:ios → speech_mode → Dictée_Brouillon)</strong>
                  <span style={{ color: 'var(--hakille-texte-3)', fontSize: '0.74rem' }}>
                    Cloison : Personnel · Frais (1,00)
                  </span>
                </div>
              </div>
            )}

            {tab === 'activite' && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '10px', fontSize: '0.8rem' }}>
                <span className="eyebrow-mono">
                  {isFr ? 'Machine à états Checkpoint v2' : 'Checkpoint v2 State Machine'}
                </span>
                <div className="data-card-compact">
                  <span>
                    {isFr ? 'État courant sur disque :' : 'Durable on-disk state:'}{' '}
                    <strong className="mono">{checkpoint}</strong>
                  </span>
                  <span style={{ fontSize: '0.74rem', color: 'var(--hakille-texte-3)' }}>
                    {checkpoint === 'idle' &&
                      (isFr ? 'Aucune mutation en attente.' : 'No pending mutation.')}
                    {checkpoint === 'preparing' &&
                      (isFr ? 'Écriture atomique avant envoi réseau…' : 'Atomic pre-flight write…')}
                    {checkpoint === 'in_flight' &&
                      (isFr
                        ? 'Requête envoyée au Hub — coupez le réseau à droite pour tester !'
                        : 'Request in flight to Hub — trigger crash on the right to test!')}
                    {checkpoint === 'confirmed' &&
                      (isFr
                        ? 'Accusé signé reçu du Hub et scellé dans audit.redb.'
                        : 'Signed receipt received from Hub and sealed in audit.redb.')}
                    {checkpoint === 'unknown' &&
                      (isFr
                        ? 'INCERTAIN : refus strict de ré-exécuter en doublon.'
                        : 'UNKNOWN: strict refusal to blindly re-execute.')}
                  </span>
                </div>
              </div>
            )}

            {tab === 'hub' && (
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', fontSize: '0.78rem' }}>
                <span className="eyebrow-mono">
                  {isFr ? 'Séparation Honnête Téléphone vs Hub' : 'Honest Phone vs Hub Split'}
                </span>
                <div className="data-card-compact">
                  <strong>{isFr ? 'Sur ce téléphone' : 'On this phone'}</strong>
                  <span style={{ color: 'var(--hakille-texte-3)' }}>
                    Dictée OS locale · Lecture vocale hors ligne · Photo/Fichier à l’action · 0 AccessibilityService intrusif
                  </span>
                </div>
                <div className="data-card-compact">
                  <strong>{isFr ? 'Sur ton Hub souverain' : 'On your sovereign Hub'}</strong>
                  <span style={{ color: 'var(--hakille-texte-3)' }}>
                    Graphe SPO redb · Whisper Tiny · Qwen 2.5 0.5B · Wasmtime 48 · Journal signé
                  </span>
                </div>
              </div>
            )}
          </div>

          {/* 4 Canonical Bottom Tabs per DESIGN_MOBILE_2026-11-04.md */}
          <div className="phone-bottom-nav" role="tablist">
            {(
              [
                { id: 'voix', fr: 'Voix', en: 'Voice' },
                { id: 'memoire', fr: 'Mémoire', en: 'Memory' },
                { id: 'activite', fr: 'Activité', en: 'Activity' },
                { id: 'hub', fr: 'Hub', en: 'Hub' },
              ] as const
            ).map((item) => (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={tab === item.id}
                className="phone-tab-btn"
                onClick={() => setTab(item.id)}
              >
                {isFr ? item.fr : item.en}
              </button>
            ))}
          </div>
        </div>

        {/* Right Side: Interactive Crash & Checkpoint v2 Bench */}
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', justifyContent: 'center' }}>
          <span className="eyebrow-mono">
            {isFr
              ? 'Banc de Torture · Protocole Durable Checkpoint v2 (ADR-011)'
              : 'Torture Bench · Durable Checkpoint v2 Protocol (ADR-011)'}
          </span>
          <h4 style={{ fontSize: 'var(--hakille-type-heading-size)', fontWeight: 600 }}>
            {isFr
              ? 'Pourquoi Hakille ne rejoue jamais une action interrompue en double.'
              : 'Why Hakille never double-executes an interrupted mobile action.'}
          </h4>
          <p style={{ color: 'var(--hakille-texte-2)', fontSize: '0.93rem', lineHeight: 1.65 }}>
            {isFr
              ? 'Sur iOS et Android, le système peut tuer l’application en arrière-plan à tout instant. Là où un client ordinaire renvoie aveuglément sa file d’attente au retour du réseau (risquant un double virement ou un double message), UniFFI grave l’état sur disque avant l’émission et bascule en état explicite « unknown » en cas de coupure.'
              : 'Mobile OS kernels aggressively suspend or kill background sockets. Instead of blindly retrying queued mutations on reconnect (risking duplicate transfers or emails), Hakille persists pre-flight state via UniFFI and transitions interrupted calls to an explicit "unknown" state requiring human review.'}
          </p>

          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '10px' }}>
            <button
              type="button"
              className="primary-lueur-btn"
              onClick={() => {
                setTab('activite');
                startMutation();
              }}
              disabled={checkpoint === 'in_flight' || checkpoint === 'preparing'}
            >
              {isFr ? '1. Émettre une requête vers le Hub' : '1. Dispatch Hub Mutation'}
            </button>

            <button
              type="button"
              className="ghost-btn"
              style={{ minHeight: '44px' }}
              disabled={checkpoint !== 'in_flight'}
              onClick={simulateCrashInFlight}
            >
              {isFr ? '2. ⚡ Simuler kill iOS/Android en plein vol' : '2. ⚡ Simulate Mid-Flight OS Kill'}
            </button>

            <button
              type="button"
              className="ghost-btn"
              style={{ minHeight: '44px' }}
              disabled={checkpoint !== 'in_flight'}
              onClick={confirmNormalCompletion}
            >
              {isFr ? '✓ Réceptionner accusé signé' : '✓ Receive Signed Receipt'}
            </button>
          </div>

          <div className="code-command-box">
            <span className="mono" style={{ fontSize: '0.8rem', color: 'var(--hakille-texte-2)' }}>
              idle → preparing (fsync) → in_flight →{' '}
              <strong style={{ color: 'var(--hakille-lueur)' }}>[{checkpoint.toUpperCase()}]</strong>
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}
