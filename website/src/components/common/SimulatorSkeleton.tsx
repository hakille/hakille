import type { JSX } from 'react';
import { useI18n } from '@/i18n/context';
import { pickBi } from '@/i18n/types';

export interface SimulatorSkeletonProps {
  readonly title?: string;
  readonly description?: string;
  readonly minHeight?: number | string;
  readonly className?: string;
}

/**
 * Cybernetic Simulator Loading Skeleton
 *
 * Provides a graceful, accessible placeholder during dynamic chunk loading.
 * Styled with mineral glow aura (--hakille-lueur) and acrylic borders.
 * Conforms strictly to the Sacred Rule of Braise (zero braise color).
 */
export function SimulatorSkeleton({
  title,
  description,
  minHeight = 360,
  className = '',
}: SimulatorSkeletonProps): JSX.Element {
  const { locale } = useI18n();

  const defaultTitle = pickBi(
    locale,
    'Simulateur Souverain Hakille',
    'Hakille Sovereign Simulator'
  );

  const defaultDescription = pickBi(
    locale,
    'Chargement asynchrone des modèles mathématiques et du moteur audio/crypto...',
    'Asynchronously loading mathematical models, audio synthesis, and cryptographic engines...'
  );

  const statusLabel = pickBi(locale, 'INITIALISATION...', 'INITIALIZING...');

  const heightStyle = typeof minHeight === 'number' ? `${minHeight}px` : minHeight;

  return (
    <div
      className={`sim-skeleton-frame ${className}`.trim()}
      style={{ minHeight: heightStyle }}
      role="status"
      aria-live="polite"
      aria-busy="true"
      data-testid="simulator-skeleton"
    >
      <div className="sim-skeleton-header">
        <div className="sim-skeleton-meta">
          <span className="mono eyebrow-mono" style={{ color: 'var(--hakille-lueur)' }}>
            [ {statusLabel} ]
          </span>
          <h3 className="sim-skeleton-title">{title ?? defaultTitle}</h3>
          <p className="sim-skeleton-desc">{description ?? defaultDescription}</p>
        </div>
        <div className="sim-skeleton-pulse-ring" aria-hidden="true" />
      </div>

      <div className="sim-skeleton-body" aria-hidden="true">
        <div className="sim-skeleton-shimmer-bar" style={{ width: '45%' }} />
        <div className="sim-skeleton-stage">
          <div className="sim-skeleton-grid-line" />
          <div className="sim-skeleton-grid-line" />
          <div className="sim-skeleton-grid-line" />
        </div>
        <div className="sim-skeleton-controls">
          <div className="sim-skeleton-shimmer-chip" />
          <div className="sim-skeleton-shimmer-chip" />
          <div className="sim-skeleton-shimmer-chip" />
        </div>
      </div>
    </div>
  );
}

export default SimulatorSkeleton;
