import type { JSX } from 'react';
import { useI18n } from '@/i18n/context';
import type { LocaleCode } from '@/i18n/types';

/**
 * Directional Arrow Helper matching RTL requirements:
 * In Arabic (RTL), forward CTA navigation points left ('←').
 * In LTR languages, forward CTA navigation points right ('→').
 */
export function getDirectionalArrow(locale: LocaleCode): '←' | '→' {
  return locale === 'ar' ? '←' : '→';
}

/**
 * Breadcrumb Separator Helper matching RTL requirements
 */
export function getBreadcrumbSeparator(locale: LocaleCode): '←' | '→' {
  return locale === 'ar' ? '←' : '→';
}

export interface DirectionalArrowProps {
  readonly className?: string;
  readonly glyph?: 'arrow' | 'breadcrumb';
  readonly ariaHidden?: boolean;
}

/**
 * RTL-Aware Directional CTA Arrow Glyph
 *
 * Automatically renders '←' in Arabic RTL documents and '→' in LTR documents,
 * ensuring users reading right-to-left see arrows pointing in their natural reading direction.
 */
export function DirectionalArrow({
  className = '',
  glyph = 'arrow',
  ariaHidden = true,
}: DirectionalArrowProps): JSX.Element {
  const { dir, locale } = useI18n();
  const isRtl = dir === 'rtl';
  const arrowChar = getDirectionalArrow(locale);

  return (
    <span
      className={`hakille-directional-arrow ${isRtl ? 'is-rtl' : 'is-ltr'} ${className}`.trim()}
      aria-hidden={ariaHidden}
      data-testid="directional-arrow"
      data-glyph={glyph}
    >
      {arrowChar}
    </span>
  );
}

export default DirectionalArrow;
