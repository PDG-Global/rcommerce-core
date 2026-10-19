import { apiFetchAuthenticated, getAuthToken } from './api';
import { stripHtml } from './html';

// NOTE: the API wraps the stats payload in a "stats" key:
// GET /admin/stats -> { status: "ok", stats: { total_orders, ... } }
export interface AdminStats {
  total_orders: number;
  total_customers: number;
  total_revenue: number | string;
  pending_orders: number;
}

export interface AdminOrderItem {
  id: string;
  product_id: string;
  variant_id: string | null;
  name: string;
  variant_title?: string | null;
  sku: string | null;
  quantity: number;
  price: string;
  total: string;
  /** Bundle lines: components are $0 child lines hidden from item displays. */
  is_bundle_component?: boolean;
  is_bundle_parent?: boolean;
}

export interface AdminRefund {
  id: string;
  amount: string;
  currency: string;
  status: string;
  reason: string | null;
  gateway_refund_id: string | null;
  created_at: string;
  settled_at: string | null;
}

export interface AdminPayment {
  id: string;
  amount: string;
  currency: string;
  status: string;
  gateway: string;
  card_brand: string | null;
  card_last_four: string | null;
  processed_at: string | null;
  refunds?: AdminRefund[];
}

// shipping_address / billing_address are stored as JSON snapshots
export interface AdminAddress {
  first_name?: string;
  last_name?: string;
  company?: string | null;
  phone?: string | null;
  address1?: string;
  address2?: string | null;
  city?: string;
  state?: string | null;
  country?: string;
  zip?: string;
}

export interface AdminOrderListItem {
  id: string;
  order_number: string;
  customer_id: string | null;
  customer_email: string;
  status: string;
  payment_status: string;
  fulfillment_status: string;
  currency: string;
  subtotal: string;
  tax_total: string;
  shipping_total: string;
  /** Coupon discount (money). Absent (or 0) when no coupon was applied. */
  discount_total?: number | string;
  /** Applied coupon code, present only when a discount was applied. */
  coupon_code?: string | null;
  total: string;
  created_at: string;
}

export interface AdminOrderDetail extends AdminOrderListItem {
  items: AdminOrderItem[];
  payments: AdminPayment[];
  billing_address: AdminAddress | null;
  shipping_address: AdminAddress | null;
  notes?: string | null;
}

// Statuses accepted by PUT /orders/:id/status (crates/rcommerce-api/src/routes/order.rs)
export const ORDER_STATUSES = [
  'pending',
  'confirmed',
  'processing',
  'on_hold',
  'completed',
  'cancelled',
  'refunded',
] as const;

// Statuses for which the Cancel action is offered
export const CANCELLABLE_STATUSES = ['pending', 'confirmed', 'processing'];

export function formatMoney(currency: string, amount: number | string | null | undefined): string {
  const value = amount == null ? 0 : parseFloat(String(amount));
  return `${currency} ${Number.isFinite(value) ? value.toFixed(2) : String(amount)}`;
}

export function formatDate(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
}

export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return `${formatDate(iso)}, ${d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' })}`;
}

export function statusBadgeClass(status: string): string {
  switch (status) {
    case 'confirmed':
    case 'processing':
      return 'bg-blue-100 text-blue-800';
    case 'completed':
      return 'bg-emerald-100 text-emerald-800';
    case 'cancelled':
      return 'bg-rcommerce-200 text-rcommerce-700';
    case 'refunded':
      return 'bg-red-100 text-red-800';
    case 'on_hold':
      return 'bg-amber-100 text-amber-800';
    default:
      return 'bg-rcommerce-100 text-rcommerce-700';
  }
}

export function paymentBadgeClass(status: string): string {
  switch (status) {
    case 'paid':
      return 'bg-emerald-100 text-emerald-800';
    case 'authorized':
      return 'bg-blue-100 text-blue-800';
    case 'failed':
    case 'refunded':
      return 'bg-red-100 text-red-800';
    default:
      return 'bg-rcommerce-100 text-rcommerce-700';
  }
}

export function fulfillmentBadgeClass(status: string): string {
  switch (status) {
    case 'shipped':
    case 'delivered':
      return 'bg-emerald-100 text-emerald-800';
    case 'processing':
    case 'partial':
      return 'bg-amber-100 text-amber-800';
    case 'cancelled':
    case 'returned':
      return 'bg-red-100 text-red-800';
    default:
      return 'bg-rcommerce-100 text-rcommerce-700';
  }
}

export interface OrderNoteLine {
  at: string | null;
  text: string;
}

/// Splits an order's `notes` blob into `[<ISO timestamp>] <text>` lines.
export function parseOrderNotes(notes: string | null | undefined): OrderNoteLine[] {
  if (!notes) return [];
  return notes
    .split('\n')
    .map((raw) => {
      const m = raw.match(/^\[(.*?)\]\s*(.*)$/);
      return m ? { at: m[1] || null, text: m[2] ?? '' } : { at: null, text: raw };
    })
    .filter((line) => line.at !== null || line.text.trim().length > 0);
}

export interface TrackingInfo {
  code: string;
  url: string;
  carrier: string | null;
}

/// Extracts a tracking number (and optional carrier) from a single note line,
/// e.g. Dianxiaomi shipment notes: "shipped by yunexpress ... <span>YT2628...</span>
/// ... href=\"https://t.17track.net/#nums=YT2628...\"". Prefers the 17track
/// `#nums=` URL parameter, falls back to the longest alphanumeric run (≥ 8
/// chars) in the HTML-stripped text.
export function extractTracking(noteText: string): TrackingInfo | null {
  const trimmed = noteText.trim();
  if (!trimmed) return null;

  const plain = stripHtml(trimmed);
  const urlMatch = trimmed.match(/t\.17track\.net[^\s"'<>]*[#&]nums=([A-Za-z0-9]+)/i);
  const code = urlMatch?.[1] ?? longestAlphanumericRun(plain);
  if (!code) return null;

  const url = urlMatch
    ? urlMatch[0].startsWith('http')
      ? urlMatch[0]
      : `https://${urlMatch[0]}`
    : `https://t.17track.net/#nums=${code}`;

  const carrierMatch = plain.match(/shipped\s+by\s+([A-Za-z][A-Za-z0-9]*)/i);
  const carrier = carrierMatch?.[1]
    ? carrierMatch[1][0].toUpperCase() + carrierMatch[1].slice(1)
    : null;

  return { code, url, carrier };
}

function longestAlphanumericRun(text: string): string | null {
  // Tracking codes always contain at least one digit; requiring one avoids
  // false positives from ordinary words like "customer" (≥ 8 letters).
  const runs = text.match(/[A-Za-z0-9]{8,}/g)?.filter((run) => /\d/.test(run)) ?? [];
  return runs.sort((a, b) => b.length - a.length)[0] ?? null;
}

function decodeTokenPayload(token: string): { permissions?: unknown } | null {
  try {
    const payload = JSON.parse(atob(token.split('.')[1]));
    return payload && typeof payload === 'object' ? payload : null;
  } catch {
    return null;
  }
}

// The JWT payload carries a permissions array; the admin role includes "admin"
// (crates/rcommerce-core/src/services/auth_service.rs).
export function tokenHasAdminPermission(token: string): boolean {
  const payload = decodeTokenPayload(token);
  return Array.isArray(payload?.permissions) && payload.permissions.includes('admin');
}

export function getAdminToken(): string | null {
  const token = getAuthToken();
  return token && tokenHasAdminPermission(token) ? token : null;
}

let statsCache: AdminStats | null = null;
let statsPromise: Promise<AdminStats> | null = null;

export function getCachedAdminStats(): AdminStats | null {
  return statsCache;
}

export function clearCachedAdminStats() {
  statsCache = null;
}

// Dedupes concurrent calls (guard + dashboard mount together).
export function fetchAdminStats(): Promise<AdminStats> {
  if (!statsPromise) {
    statsPromise = apiFetchAuthenticated<{ stats: AdminStats }>('/admin/stats')
      .then((res) => {
        statsCache = res.stats;
        return res.stats;
      })
      .finally(() => {
        statsPromise = null;
      });
  }
  return statsPromise;
}
