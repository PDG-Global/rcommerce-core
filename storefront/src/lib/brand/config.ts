/**
 * Build-time brand configuration. The same storefront codebase serves
 * multiple stores from separate Cloudflare Pages projects; each project
 * sets these PUBLIC_* env vars at build time.
 *
 * Default values are generic (the rCommerce demo store). When no
 * PUBLIC_BRAND_SLUG is set and a store-private overlay provides
 * `configDefaults` (see src/lib/brand/private/, excluded from the public
 * repo sync), those become the defaults instead — so this checkout builds
 * its own store with zero env vars while the public template builds the
 * generic demo store.
 *
 * NOTE: do NOT copy another store's logo as a stand-in — each store must
 * provide its own logo asset.
 */

export interface MenuCategory {
  slug: string;
  label: string;
}

export interface BrandFavicons {
  any: string;
  png32: string;
  png16: string;
  apple: string;
}

export interface Brand {
  name: string;
  logo: string;
  primary: string;
  apiBase: string;
  /** Store key ('default' for the generic template) — selects overlays. */
  slug: string;
  /** Cloudflare Stream iframe src for the homepage hero video. */
  heroVideoUrl: string;
  /** Product slug featured in the hero panel (may not exist — render gracefully). */
  heroProductSlug: string;
  /** Fixed header/footer menu categories, in display order. */
  menuCategories: MenuCategory[];
  /** Class applied to <html> for themed builds ('' = none). */
  htmlClass: string;
  /** Optional per-brand stylesheet (private asset path, e.g. a brand scope). */
  themeStylesheet: string;
  /** Favicon set (private asset paths); null serves only the root favicon.svg. */
  favicons: BrandFavicons | null;
  /** Center the header nav links (minimal-header stores). */
  centeredNav: boolean;
  /** Soft product cards (rounded, hover lift, brand-colored price). */
  softCards: boolean;
  /**
   * Pre-select a sensible default variant on the product page (title match
   * on a "deck only"-style option, falling back to the cheapest) instead of
   * forcing an explicit pick. Stores whose variants are distinct
   * configurations (e.g. deck-only vs complete) enable this.
   */
  autoSelectDefaultVariant: boolean;
  /** Tab-title suffix for customer-facing pages. */
  titleSuffix: string;
}

/** Generic rCommerce demo store defaults (the public template's defaults). */
const GENERIC_DEFAULTS: Brand = {
  name: 'rCommerce Store',
  logo: '/images/rc-logo.svg',
  primary: '#C1481A',
  apiBase: 'https://api.example.com/api/v1',
  slug: 'default',
  heroVideoUrl: '',
  heroProductSlug: '',
  menuCategories: [{ slug: 'products', label: 'Shop' }],
  htmlClass: '',
  themeStylesheet: '',
  favicons: null,
  centeredNav: false,
  softCards: false,
  autoSelectDefaultVariant: false,
  titleSuffix: 'rCommerce',
};

interface PrivateOverlayModule {
  content?: unknown;
  configDefaults?: Partial<Brand>;
}

// Store-private overlays (src/lib/brand/private/, excluded from the public
// repo sync). Globbed so a checkout without the directory builds fine.
const overlayModules = import.meta.glob<PrivateOverlayModule>('./private/*.ts', {
  eager: true,
});

function overlayConfigDefaults(slug: string): Partial<Brand> | undefined {
  return overlayModules[`./private/${slug}.ts`]?.configDefaults;
}

function env(name: string): string | undefined {
  const value = import.meta.env[name];
  return typeof value === 'string' && value.trim() !== '' ? value : undefined;
}

/**
 * Slug resolution: an explicit PUBLIC_BRAND_SLUG always wins. Otherwise, if a
 * private overlay exists for the legacy '88' store, that is this checkout's
 * default store; without overlays the generic 'default' store builds.
 */
const envSlug = env('PUBLIC_BRAND_SLUG');
const slug = envSlug ?? (overlayConfigDefaults('88') ? '88' : 'default');
// The active brand's private overlay may supply build-time defaults (full or
// partial) layered over the generic demo defaults. Explicit PUBLIC_* env vars
// always win for the classic scalar fields.
const DEFAULTS: Brand = { ...GENERIC_DEFAULTS, ...(overlayConfigDefaults(slug) ?? {}) };

/// Parses PUBLIC_BRAND_MENU: comma-separated "slug:Label" pairs,
/// e.g. "wheels:Wheels,bearings:Bearings". Falls back to the active defaults.
function parseMenuCategories(value: string | undefined): MenuCategory[] {
  if (!value) return DEFAULTS.menuCategories;
  const parsed = value
    .split(',')
    .map((pair) => pair.trim())
    .filter(Boolean)
    .map((pair) => {
      const sep = pair.indexOf(':');
      const slug = (sep === -1 ? pair : pair.slice(0, sep)).trim();
      const label = sep === -1 ? slug : pair.slice(sep + 1).trim();
      return { slug, label: label || slug };
    })
    .filter((c) => c.slug.length > 0);
  return parsed.length > 0 ? parsed : DEFAULTS.menuCategories;
}

export const brand: Brand = {
  name: env('PUBLIC_BRAND_NAME') ?? DEFAULTS.name,
  logo: env('PUBLIC_BRAND_LOGO') ?? DEFAULTS.logo,
  primary: env('PUBLIC_BRAND_PRIMARY') ?? DEFAULTS.primary,
  apiBase: env('PUBLIC_API_BASE') ?? DEFAULTS.apiBase,
  slug,
  heroVideoUrl: env('PUBLIC_HERO_VIDEO_URL') ?? DEFAULTS.heroVideoUrl,
  heroProductSlug: env('PUBLIC_HERO_PRODUCT_SLUG') ?? DEFAULTS.heroProductSlug,
  menuCategories: parseMenuCategories(env('PUBLIC_BRAND_MENU')),
  htmlClass: DEFAULTS.htmlClass,
  themeStylesheet: DEFAULTS.themeStylesheet,
  favicons: DEFAULTS.favicons,
  centeredNav: DEFAULTS.centeredNav,
  softCards: DEFAULTS.softCards,
  autoSelectDefaultVariant: DEFAULTS.autoSelectDefaultVariant,
  titleSuffix: DEFAULTS.titleSuffix,
};
