import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from 'react';
import { RouteDefinition, normalizePathname } from '@/config/routes';

interface RouterContextValue {
  readonly currentPath: RouteDefinition['path'];
  readonly navigate: (to: RouteDefinition['path'], hash?: string) => void;
}

const RouterContext = createContext<RouterContextValue>({
  currentPath: '/',
  navigate: () => undefined,
});

export function RouterProvider({
  initialPath,
  children,
}: {
  initialPath?: string;
  children: React.ReactNode;
}) {
  const [currentPath, setCurrentPath] = useState<RouteDefinition['path']>(() => {
    if (initialPath) return normalizePathname(initialPath);
    if (typeof window !== 'undefined') return normalizePathname(window.location.pathname);
    return '/';
  });

  useEffect(() => {
    if (typeof window === 'undefined') return;
    const onPopState = () => {
      setCurrentPath(normalizePathname(window.location.pathname));
    };
    window.addEventListener('popstate', onPopState);
    return () => window.removeEventListener('popstate', onPopState);
  }, []);

  const navigate = useCallback((to: RouteDefinition['path'], hash?: string) => {
    const cleanTarget = normalizePathname(to);
    setCurrentPath(cleanTarget);
    if (typeof window !== 'undefined') {
      const search = window.location.search;
      const hashPart = hash ? (hash.startsWith('#') ? hash : `#${hash}`) : '';
      window.history.pushState({}, '', `${cleanTarget}${search}${hashPart}`);
      if (hashPart) {
        const el = document.getElementById(hashPart.slice(1));
        if (el) {
          el.scrollIntoView({ behavior: 'smooth' });
          return;
        }
      }
      window.scrollTo({ top: 0, behavior: 'auto' });
    }
  }, []);

  const value = useMemo(() => ({ currentPath, navigate }), [currentPath, navigate]);

  return <RouterContext.Provider value={value}>{children}</RouterContext.Provider>;
}

export function useRouter(): RouterContextValue {
  return useContext(RouterContext);
}

export function AppLink({
  to,
  hash,
  className,
  ariaCurrent,
  onClickAfter,
  children,
}: {
  to: RouteDefinition['path'];
  hash?: string;
  className?: string;
  ariaCurrent?: 'page' | undefined;
  onClickAfter?: () => void;
  children: React.ReactNode;
}) {
  const { navigate } = useRouter();
  const href = `${to}${hash ? (hash.startsWith('#') ? hash : `#${hash}`) : ''}`;

  return (
    <a
      href={href}
      className={className}
      aria-current={ariaCurrent}
      onClick={(e) => {
        if (
          e.defaultPrevented ||
          e.button !== 0 ||
          e.metaKey ||
          e.ctrlKey ||
          e.shiftKey ||
          e.altKey
        ) {
          return;
        }
        e.preventDefault();
        navigate(to, hash);
        onClickAfter?.();
      }}
    >
      {children}
    </a>
  );
}
