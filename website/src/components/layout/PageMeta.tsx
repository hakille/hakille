import { useEffect } from 'react';
import { ROUTES_LIST } from '@/config/routes';
import { useRouter } from '@/context/RouterContext';
import { useI18n } from '@/i18n/context';

export function PageMeta() {
  const { currentPath } = useRouter();
  const { t } = useI18n();

  useEffect(() => {
    if (typeof document === 'undefined') return;
    const matched = ROUTES_LIST.find((r) => r.path === currentPath) ?? ROUTES_LIST[0]!;
    const seo = t.routes[matched.id];
    document.title = seo.title;

    let descTag = document.querySelector('meta[name="description"]');
    if (!descTag) {
      descTag = document.createElement('meta');
      descTag.setAttribute('name', 'description');
      document.head.appendChild(descTag);
    }
    descTag.setAttribute('content', seo.description);
  }, [currentPath, t]);

  return null;
}
