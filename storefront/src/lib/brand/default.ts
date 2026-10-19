import type { BrandContent } from './content';

/**
 * Generic rCommerce demo store — the default brand content for the public
 * storefront template. Every field of BrandContent is present here; real
 * stores ship a brand overlay (e.g. src/lib/brand/private/<slug>.ts) that
 * deep-merges over these values.
 */
export const defaultContent: BrandContent = {
  tagline: 'A modern headless commerce storefront. Fast, flexible, and ready to sell.',

  contact: {
    email: 'hello@example.com',
    links: [],
    intro: [
      `Questions about an order, a product, or a partnership? Send us an email and we'll get back to you as soon as we can.`,
    ],
    heroImage: '',
    emailCtaLabel: 'Email Us ⟶',
    faqs: [],
  },

  navLinks: [
    { label: 'About Us', href: '/about-us' },
    { label: 'Support', href: '/customer-support' },
  ],

  catalog: {
    line: 'products',
    categoryLine: 'our collection',
    photoAlt: 'products',
    categoryOrder: [],
  },

  footer: {
    blurb: 'Powered by rCommerce — a fast, headless storefront built for modern commerce.',
    supportLinks: [
      { label: 'About Us', href: '/about-us' },
      { label: 'Customer Support', href: '/customer-support' },
      { label: 'Contact Us', href: '/contact-us' },
      { label: 'My Account', href: '/account' },
    ],
    productColumns: [],
    socialLinks: [],
  },

  checkoutSupportEmail: 'hello@example.com',

  // Generic defaults — real stores override in their brand overlay. Rates
  // mirror the shared weight-based carrier zones (one-unit prices).
  seo: {
    returnDays: 30,
    returnFees: 'https://schema.org/ReturnFeesCustomerResponsibility',
    shipping: [
      { country: 'US', price: '10' },
      { country: 'CA', price: '5' },
      { country: 'GB', price: '3.5' },
      { country: 'AU', price: '3.5' },
      { country: 'JP', price: '3.5' },
      { country: 'HK', price: '2.5' },
      { country: 'SG', price: '2.5' },
      { country: 'CN', price: '0' },
    ],
  },

  pages: { about: true, support: true },

  productRedirects: {},
  sectionRedirects: {},

  // No legacy platform redirects in the generic template.
  wooRedirects: [],

  home: {
    metaTitle: 'Shop the rCommerce Store',
    metaDescription: 'Shop the rCommerce Store — a modern demo storefront for the rCommerce headless commerce platform.',
    hero: {
      kicker: 'Welcome to the',
      headline: 'rCommerce Store',
      ctaLabel: 'Shop Now ⟶',
      ctaHref: '/products',
    },
    wideHero: null,
    yearsPanel: null,
    panelImage: null,
    about: {
      heading: 'About the rCommerce Store',
      paragraphs: [
        `This is a demo storefront running on rCommerce, a high-performance headless commerce platform. Browse the catalog, manage a cart, and check out end to end.`,
        `Every store built on rCommerce gets its own branding, catalog, and content — everything you see here is data-driven.`,
      ],
    },
    showCategoryTiles: true,
    promoCategory: null,
    promo: null,
  },

  // Categories hidden from the sitemap / category navigation.
  hiddenCategories: ['uncategorized'],
};
