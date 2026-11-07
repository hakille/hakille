import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';
import { AppLink } from '@/context/RouterContext';

export function NotFoundPage({ onOpenSpotlight }: { onOpenSpotlight: () => void }) {
  const { locale } = useI18n();

  return (
    <section className="page-hero" style={{ borderBlockEnd: 'none', paddingBlock: '96px' }}>
      <div
        className="container"
        style={{
          maxWidth: '680px',
          display: 'flex',
          flexDirection: 'column',
          gap: '20px',
        }}
      >
        <span className="eyebrow-mono">HTTP 404 · GRAPH NODE NOT FOUND</span>
        <h1 className="display-h1">
          {pickBi(
            locale,
            'Ce chemin n’existe pas dans le graphe public.',
            'This route does not exist in the public graph.'
          )}
        </h1>
        <p className="hero-lead">
          {pickBi(
            locale,
            'Aucune redirection trompeuse ni pistage : revenez au Manifeste ou ouvrez le HUD Spotlight (Alt+Espace / Ctrl+K) pour sauter instantanément vers l’une des 11 routes vérifiées.',
            'Zero silent tracking redirects: return to the Manifesto or summon the Spotlight HUD (Alt+Space / Ctrl+K) to jump directly to any of the 11 verified routes.'
          )}
        </p>
        <div style={{ display: 'flex', flexWrap: 'wrap', gap: '12px' }}>
          <AppLink to="/" className="primary-lueur-btn">
            {pickBi(locale, 'Retourner au Manifeste', 'Return to Manifesto')}
          </AppLink>
          <button type="button" className="secondary-outline-btn" onClick={onOpenSpotlight}>
            {pickBi(locale, 'Ouvrir Spotlight HUD (Ctrl+K)', 'Open Spotlight HUD (Ctrl+K)')}
          </button>
        </div>
      </div>
    </section>
  );
}
