import React, { createContext, useContext, useEffect, useMemo, useState } from 'react';
import { hakilleAudio } from '@/engine/audioPlayer';

export type ThemeMode = 'dark' | 'light';
export type ContrastMode = 'normal' | 'high';
export type MotionMode = 'normal' | 'reduced';

export interface UserPreferences {
  readonly theme: ThemeMode;
  readonly contrast: ContrastMode;
  readonly textScale: number; // 0.8 .. 2.0
  readonly motion: MotionMode;
  readonly soundEnabled: boolean;
}

const DEFAULT_PREFS: UserPreferences = {
  theme: 'dark',
  contrast: 'normal',
  textScale: 1,
  motion: 'normal',
  soundEnabled: false, // Strictly muted by default per Hakille's Silence Doctrine
};

const PREFS_STORAGE_KEY = 'hakille.preferences.v1';

interface PreferencesContextValue extends UserPreferences {
  readonly setTheme: (theme: ThemeMode) => void;
  readonly toggleTheme: () => void;
  readonly setContrast: (contrast: ContrastMode) => void;
  readonly setTextScale: (scale: number) => void;
  readonly setMotion: (motion: MotionMode) => void;
  readonly toggleSound: () => void;
  readonly resetAllLocalData: () => void;
}

const PreferencesContext = createContext<PreferencesContextValue>({
  ...DEFAULT_PREFS,
  setTheme: () => undefined,
  toggleTheme: () => undefined,
  setContrast: () => undefined,
  setTextScale: () => undefined,
  setMotion: () => undefined,
  toggleSound: () => undefined,
  resetAllLocalData: () => undefined,
});

function loadInitialPreferences(): UserPreferences {
  if (typeof window === 'undefined') return DEFAULT_PREFS;
  try {
    const prefersReduced =
      typeof window.matchMedia === 'function' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const raw = window.localStorage.getItem(PREFS_STORAGE_KEY);
    if (!raw) {
      return { ...DEFAULT_PREFS, motion: prefersReduced ? 'reduced' : 'normal' };
    }
    const parsed = JSON.parse(raw) as Partial<UserPreferences>;
    return {
      theme: parsed.theme === 'light' ? 'light' : 'dark',
      contrast: parsed.contrast === 'high' ? 'high' : 'normal',
      textScale:
        typeof parsed.textScale === 'number'
          ? Math.max(0.8, Math.min(2.0, Number(parsed.textScale.toFixed(2))))
          : 1,
      motion: parsed.motion === 'reduced' || prefersReduced ? 'reduced' : 'normal',
      soundEnabled: false, // Always start muted on fresh page load
    };
  } catch {
    return DEFAULT_PREFS;
  }
}

export function PreferencesProvider({ children }: { children: React.ReactNode }) {
  const [prefs, setPrefs] = useState<UserPreferences>(() => loadInitialPreferences());

  useEffect(() => {
    if (typeof document === 'undefined') return;
    const root = document.documentElement;
    root.dataset.theme = prefs.theme;
    root.dataset.contrast = prefs.contrast;
    root.dataset.motion = prefs.motion;
    root.style.setProperty('--hakille-text-scale', String(prefs.textScale));

    try {
      const toPersist = {
        theme: prefs.theme,
        contrast: prefs.contrast,
        textScale: prefs.textScale,
        motion: prefs.motion,
      };
      window.localStorage.setItem(PREFS_STORAGE_KEY, JSON.stringify(toPersist));
    } catch {
      // Ignore storage errors
    }
  }, [prefs.theme, prefs.contrast, prefs.motion, prefs.textScale]);

  const value = useMemo<PreferencesContextValue>(
    () => ({
      ...prefs,
      setTheme: (theme) => setPrefs((p) => ({ ...p, theme })),
      toggleTheme: () => setPrefs((p) => ({ ...p, theme: p.theme === 'dark' ? 'light' : 'dark' })),
      setContrast: (contrast) => setPrefs((p) => ({ ...p, contrast })),
      setTextScale: (scale) =>
        setPrefs((p) => ({ ...p, textScale: Math.max(0.8, Math.min(2.0, Number(scale.toFixed(2)))) })),
      setMotion: (motion) => setPrefs((p) => ({ ...p, motion })),
      toggleSound: () =>
        setPrefs((p) => {
          const next = !p.soundEnabled;
          if (next) {
            hakilleAudio.playStateSignal('eveil', true, 0.55);
          }
          return { ...p, soundEnabled: next };
        }),
      resetAllLocalData: () => {
        if (typeof window !== 'undefined') {
          try {
            window.localStorage.removeItem(PREFS_STORAGE_KEY);
            window.localStorage.removeItem('hakille.locale');
          } catch {
            // Ignore
          }
        }
        setPrefs(DEFAULT_PREFS);
      },
    }),
    [prefs]
  );

  return <PreferencesContext.Provider value={value}>{children}</PreferencesContext.Provider>;
}

export function usePreferences(): PreferencesContextValue {
  return useContext(PreferencesContext);
}
