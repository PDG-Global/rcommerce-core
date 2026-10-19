import type { APIRoute } from 'astro';

export const GET: APIRoute = async ({ request }) => {
  const origin = new URL(request.url).origin;
  const body = `User-agent: *
Allow: /
Disallow: /account/
Disallow: /cart
Disallow: /checkout
Disallow: /orders/
Disallow: /api/

Sitemap: ${origin}/sitemap.xml
`;
  return new Response(body, {
    headers: { 'content-type': 'text/plain; charset=utf-8' },
  });
};
