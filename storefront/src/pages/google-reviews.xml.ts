import type { APIRoute } from 'astro';
import { API_BASE } from '../lib/api';
import { brand } from '../lib/brand';

/**
 * Google Merchant Center product reviews feed (Atom with the Google
 * namespace). <g:product_id> MUST match the <g:id> used in google-feed.xml
 * for the same product — that is how Google attaches reviews to products.
 * Reviews are flattened across all active products, newest first.
 */

interface Review {
  id: string;
  author_name: string;
  rating: number;
  content: string;
  created_at: string;
}

interface FlatReview {
  review: Review;
  productId: string;
}

const CACHE_TTL_MS = 60 * 60 * 1000; // Google fetches daily at most
let cache: { xml: string; fetchedAt: number } | null = null;

function xmlEscape(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

export const GET: APIRoute = async ({ request }) => {
  const base = new URL(request.url).origin;

  if (cache && Date.now() - cache.fetchedAt < CACHE_TTL_MS) {
    return new Response(cache.xml, {
      headers: { 'Content-Type': 'application/xml; charset=utf-8', 'Cache-Control': 'public, max-age=3600' },
    });
  }

  const all: FlatReview[] = [];

  try {
    const res = await fetch(`${API_BASE}/products`);
    if (res.ok) {
      const data = await res.json();
      const products = ((data.products || []) as { id: string; sku: string | null; is_active: boolean; slug: string }[])
        .filter((p) => p.is_active && p.slug);
      // Sequential fetches: ~29 products, and this runs at most once per hour.
      for (const p of products) {
        try {
          const r = await fetch(`${API_BASE}/products/${p.id}/reviews`);
          if (!r.ok) continue;
          const summary = await r.json();
          for (const review of (summary.reviews || []) as Review[]) {
            // Same identifier as google-feed.xml's <g:id> (SKU, else product id).
            all.push({ review, productId: p.sku || p.id });
          }
        } catch {
          // skip this product's reviews
        }
      }
    }
  } catch {
    // serve whatever we have (possibly none) as a valid empty feed
  }

  all.sort((a, b) => Date.parse(b.review.created_at) - Date.parse(a.review.created_at));

  const entries = all
    .map(
      ({ review, productId }) => `  <entry>
    <g:review_id>${xmlEscape(review.id)}</g:review_id>
    <g:product_id>${xmlEscape(productId)}</g:product_id>
    <g:reviewer_name>${xmlEscape(review.author_name)}</g:reviewer_name>
    <g:review_timestamp>${xmlEscape(review.created_at)}</g:review_timestamp>
    <g:review_type>product_review</g:review_type>
    <g:content>${xmlEscape(review.content)}</g:content>
    <g:rating>${Math.round(review.rating)}</g:rating>
  </entry>`,
    )
    .join('\n');

  const xml = `<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:g="http://base.google.com/ns/1.0">
  <title>${xmlEscape(`${brand.name} Product Reviews`)}</title>
  <link rel="self" href="${xmlEscape(`${base}/google-reviews.xml`)}"/>
  <updated>${new Date().toISOString()}</updated>
${entries}
</feed>`;

  cache = { xml, fetchedAt: Date.now() };

  return new Response(xml, {
    headers: { 'Content-Type': 'application/xml; charset=utf-8', 'Cache-Control': 'public, max-age=3600' },
  });
};
