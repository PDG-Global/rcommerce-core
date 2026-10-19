import type { APIRoute } from 'astro';
import { API_BASE } from '../lib/api';
import { brand, content } from '../lib/brand';
import { pageOverlay } from '../lib/brand/pageOverlays';

export const GET: APIRoute = async ({ request }) => {
  const base = new URL(request.url).origin;

  let products: { slug: string; created_at?: string; categories?: { slug: string }[] }[] = [];
  let categories: { slug: string }[] = [];

  try {
    const [productsRes, categoriesRes] = await Promise.all([
      fetch(`${API_BASE}/products`),
      fetch(`${API_BASE}/categories`),
    ]);
    if (productsRes.ok) {
      const data = await productsRes.json();
      const seen = new Set<string>();
      products = (data.products || [])
        .filter((p: any) => p.is_active && p.slug && !seen.has(p.slug) && seen.add(p.slug))
        .map((p: any) => ({ slug: p.slug, created_at: p.created_at, categories: p.categories }));
      // Paginate remaining pages (default per_page only returns page 1).
      const totalPages = data.meta?.total_pages ?? 1;
      for (let page = 2; page <= totalPages; page++) {
        const res = await fetch(`${API_BASE}/products?page=${page}`);
        if (!res.ok) break;
        const d = await res.json();
        for (const p of d.products || []) {
          if (p.is_active && p.slug && !seen.has(p.slug)) {
            seen.add(p.slug);
            products.push({ slug: p.slug, created_at: p.created_at, categories: p.categories });
          }
        }
      }
    }
    if (categoriesRes.ok) {
      const data = await categoriesRes.json();
      categories = (data.categories || [])
        .filter((c: any) => !content.hiddenCategories.includes(c.slug))
        .map((c: any) => ({ slug: c.slug }));
    }
  } catch {
    // serve what we have
  }

  const staticUrls = [
    { loc: '/', changefreq: 'daily', priority: '1.0' },
    { loc: '/products', changefreq: 'daily', priority: '0.9' },
    { loc: '/about-us', changefreq: 'monthly', priority: '0.5' },
    { loc: '/contact-us', changefreq: 'monthly', priority: '0.5' },
    { loc: '/customer-support', changefreq: 'monthly', priority: '0.5' },
    // Stores with bespoke page overlays (see src/lib/brand/private/) also
    // serve these short routes; include each one only when its overlay
    // exists. (/about content lives at /about-us for every store.)
    ...(['team', 'contact'] as const)
      .filter((page) => pageOverlay(`${brand.slug}-${page}`))
      .map((page) => ({ loc: `/${page}`, changefreq: 'monthly', priority: '0.5' })),
  ];

  const lastmod = (iso?: string): string => {
    if (!iso) return '';
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return '';
    return `<lastmod>${d.toISOString().split('T')[0]}</lastmod>`;
  };

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
${staticUrls
  .map(
    (u) => `  <url>
    <loc>${base}${u.loc}</loc>
    <changefreq>${u.changefreq}</changefreq>
    <priority>${u.priority}</priority>
  </url>`,
  )
  .join('\n')}
${categories
  .map(
    (c) => `  <url>
    <loc>${base}/products/${c.slug}</loc>
    <changefreq>daily</changefreq>
    <priority>0.8</priority>
  </url>`,
  )
  .join('\n')}
${products
  .map((p) => {
    const category = p.categories?.[0]?.slug;
    const path = category ? `/products/${category}/${p.slug}` : `/products/${p.slug}`;
    const lm = lastmod(p.created_at);
    return `  <url>
    <loc>${base}${path}</loc>${lm ? `\n    ${lm}` : ''}
    <changefreq>weekly</changefreq>
    <priority>0.7</priority>
  </url>`;
  })
  .join('\n')}
</urlset>`;

  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml' },
  });
};
