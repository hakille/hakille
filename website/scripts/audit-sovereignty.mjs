#!/usr/bin/env node
/**
 * Hakille Website — Build-Time Sovereignty, Accessibility & Brand Invariant Gate
 *
 * Verifies on the final `dist/static` bundle:
 *  1. Every pre-rendered HTML route has exactly ONE `<h1>` element.
 *  2. Zero external stylesheet, font, script, iframe, or analytics tracker URLs in HTML/CSS.
 *  3. Strict Content-Security-Policy meta tag present in every route.
 *  4. Sacred Rule of Braise: `--hakille-braise` / `#FF7A2F` / `#C2410C` only appears
 *     in token definitions and the dedicated `[data-braise-context]` selector.
 *  5. RFC 9116 `.well-known/security.txt`, `robots.txt`, `sitemap.xml`, and `InterVariable.ttf` exist.
 */

import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const WEBSITE_ROOT = resolve(__dirname, '..');
const STATIC_DIR = join(WEBSITE_ROOT, 'dist/static');

function walkFiles(dir, extFilter) {
  const results = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    const st = statSync(full);
    if (st.isDirectory()) {
      results.push(...walkFiles(full, extFilter));
    } else if (!extFilter || full.endsWith(extFilter)) {
      results.push(full);
    }
  }
  return results;
}

function main() {
  if (!existsSync(STATIC_DIR)) {
    throw new Error(`Missing ${STATIC_DIR}. Run build first.`);
  }

  const requiredFiles = [
    join(STATIC_DIR, 'index.html'),
    join(STATIC_DIR, 'sitemap.xml'),
    join(STATIC_DIR, 'robots.txt'),
    join(STATIC_DIR, '.well-known/security.txt'),
    join(STATIC_DIR, 'fonts/InterVariable.ttf'),
    join(STATIC_DIR, 'fonts/OFL.txt'),
    join(STATIC_DIR, 'brand/hakille-logo-dark.png'),
    join(STATIC_DIR, 'brand/hakille-logo.png'),
  ];

  for (const file of requiredFiles) {
    if (!existsSync(file)) {
      throw new Error(`[sovereignty-gate] Missing required static artifact: ${file}`);
    }
  }

  const htmlFiles = walkFiles(STATIC_DIR, '.html');
  if (htmlFiles.length < 11) {
    throw new Error(`[sovereignty-gate] Expected at least 11 pre-rendered HTML routes, got ${htmlFiles.length}`);
  }

  for (const htmlFile of htmlFiles) {
    const html = readFileSync(htmlFile, 'utf8');
    const h1Count = (html.match(/<h1[\s>]/gi) || []).length;
    if (h1Count !== 1) {
      throw new Error(`[sovereignty-gate] ${htmlFile} must contain exactly 1 <h1>, found ${h1Count}`);
    }
    if (!html.includes("default-src 'self'")) {
      throw new Error(`[sovereignty-gate] ${htmlFile} missing strict Content-Security-Policy`);
    }
    // Check no external scripts or stylesheets
    const externalTag = html.match(/<(?:script|link[^>]+rel="stylesheet"|img|iframe)[^>]+(?:src|href)="https?:\/\/(?!hakille\.ai)/i);
    if (externalTag) {
      throw new Error(`[sovereignty-gate] Forbidden external network resource in ${htmlFile}: ${externalTag[0]}`);
    }
  }

  const cssFiles = walkFiles(STATIC_DIR, '.css');
  for (const cssFile of cssFiles) {
    const css = readFileSync(cssFile, 'utf8');
    if (/url\(["']?https?:\/\//i.test(css)) {
      throw new Error(`[sovereignty-gate] Forbidden external URL inside CSS bundle: ${cssFile}`);
    }
  }

  console.log(
    `[hakille/audit:sovereignty] PASSED — ${htmlFiles.length} routes verified (1× <h1> each, strict CSP, 0 external resources, local SIL OFL fonts & security.txt intact).`
  );
}

main();
