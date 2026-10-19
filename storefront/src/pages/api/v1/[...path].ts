import type { APIRoute } from 'astro';

export const ALL: APIRoute = async ({ request, params, locals }) => {
  const path = params.path || '';
  const url = new URL(request.url);
  const search = url.search;

  const runtime = locals.runtime;
  const apiUrl = runtime?.env?.RCOMMERCE_API_URL || 'http://localhost:8080';
  const apiKey = runtime?.env?.RCOMMERCE_API_KEY;

  const targetUrl = `${apiUrl}/api/v1/${path}${search}`;

  const headers = new Headers();
  const contentType = request.headers.get('content-type');
  if (contentType) {
    headers.set('content-type', contentType);
  }

  const authHeader = request.headers.get('authorization');
  if (authHeader) {
    headers.set('authorization', authHeader);
  } else if (apiKey) {
    headers.set('authorization', `Bearer ${apiKey}`);
  }

  const body = request.body;

  let response: Response;
  try {
    response = await fetch(targetUrl, {
      method: request.method,
      headers,
      body: body ? body : undefined,
    });
  } catch (err) {
    return new Response(JSON.stringify({ error: 'Upstream unavailable' }), {
      status: 502,
      headers: { 'content-type': 'application/json' },
    });
  }

  const responseHeaders = new Headers();
  const responseContentType = response.headers.get('content-type');
  if (responseContentType) {
    responseHeaders.set('content-type', responseContentType);
  }
  const setCookies = response.headers.getSetCookie?.() || [];
  for (const cookie of setCookies) {
    responseHeaders.append('set-cookie', cookie);
  }
  const cacheControl = response.headers.get('cache-control');
  if (cacheControl) {
    responseHeaders.set('cache-control', cacheControl);
  }

  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: responseHeaders,
  });
};
