import type { APIRoute } from 'astro';
import { API_BASE } from '../lib/api';
import { productUrl } from '../lib/url';
import { stripHtml, stripHtmlTruncated } from '../lib/html';
import { brand, content } from '../lib/brand';

interface FeedProduct {
  id: string;
  title: string;
  slug: string;
  description: string | null;
  seo_description: string | null;
  price: string;
  currency: string;
  sku: string | null;
  inventory_quantity: number;
  is_active: boolean;
  images: { src: string; alt_text: string | null }[];
  image?: string | null;
  categories: { id: string; name: string; slug: string }[];
}

const BRAND = brand.name;

function xmlEscape(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

function itemField(tag: string, value: string | null | undefined): string {
  if (!value) return '';
  return `\n      <g:${tag}>${xmlEscape(value)}</g:${tag}>`;
}

export const GET: APIRoute = async ({ request }) => {
  const base = new URL(request.url).origin;

  let products: FeedProduct[] = [];

  try {
    const res = await fetch(`${API_BASE}/products`);
    if (res.ok) {
      const data = await res.json();
      products = (data.products || []).filter((p: FeedProduct) => p.is_active && p.slug);
    }
  } catch {
    // serve an empty-but-valid feed
  }

  const items = products
    .map((p) => {
      // Title as plain text; avoid duplicating the brand at the front
      let title = stripHtml(p.title) || p.slug;
      if (!title.toLowerCase().startsWith(BRAND.toLowerCase())) {
        title = `${BRAND} ${title}`;
      }

      // API images are hosted on the API origin under /uploads/... which is a
      // valid absolute image_link for Merchant Center.
      const imageLink = p.images?.[0]?.src || p.image || null;

      const availability = p.inventory_quantity > 0 ? 'in stock' : 'out of stock';

      const fields = [
        itemField('id', p.sku || p.id),
        itemField('title', title),
        itemField('description', stripHtmlTruncated(p.description || p.seo_description, 5000)),
        itemField('link', `${base}${productUrl(p)}`),
        itemField('image_link', imageLink),
        itemField('availability', availability),
        itemField('price', `${p.price} ${p.currency}`),
        itemField('condition', 'new'),
        itemField('brand', BRAND),
        itemField('mpn', p.sku),
        itemField(
          'product_type',
          p.categories?.map((c) => c.name).join(' > ') || undefined,
        ),
      ]
        .filter(Boolean)
        .join('');

      if (!fields.trim()) return '';
      return `    <item>${fields}
    </item>`;
    })
    .filter(Boolean)
    .join('\n');

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:g="http://base.google.com/ns/1.0">
  <channel>
    <title>${xmlEscape(`${BRAND} Product Feed`)}</title>
    <link>${xmlEscape(base)}</link>
    <description>${xmlEscape(`${BRAND} ${content.catalog.line}.`)}</description>
${items}
  </channel>
</rss>`;

  return new Response(xml, {
    headers: {
      'Content-Type': 'application/xml; charset=utf-8',
      'Cache-Control': 'public, max-age=300',
    },
  });
};
