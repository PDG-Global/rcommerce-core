/**
 * Per-brand user-visible content (copy, links, emails) for the whitelabelled
 * storefront. `default.ts` holds the full generic default-store content;
 * a store ships a DeepPartial override that is deep-merged over the defaults
 * at build time, selected by PUBLIC_BRAND_SLUG.
 */

export interface Link {
  label: string;
  href: string;
}

export interface FooterColumn {
  title: string;
  links: Link[];
}

export interface BrandContent {
  /** Default <meta name="description"> for pages that don't set one. */
  tagline: string;
  /**
   * Category slugs hidden from the sitemap, category navigation and homepage
   * tiles (e.g. system/import leftovers).
   */
  hiddenCategories: string[];
  contact: {
    email: string;
    /** Extra channel links shown next to the email (Telegram etc.). */
    links: Link[];
    /** Intro paragraphs above the contact details. */
    intro: string[];
    /** Hero photo on the split contact page; empty hides the image block. */
    heroImage: string;
    /** Pill CTA label on the accordion-style contact page (mailto: email). */
    emailCtaLabel: string;
    /** "Quick questions" accordion items (accordion contact variant); [] selects the form variant. */
    faqs: { question: string; answer: string[] }[];
  };
  /**
   * Static nav links rendered after the category menu, before Contact.
   * Contact is always appended by the Nav component.
   */
  navLinks: Link[];
  /** Catalog vocabulary used in page descriptions, alt text and ordering. */
  catalog: {
    /** "Shop <brand> — premium <line>." / search-result descriptions. */
    line: string;
    /** "Shop <brand> <category> — <categoryLine>." */
    categoryLine: string;
    /** Alt-text noun for catalog photography. */
    photoAlt: string;
    /** Preferred category ordering for tiles/chips; [] = API order. */
    categoryOrder: string[];
  };
  footer: {
    blurb: string;
    supportLinks: Link[];
    /**
     * Per-product footer link columns. When empty, the footer renders one
     * "Shop <label>" column per menu category instead.
     */
    productColumns: FooterColumn[];
    socialLinks: Link[];
  };
  /** Shown to customers when automatic payment confirmation times out. */
  checkoutSupportEmail: string;
  /**
   * Structured-data (JSON-LD) shipping + returns facts for product offers,
   * sourced from the store's real carrier zones and policy.
   */
  seo: {
    /** Merchant return window in days. */
    returnDays: number;
    /** Who pays return shipping. */
    returnFees:
      | 'https://schema.org/FreeReturn'
      | 'https://schema.org/ReturnFeesCustomerResponsibility'
      | 'https://schema.org/ReturnShippingFees';
    /**
     * Per-destination shipping rates surfaced in offers.shippingDetails:
     * ISO country code + one-unit price in the store currency.
     */
    shipping: { country: string; price: string }[];
  };
  /** Static content pages this store serves; false redirects to contact. */
  pages: { about: boolean; support: boolean };
  /**
   * 301 redirects for discontinued/merged product slugs
   * (old slug -> canonical product path), checked when a product is missing
   * or inactive.
   */
  productRedirects: Record<string, string>;
  /**
   * 301 redirects for legacy top-level section URLs (old WooCommerce site
   * served pages like /wheels): section slug -> full target path. Checked
   * before the menuCategories fallback in [section].astro, so a section can
   * point at a specific product instead of the category listing.
   */
  sectionRedirects: Record<string, string>;
  /**
   * Legacy WooCommerce /product/<slug>/ redirects: [oldSlug, targetPath] pairs
   * consulted before the API fallback. Covers old slugs whose products were
   * renamed or merged during migration (e.g. WPML translations). Arrays
   * replace wholesale per brand, so a brand with no legacy URLs sets [].
   */
  wooRedirects: [string, string][];
  home: {
    /** <title> of the homepage. */
    metaTitle: string;
    /** <meta name="description"> for the homepage. */
    metaDescription: string;
    /** Split hero (video left, headline + featured product right). */
    hero: {
      kicker: string;
      headline: string;
      ctaLabel: string;
      ctaHref: string;
    };
    /**
     * Full-width hero variant (headline + highlight span + colored CTA).
     * When non-null it replaces the split hero entirely.
     */
    wideHero: {
      before: string;
      highlight: string;
      after: string;
      ctaLabel: string;
      ctaHref: string;
      ctaColor: string;
    } | null;
    /** Anniversary/announcement panel under the hero; null hides it. */
    yearsPanel: { heading: string; body: string } | null;
    /** Photo paired with the years panel (old-site image-left/blue-right block). */
    panelImage: { src: string; alt: string } | null;
    /** Closing "about" section above the footer. */
    about: { heading: string; paragraphs: string[] };
    /** Whether to render the API-driven category tiles under the hero. */
    showCategoryTiles: boolean;
    /**
     * Category slug for the promotional bundle section on the homepage;
     * null disables the section.
     */
    promoCategory: string | null;
    /** Copy for the promotional bundle section; null disables the section. */
    promo: {
      eyebrow: string;
      title: string;
      body: string;
      ctaLabel: string;
      ctaHref: string;
    } | null;
  };
}

export type DeepPartial<T> = {
  [K in keyof T]?: T[K] extends Array<infer U>
    ? U[]
    : T[K] extends object
      ? DeepPartial<T[K]>
      : T[K];
};

/**
 * Deep-merges a brand override over the default content. Arrays and scalars
 * are replaced wholesale; plain objects are merged key-by-key; `undefined`
 * keeps the default.
 */
export function mergeContent<T>(base: T, override: DeepPartial<T> | undefined): T {
  if (override === undefined) return base;
  if (
    typeof base !== 'object' ||
    base === null ||
    typeof override !== 'object' ||
    override === null ||
    Array.isArray(base) ||
    Array.isArray(override)
  ) {
    return override as T;
  }
  const out: Record<string, unknown> = { ...(base as Record<string, unknown>) };
  for (const key of Object.keys(override)) {
    out[key] = mergeContent(
      (base as Record<string, unknown>)[key],
      (override as Record<string, unknown>)[key] as never,
    );
  }
  return out as T;
}
