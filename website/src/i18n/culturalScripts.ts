/**
 * Hakille — Cultural & Multi-Script Typography Specimens
 *
 * Honors the Fulani/Pulaar etymology of *Hakille* ("intelligence, discernment, wisdom"),
 * the 21-string Manding Kora acoustic heritage, and the Unicode fallback chain defined in
 * `tokens/type.toml`:
 *   ["Noto Sans", "Noto Sans Arabic", "Noto Sans Devanagari", "Noto Sans CJK", "Noto Sans Adlam", "Noto Sans NKo"]
 */

export interface CulturalScriptSpecimen {
  readonly id: string;
  readonly languageName: string;
  readonly scriptName: string;
  readonly dir: 'ltr' | 'rtl';
  readonly wordmarkSample: string;
  readonly phraseSample: string;
  readonly transliteration: string;
  readonly meaningEn: string;
  readonly meaningFr: string;
}

export const CULTURAL_SCRIPT_SPECIMENS: readonly CulturalScriptSpecimen[] = [
  {
    id: 'pulaar-adlam',
    languageName: 'Pulaar / Fulfulde (Peul)',
    scriptName: 'Adlam (𞤀𞤣𞤤𞤢𞤥)',
    dir: 'rtl',
    wordmarkSample: '𞤖𞤢𞤳𞤭𞤤𞥆𞤫',
    phraseSample: '𞤖𞤢𞤳𞤭𞤤𞥆𞤫 — 𞤲𞤣𞤭𞤥𞤵 𞤸𞤢𞤳𞥆𞤭𞤤𞤢𞤲𞤼𞤢𞥄𞤺𞤢𞤤',
    transliteration: 'Hakille — ndimu hakkilantaagal',
    meaningEn: 'Intelligence, discernment, and sovereign wisdom — the living root of Hakille’s name.',
    meaningFr: 'L’intelligence, le discernement et la sagesse souveraine — racine vivante du nom Hakille.',
  },
  {
    id: 'manding-nko',
    languageName: 'Manding / Bambara / Maninka',
    scriptName: "N'Ko (ߒߞߏ)",
    dir: 'rtl',
    wordmarkSample: 'ߤߊߞߌߟߌ',
    phraseSample: 'ߤߊߞߌߟߌ — ߞߏߟߊ߫ ߖߎ߬ߟߎ ߂߁ ߠߎ߬',
    transliteration: 'Hakili — Kora julu 21 lu',
    meaningEn: 'Mind & discernment beneath the 21 strings of the West African Kora.',
    meaningFr: 'L’esprit et le discernement sous les 21 cordes de la kora ouest-africaine.',
  },
  {
    id: 'arabic-rtl',
    languageName: 'العربية (Arabic)',
    scriptName: 'Arabic (RTL)',
    dir: 'rtl',
    wordmarkSample: 'هاكيلي — ذكاء سيادي',
    phraseSample: 'النظام الذي لا ينسى شيئًا ويقول دائمًا ما فعله.',
    transliteration: 'An-niẓām alladhī lā yansā shay’an wa yaqūlu dā’iman mā fa‘alah',
    meaningEn: 'Full RTL layout mirroring while keeping acoustic waveforms invariant (`typo.rs`).',
    meaningFr: 'Miroir complet de mise en page RTL tout en gardant l’onde acoustique invariante (`typo.rs`).',
  },
  {
    id: 'sanskrit-devanagari',
    languageName: 'हिन्दी / देवनागरी',
    scriptName: 'Devanagari',
    dir: 'ltr',
    wordmarkSample: 'हाकिले — स्वायत्त प्रज्ञा',
    phraseSample: 'स्मृति आपके अपने यंत्र पर रहती है, किसी बाहरी मेघ में नहीं।',
    transliteration: 'Smṛti āpake apane yantra par rahatī hai',
    meaningEn: 'Memory lives on your own machine, never inside an external cloud.',
    meaningFr: 'La mémoire demeure sur votre propre machine, jamais dans un nuage tiers.',
  },
  {
    id: 'japanese-cjk',
    languageName: '日本語 (Japanese)',
    scriptName: 'Kanji · Hiragana · Katakana',
    dir: 'ltr',
    wordmarkSample: 'hakille — 主権的知性',
    phraseSample: '何も忘れず、行ったことを常に証明するシステム。',
    transliteration: 'Nanimo wasurezu, okonatta koto o tsuneni shōmei suru shisutemu.',
    meaningEn: 'The system that forgets nothing and always proves what it did.',
    meaningFr: 'Le système qui n’oublie rien et prouve toujours ce qu’il a accompli.',
  },
] as const;
