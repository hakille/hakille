export type RouteId =
  | 'home'
  | 'architecture'
  | 'honestStatus'
  | 'security'
  | 'constitution'
  | 'install'
  | 'privacy'
  | 'legalNotice'
  | 'conditions'
  | 'licenses'
  | 'accessibility';

export interface RouteDefinition {
  id: RouteId;
  path:
    | '/'
    | '/architecture'
    | '/etat-reel'
    | '/securite'
    | '/constitution'
    | '/installer'
    | '/confidentialite'
    | '/mentions-legales'
    | '/conditions'
    | '/licences'
    | '/accessibilite';
  group: 'product' | 'legal';
}

export const ROUTES_LIST: readonly RouteDefinition[] = [
  { id: 'home', path: '/', group: 'product' },
  { id: 'architecture', path: '/architecture', group: 'product' },
  { id: 'honestStatus', path: '/etat-reel', group: 'product' },
  { id: 'security', path: '/securite', group: 'product' },
  { id: 'constitution', path: '/constitution', group: 'product' },
  { id: 'install', path: '/installer', group: 'product' },
  { id: 'privacy', path: '/confidentialite', group: 'legal' },
  { id: 'legalNotice', path: '/mentions-legales', group: 'legal' },
  { id: 'conditions', path: '/conditions', group: 'legal' },
  { id: 'licenses', path: '/licences', group: 'legal' },
  { id: 'accessibility', path: '/accessibilite', group: 'legal' },
] as const;

export function isKnownRoute(rawPath: string): boolean {
  const cleaned = rawPath.split('?')[0]?.split('#')[0]?.replace(/\/+$/, '') || '/';
  const normalized = cleaned === '' ? '/' : cleaned;
  return ROUTES_LIST.some((r) => r.path === normalized);
}

export function normalizePathname(rawPath: string): RouteDefinition['path'] {
  const cleaned = rawPath.split('?')[0]?.split('#')[0]?.replace(/\/+$/, '') || '/';
  const normalized = cleaned === '' ? '/' : cleaned;
  const found = ROUTES_LIST.find((r) => r.path === normalized);
  return found ? found.path : '/';
}
