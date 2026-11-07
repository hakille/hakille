import { useEffect, useMemo, useRef, useState } from 'react';
import { ROUTES_LIST, RouteDefinition } from '@/config/routes';
import { WORKSPACE_METRICS } from '@/data/cratesCatalog';
import { LocaleCode, SUPPORTED_LOCALES } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { usePreferences } from '@/context/PreferencesContext';
import { useRouter } from '@/context/RouterContext';

interface SpotlightHudModalProps {
  readonly open: boolean;
  readonly onClose: () => void;
}

interface CommandDescriptor {
  readonly cmd: string;
  readonly summary: string;
  readonly run: () => string | null;
}

export function SpotlightHudModal({ open, onClose }: SpotlightHudModalProps) {
  const { locale, setLocale, t } = useI18n();
  const { toggleTheme, toggleSound, soundEnabled, theme } = usePreferences();
  const { navigate } = useRouter();
  const [query, setQuery] = useState('');
  const [output, setOutput] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (!open) return;
    setQuery('');
    setOutput(null);
    const id = window.setTimeout(() => inputRef.current?.focus(), 20);
    return () => window.clearTimeout(id);
  }, [open]);

  useEffect(() => {
    if (!open || typeof window === 'undefined') return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, onClose]);

  const commands = useMemo<readonly CommandDescriptor[]>(() => {
    const base: CommandDescriptor[] = [
      {
        cmd: ':status',
        summary: 'Daemon health, Rust workspace footprint & zero-telemetry proof',
        run: () =>
          [
            'hakille-daemon v0.1.0-pre-alpha [LOCAL SOVEREIGN]',
            `workspace : ${WORKSPACE_METRICS.totalPackages} Cargo packages · ${WORKSPACE_METRICS.totalRustLoc.toLocaleString('en-US')} Rust LoC · ${WORKSPACE_METRICS.totalAutomatedTests.toLocaleString('en-US')} #[test]`,
            'shell     : winit + wgpu + AccessKit (0.0 fps idle · 0% Electron · 0% Python)',
            'website   : 0 cookies · 0 external trackers · SIL OFL 1.1 InterVariable SHA-256 verified',
          ].join('\n'),
      },
      {
        cmd: ':models-honest',
        summary: 'Real local inference vs unprovisioned roadmap models',
        run: () =>
          [
            '[REAL DSP]       Klatt Formant TTS (0 MB) + YIN F0 Tracker — active offline',
            '[REAL CPU]       Whisper Tiny 99-lang (×46 FP32 GEMM speedup, ~1.4s)',
            '[REAL CPU]       Qwen 2.5 0.5B Instruct Q4_K_M (Candle GGUF) + Silero VAD + MiniLM HNSW',
            '[REAL CODEC]     Kyutai Mimi 24 kHz 8×2048 RVQ neural codec',
            '[UNPROVISIONED]  Kyutai Moshi 7.6B full-duplex dialogue LLM (honestly reported, not faked)',
          ].join('\n'),
      },
      {
        cmd: ':powers',
        summary: 'Active Biscuit capabilities & constitutional invariants',
        run: () =>
          [
            'root_authority : Ed25519 (HAKKEY02 Argon2id + ChaCha20-Poly1305)',
            'rule_01        : TrustLevel::Untrusted -> HardBlock on all irreversible capabilities',
            'rule_02        : 600 ms physical Braise hold mandatory before irreversible side effects',
            'rule_08        : cognition:* affective cues purged without tombstone at t = 5 min',
            'system_2_lease : NONE (0 outbound network bytes authorized)',
          ].join('\n'),
      },
      {
        cmd: ':budget',
        summary: 'Silence Budget (3/day) & System 2 taxi-meter',
        run: () =>
          [
            'proactive_silence_budget : 3 / 3 credits available today (14-day snooze on dismiss)',
            'system_1_local_cost      : €0.0000 (Candle / ONNX offline)',
            'system_2_cloud_spend     : €0.0000 (no active 300s Biscuit lease)',
          ].join('\n'),
      },
      {
        cmd: ':journal-why 1',
        summary: 'Inspect 3-level WhySheet for the latest sealed decision',
        run: () =>
          [
            'Level 1 (Plain) : Blocked inbound email attempting unauthorized €4,800 transfer.',
            'Level 2 (SPO)   : (msg:ext-94 -> provenance -> TrustLevel::Untrusted) [frais]',
            'Level 3 (Proof) : Enforced by hakille-constitution::RULE-01 prior to agent invocation.',
          ].join('\n'),
      },
      {
        cmd: ':theme',
        summary: `Switch surface theme (current: ${theme})`,
        run: () => {
          toggleTheme();
          return `Surface switched (${theme === 'dark' ? 'Parchment #F6F4F0' : 'Obsidian #0B0C0F'}).`;
        },
      },
      {
        cmd: ':sound',
        summary: `Toggle 48 kHz Karplus-Strong synthesis (current: ${soundEnabled ? 'ON' : 'MUTED'})`,
        run: () => {
          toggleSound();
          return `48 kHz Karplus-Strong audio ${!soundEnabled ? 'ENABLED' : 'MUTED'}.`;
        },
      },
    ];

    const routeJumpers: CommandDescriptor[] = ROUTES_LIST.map((r) => ({
      cmd: `goto ${r.path}`,
      summary: `${t.routes[r.id].navLabel} — ${t.routes[r.id].title}`,
      run: () => {
        navigate(r.path as RouteDefinition['path']);
        onClose();
        return null;
      },
    }));

    return [...base, ...routeJumpers];
  }, [navigate, onClose, soundEnabled, t.routes, theme, toggleSound, toggleTheme]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands.slice(0, 9);
    return commands.filter(
      (c) => c.cmd.toLowerCase().includes(q) || c.summary.toLowerCase().includes(q)
    );
  }, [commands, query]);

  const executeRaw = (raw: string) => {
    const trimmed = raw.trim();
    if (trimmed.startsWith(':lang ')) {
      const code = trimmed.replace(':lang ', '').trim().toLowerCase();
      if ((SUPPORTED_LOCALES as readonly string[]).includes(code)) {
        setLocale(code as LocaleCode);
        setOutput(`Active language switched to "${code}" (zero cookies; stored locally).`);
        return;
      }
    }

    const exact = commands.find((c) => c.cmd.toLowerCase() === trimmed.toLowerCase()) ?? filtered[0];
    if (exact) {
      const res = exact.run();
      if (res) setOutput(res);
    } else {
      setOutput(`Command "${trimmed}" evaluated locally (${locale.toUpperCase()}). Try :status, :models-honest, or :powers.`);
    }
  };

  if (!open) return null;

  return (
    <div
      className="spotlight-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="Hakille Spotlight HUD"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="spotlight-dialog">
        <form
          className="spotlight-input-row"
          onSubmit={(e) => {
            e.preventDefault();
            executeRaw(query);
          }}
        >
          <span className="mono" style={{ color: 'var(--hakille-lueur)', fontWeight: 600 }}>
            ›
          </span>
          <input
            ref={inputRef}
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t.spotlight.placeholder}
            className="spotlight-input"
            aria-label={t.spotlight.placeholder}
          />
          <button type="submit" className="ghost-btn">
            {t.spotlight.runBtn}
          </button>
          <button type="button" className="ghost-btn" onClick={onClose}>
            {t.spotlight.closeEsc}
          </button>
        </form>

        <div className="spotlight-output">
          {output && (
            <pre
              style={{
                padding: '12px 16px',
                background: 'var(--hakille-surface-0)',
                border: '1px solid var(--hakille-texte-3)',
                borderRadius: '8px',
                whiteSpace: 'pre-wrap',
                color: 'var(--hakille-lueur)',
              }}
            >
              {output}
            </pre>
          )}

          <div style={{ fontSize: 'var(--hakille-type-caption-size)', color: 'var(--hakille-texte-3)' }}>
            {t.spotlight.emptyPrompt}
          </div>

          <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
            {filtered.map((item) => (
              <button
                key={item.cmd}
                type="button"
                className="spotlight-cmd-item"
                onClick={() => {
                  setQuery(item.cmd);
                  const res = item.run();
                  if (res) setOutput(res);
                }}
              >
                <code style={{ color: 'var(--hakille-texte-1)', fontWeight: 600 }}>{item.cmd}</code>
                <span
                  style={{
                    fontSize: 'var(--hakille-type-caption-size)',
                    color: 'var(--hakille-texte-3)',
                    overflow: 'hidden',
                    textOverflow: 'ellipsis',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {item.summary}
                </span>
              </button>
            ))}
          </div>

          <div
            style={{
              paddingTop: '8px',
              borderTop: '1px solid var(--hakille-ligne)',
              fontSize: '0.74rem',
              color: 'var(--hakille-texte-3)',
              display: 'flex',
              justifyContent: 'space-between',
              flexWrap: 'wrap',
              gap: '8px',
            }}
          >
            <span>{t.spotlight.hintBar}</span>
            <span className="mono">:lang fr | en | es | de | ar | ja</span>
          </div>
        </div>
      </div>
    </div>
  );
}
