import type { AstroComponentFactory } from 'astro/runtime/server/index.js';

/**
 * Store-private PAGE overlays: a store may ship a bespoke Astro page
 * (e.g. `./private/88-about.astro`) that replaces the generic renderer for
 * that route. Globbed from src/lib/brand/private/ (excluded from the public
 * repo sync) so a checkout without overlays builds the generic pages.
 * Pages only — never import this from client-side islands.
 */
const pageModules = import.meta.glob<{ default: AstroComponentFactory }>(
  './private/*.astro',
  { eager: true },
);

/** Returns the overlay page component for `<slug>-<page>`, or null. */
export function pageOverlay(name: string): AstroComponentFactory | null {
  return pageModules[`./private/${name}.astro`]?.default ?? null;
}
