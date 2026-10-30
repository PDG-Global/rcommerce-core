import type { Cart } from './types';

import { API_BASE, apiFetchAuthenticated, apiFetchOptionalAuth, getAuthToken } from './api';

const CART_KEY = 'rcommerce_cart_id';
const SESSION_KEY = 'rcommerce_session_token';

export function getStoredCartId(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem(CART_KEY);
}

export function setStoredCartId(cartId: string) {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem(CART_KEY, cartId);
}

export function clearStoredCartId() {
  if (typeof localStorage === 'undefined') return;
  localStorage.removeItem(CART_KEY);
}

export function getStoredSessionToken(): string | null {
  if (typeof localStorage === 'undefined') return null;
  return localStorage.getItem(SESSION_KEY);
}

export function setStoredSessionToken(token: string) {
  if (typeof localStorage === 'undefined') return;
  localStorage.setItem(SESSION_KEY, token);
}

export function clearStoredSessionToken() {
  if (typeof localStorage === 'undefined') return;
  localStorage.removeItem(SESSION_KEY);
}

function normalizeCart(data: any): Cart {
  if (data && typeof data === 'object' && 'cart' in data && 'items' in data) {
    return { ...data.cart, items: data.items };
  }
  return data;
}

export function formatMoney(value: string | number | null | undefined): string {
  if (value == null) return '0.00';
  const num = typeof value === 'string' ? parseFloat(value) : value;
  if (Number.isNaN(num)) return '0.00';
  return num.toFixed(2);
}

// Concurrent getOrCreateCart() callers (e.g. several islands hydrating on
// load) must share one creation request, otherwise each creates its own
// guest cart and the last localStorage write wins, stranding items.
let creatingCart: Promise<Cart> | null = null;

export async function getOrCreateCart(currency?: string): Promise<Cart> {
  const existingId = getStoredCartId();
  if (existingId) {
    try {
      const data = await apiFetchOptionalAuth(`/carts/${existingId}`);
      return normalizeCart(data);
    } catch {
      // ignore, create new cart
    }
  }
  if (creatingCart) return creatingCart;
  creatingCart = (async () => {
    const res = await fetch(`${API_BASE}/carts/guest`, {
      method: 'POST',
      headers: currency ? { 'content-type': 'application/json' } : undefined,
      body: currency ? JSON.stringify({ currency }) : undefined,
    });
    if (!res.ok) {
      throw new Error('Failed to create cart');
    }
    const data = await res.json();
    const cart: Cart = normalizeCart(data);
    setStoredCartId(cart.id);
    if (cart.session_token) {
      setStoredSessionToken(cart.session_token);
    }
    return cart;
  })();
  try {
    return await creatingCart;
  } finally {
    creatingCart = null;
  }
}

export async function mergeGuestCartOnLogin(): Promise<void> {
  const sessionToken = getStoredSessionToken();
  if (!sessionToken || !getAuthToken()) return;
  try {
    const data = await apiFetchAuthenticated<Cart>('/carts/merge', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ session_token: sessionToken }),
    });
    const normalized = normalizeCart(data);
    setStoredCartId(normalized.id);
    clearStoredSessionToken();
    window.dispatchEvent(new CustomEvent('cart-updated'));
  } catch (e: any) {
    // Non-fatal: cart may already be linked or no guest cart exists
    console.warn('Failed to merge guest cart:', e.message);
  }
}

export async function addToCart(
  cartId: string,
  productId: string,
  quantity: number,
  variantId?: string,
) {
  const data = await apiFetchOptionalAuth<any>(`/carts/${cartId}/items`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ product_id: productId, variant_id: variantId || null, quantity }),
  });
  window.dispatchEvent(new CustomEvent('cart-updated'));
  return data;
}

export async function updateCartItem(cartId: string, itemId: string, quantity: number) {
  const data = await apiFetchOptionalAuth<any>(`/carts/${cartId}/items/${itemId}`, {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ quantity }),
  });
  window.dispatchEvent(new CustomEvent('cart-updated'));
  return data;
}

export async function removeCartItem(cartId: string, itemId: string) {
  await apiFetchOptionalAuth(`/carts/${cartId}/items/${itemId}`, { method: 'DELETE' });
  window.dispatchEvent(new CustomEvent('cart-updated'));
}

export async function applyCoupon(cartId: string, couponCode: string) {
  const data = await apiFetchOptionalAuth<any>(`/carts/${cartId}/coupon`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ coupon_code: couponCode }),
  });
  window.dispatchEvent(new CustomEvent('cart-updated'));
  return data;
}

export function openCart() {
  window.dispatchEvent(new CustomEvent('open-cart'));
}
