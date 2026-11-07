#!/usr/bin/env node
/**
 * Hakille Website — Static Multi-Route Pre-Renderer (SSG) & Sitemap Generator
 *
 * Pre-renders all 11 canonical routes into `dist/static/<route>/index.html`
 * with full semantic HTML, route-specific <title>, <meta name="description">,
 * OpenGraph tags, canonical URL, hreflang links for all 8 supported languages,
 * and JSON-LD structured data — ensuring complete readability even without JS.
 */

import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const WEBSITE_ROOT = resolve(__dirname, '..');
const CLIENT_DIR = join(WEBSITE_ROOT, 'dist/client');
const SERVER_ENTRY = join(WEBSITE_ROOT, 'dist/server/entry-server.js');
const STATIC_OUT = join(WEBSITE_ROOT, 'dist/static');

function escapeHtml(str) {
  return String(str)
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

async function main() {
  if (!existsSync(CLIENT_DIR) || !existsSync(SERVER_ENTRY)) {
    throw new Error('Client and SSR bundles must be built before running prerender.mjs');
  }

  if (existsSync(STATIC_OUT)) {
    rmSync(STATIC_OUT, { recursive: true, force: true });
  }
  cpSync(CLIENT_DIR, STATIC_OUT, { recursive: true });

  const template = readFileSync(join(CLIENT_DIR, 'index.html'), 'utf8');
  const serverModule = await import(pathToFileURL(SERVER_ENTRY).href);
  const { renderRoute, ROUTES_LIST, SUPPORTED_LOCALES } = serverModule;

  const baseUrl = 'https://hakille.ai';
  const sitemapEntries = [];

  for (const route of ROUTES_LIST) {
    const { appHtml, title, description, canonicalPath } = renderRoute(route.path, 'fr');
    const fullUrl = `${baseUrl}${canonicalPath === '/' ? '' : canonicalPath}`;

    const hreflangLinks = SUPPORTED_LOCALES.map((loc) => {
      const href = loc === 'fr' ? fullUrl : `${fullUrl}?lang=${loc}`;
      return `    <link rel="alternate" hreflang="${loc}" href="${escapeHtml(href)}" />`;
    }).join('\n');

    const headInjection = [
      `    <title>${escapeHtml(title)}</title>`,
      `    <meta name="description" content="${escapeHtml(description)}" />`,
      `    <link rel="canonical" href="${escapeHtml(fullUrl)}" />`,
      hreflangLinks,
      `    <link rel="alternate" hreflang="x-default" href="${escapeHtml(fullUrl)}" />`,
      `    <meta property="og:type" content="website" />`,
      `    <meta property="og:site_name" content="hakille" />`,
      `    <meta property="og:title" content="${escapeHtml(title)}" />`,
      `    <meta property="og:description" content="${escapeHtml(description)}" />`,
      `    <meta property="og:url" content="${escapeHtml(fullUrl)}" />`,
      `    <meta property="og:image" content="${baseUrl}/brand/hakille-logo-square.png" />`,
      `    <meta name="twitter:card" content="summary_large_image" />`,
      `    <meta name="twitter:title" content="${escapeHtml(title)}" />`,
      `    <meta name="twitter:description" content="${escapeHtml(description)}" />`,
    ].join('\n');

    const pageHtml = template
      .replace(/<title>[\s\S]*?<\/title>/, '')
      .replace('<!--HAKILLE_HEAD_META-->', headInjection)
      .replace('<!--HAKILLE_SSR_OUTLET-->', appHtml);

    const outDir =
      route.path === '/' ? STATIC_OUT : join(STATIC_OUT, route.path.replace(/^\//, ''));
    mkdirSync(outDir, { recursive: true });
    writeFileSync(join(outDir, 'index.html'), pageHtml, 'utf8');

    const xhtmlAlternates = SUPPORTED_LOCALES.map((loc) => {
      const href = loc === 'fr' ? fullUrl : `${fullUrl}?lang=${loc}`;
      return `    <xhtml:link rel="alternate" hreflang="${loc}" href="${escapeHtml(href)}" />`;
    }).join('\n');

    sitemapEntries.push(`  <url>
    <loc>${escapeHtml(fullUrl)}</loc>
${xhtmlAlternates}
    <changefreq>weekly</changefreq>
    <priority>${route.path === '/' ? '1.0' : '0.8'}</priority>
  </url>`);
  }

  const sitemapXml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"
        xmlns:xhtml="http://www.w3.org/1999/xhtml">
${sitemapEntries.join('\n')}
</urlset>
`;
  writeFileSync(join(STATIC_OUT, 'sitemap.xml'), sitemapXml, 'utf8');

  console.log(
    `[hakille/prerender] Pre-rendered ${ROUTES_LIST.length} static routes + sitemap.xml (${SUPPORTED_LOCALES.length} languages).`
  );
}

main();
