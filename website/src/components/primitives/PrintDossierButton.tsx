import { pickBi } from '@/i18n/types';
import { useI18n } from '@/i18n/context';

export function PrintDossierButton() {
  const { locale } = useI18n();

  const handlePrint = () => {
    if (typeof window !== 'undefined' && typeof window.print === 'function') {
      window.print();
    }
  };

  return (
    <button
      type="button"
      className="ghost-btn no-print"
      onClick={handlePrint}
      style={{ alignSelf: 'flex-start' }}
    >
      <span aria-hidden="true">⎙</span>
      <span>
        {pickBi(
          locale,
          'Imprimer / Exporter ce dossier officiel (PDF)',
          'Print / Export Official Dossier (PDF)'
        )}
      </span>
    </button>
  );
}
