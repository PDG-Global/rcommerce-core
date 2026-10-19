import { brand } from './brand';

export const API_BASE = brand.apiBase;

// Default Airwallex environment for the checkout Drop-in ('demo' when the
// backend runs against the Airwallex demo environment, 'prod' otherwise).
// The server-reported CheckoutSummary.airwallex_env takes precedence over this
// value, so this is only a fallback for servers that do not report it.
export const AIRWALLEX_ENV: string = import.meta.env.PUBLIC_AIRWALLEX_ENV ?? 'prod';

const REFRESH_BUFFER_MS = 60 * 1000; // Refresh 1 minute before expiry

export async function apiFetch(path: string, options?: RequestInit) {
  return fetch(`${API_BASE}${path}`, options);
}

export class ApiError extends Error {
  code?: string;

  constructor(
    message: string,
    public status: number,
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

export async function apiFetchJson<T = unknown>(path: string, options?: RequestInit): Promise<T> {
  const res = await apiFetch(path, options);
  if (!res.ok) {
    const bodyText = await res.text().catch(() => null);
    let err: { error?: { code?: string; message?: string } | string; message?: string } = {};
    if (bodyText) {
      try {
        const parsed = JSON.parse(bodyText);
        err = parsed;
      } catch {
        err = { error: bodyText };
      }
    }
    let message = `API error: ${res.status}`;
    if (typeof err.error === 'object' && err.error?.message) {
      message = err.error.message;
    } else if (typeof err.error === 'string') {
      message = err.error;
    } else if (err.message) {
      message = err.message;
    }
    const apiErr = new ApiError(message, res.status);
    (apiErr as any).code = typeof err.error === 'object' ? err.error?.code : undefined;
    throw apiErr;
  }
  const jsonText = await res.text();
  try {
    return JSON.parse(jsonText) as T;
  } catch (parseErr) {
    throw new Error(`Invalid JSON response: ${jsonText}`);
  }
}

export function getAuthToken(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem('rcommerce_token');
}

export function setAuthToken(token: string) {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem('rcommerce_token', token);
}

export function clearAuthToken() {
  if (typeof localStorage === 'undefined') return;
  localStorage.removeItem('rcommerce_token');
}

export function getRefreshToken(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem('rcommerce_refresh_token');
}

export function setRefreshToken(token: string) {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem('rcommerce_refresh_token', token);
}

export function clearRefreshToken() {
  if (typeof localStorage === 'undefined') return;
  localStorage.removeItem('rcommerce_refresh_token');
}

export function clearAuth() {
  clearAuthToken();
  clearRefreshToken();
  clearStoredCartId();
}

export function setStoredCartId(cartId: string) {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem('rcommerce_cart_id', cartId);
}

export function clearStoredCartId() {
  if (typeof localStorage === 'undefined') return;
  localStorage.removeItem('rcommerce_cart_id');
}

function tokenExpiryMs(token: string): number | null {
  try {
    const payload = JSON.parse(atob(token.split('.')[1]));
    if (!payload.exp) return null;
    return payload.exp * 1000;
  } catch {
    return null;
  }
}

let refreshPromise: Promise<string> | null = null;

export async function refreshAccessToken(): Promise<string> {
  if (refreshPromise) return refreshPromise;
  refreshPromise = (async () => {
    try {
      const refreshToken = getRefreshToken();
      if (!refreshToken) throw new ApiError('No refresh token', 401);
      const res = await apiFetchJson<{ access_token: string; expires_in: number }>('/auth/refresh', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ refresh_token: refreshToken }),
      });
      setAuthToken(res.access_token);
      scheduleTokenRefresh(res.access_token);
      return res.access_token;
    } finally {
      refreshPromise = null;
    }
  })();
  return refreshPromise;
}

let refreshTimeout: ReturnType<typeof setTimeout> | null = null;

export function scheduleTokenRefresh(token?: string | null) {
  if (typeof window === 'undefined') return;
  if (refreshTimeout) {
    clearTimeout(refreshTimeout);
    refreshTimeout = null;
  }
  const t = token || getAuthToken();
  if (!t) return;
  const exp = tokenExpiryMs(t);
  if (!exp) return;
  const now = Date.now();
  const delay = Math.max(0, exp - now - REFRESH_BUFFER_MS);
  refreshTimeout = setTimeout(() => {
    refreshAccessToken().catch((e) => {
      if (e instanceof ApiError && e.status === 401) {
        clearAuth();
        window.location.href = '/account/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search);
      }
    });
  }, delay);
}

export async function apiFetchAuthenticated<T = unknown>(
  path: string,
  options: RequestInit = {},
): Promise<T> {
  let token = getAuthToken();
  if (!token) {
    token = await refreshAccessToken();
  }
  if (!token) {
    if (typeof window !== 'undefined') {
      window.location.href = '/account/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search);
    }
    throw new ApiError('Unauthorized', 401);
  }

  const headers = new Headers(options.headers);
  headers.set('content-type', 'application/json');
  headers.set('authorization', `Bearer ${token}`);
  try {
    return await apiFetchJson<T>(path, { ...options, headers });
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) {
      try {
        const newToken = await refreshAccessToken();
        headers.set('authorization', `Bearer ${newToken}`);
        return await apiFetchJson<T>(path, { ...options, headers });
      } catch {
        clearAuth();
        if (typeof window !== 'undefined') {
          window.location.href = '/account/login?redirect=' + encodeURIComponent(window.location.pathname + window.location.search);
        }
      }
    }
    throw e;
  }
}
