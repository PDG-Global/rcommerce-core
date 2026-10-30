import { useCallback, useEffect, useState, type FormEvent } from 'react';
import { apiFetchAuthenticated, API_BASE, getAuthToken } from '../lib/api';
import { visibleItems } from '../lib/bundles';
import AdminGuard from './AdminGuard';
import {
  CANCELLABLE_STATUSES,
  clearCachedAdminStats,
  extractTracking,
  formatDateTime,
  formatMoney,
  fulfillmentBadgeClass,
  ORDER_STATUSES,
  parseOrderNotes,
  paymentBadgeClass,
  statusBadgeClass,
  type AdminAddress,
  type AdminOrderDetail,
} from '../lib/admin';
import { stripHtml } from '../lib/html';

type Feedback = { kind: 'success' | 'error'; message: string } | null;

function AddressBlock({ title, address }: { title: string; address: AdminAddress | null }) {
  return (
    <div>
      <h3 className="text-xs font-semibold uppercase tracking-button text-rcommerce-500 mb-2">{title}</h3>
      {address ? (
        <address className="not-italic text-sm text-rcommerce-700 leading-relaxed">
          <p className="font-semibold text-rcommerce-900">
            {address.first_name} {address.last_name}
          </p>
          {address.company && <p>{address.company}</p>}
          <p>{address.address1}</p>
          {address.address2 && <p>{address.address2}</p>}
          <p>
            {[address.city, address.state, address.zip].filter(Boolean).join(', ')}
          </p>
          <p>{address.country}</p>
          {address.phone && <p>{address.phone}</p>}
        </address>
      ) : (
        <p className="text-sm text-rcommerce-500">—</p>
      )}
    </div>
  );
}

export default function AdminOrderDetail({ orderId }: { orderId: string }) {
  return (
    <AdminGuard>
      <OrderDetailContent orderId={orderId} />
    </AdminGuard>
  );
}

function OrderDetailContent({ orderId }: { orderId: string }) {
  const [order, setOrder] = useState<AdminOrderDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<Feedback>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [statusDraft, setStatusDraft] = useState('');
  const [refundAmount, setRefundAmount] = useState('');
  const [refundReason, setRefundReason] = useState('');
  const [carrier, setCarrier] = useState('UPS');
  const [trackingNumber, setTrackingNumber] = useState('');

  const loadOrder = useCallback(() => {
    setLoading(true);
    setError(null);
    return apiFetchAuthenticated<AdminOrderDetail>(`/orders/${orderId}`)
      .then((data) => {
        setOrder(data);
        setStatusDraft(data.status);
      })
      .catch((e: any) => setError(e.message || 'Failed to load order'))
      .finally(() => setLoading(false));
  }, [orderId]);

  useEffect(() => {
    loadOrder();
  }, [loadOrder]);

  const runAction = (
    key: string,
    fn: () => Promise<unknown>,
    successMessage: string,
    onSuccess?: () => void,
  ) => {
    setBusy(key);
    setFeedback(null);
    fn()
      .then(() => {
        onSuccess?.();
        setFeedback({ kind: 'success', message: successMessage });
        clearCachedAdminStats();
        return loadOrder();
      })
      .catch((e: any) => {
        setFeedback({ kind: 'error', message: e.message || 'Action failed' });
      })
      .finally(() => setBusy(null));
  };

  const handleCancel = () => {
    if (!order) return;
    if (!window.confirm(`Cancel order ${order.order_number}? This releases the reserved inventory.`)) return;
    runAction(
      'cancel',
      () =>
        apiFetchAuthenticated(`/orders/${order.id}/cancel`, {
          method: 'POST',
          body: JSON.stringify({ reason: 'Cancelled via admin dashboard' }),
        }),
      'Order cancelled and inventory released.',
    );
  };

  const handleInvoice = () => {
    if (!order) return;
    const token = getAuthToken();
    fetch(`${API_BASE}/orders/${order.id}/invoice`, {
      headers: { Authorization: `Bearer ${token}` },
    })
      .then((res) => {
        if (!res.ok) throw new Error('Failed to load invoice');
        return res.text();
      })
      .then((html) => {
        const win = window.open('', '_blank');
        if (win) {
          win.document.write(html);
          win.document.close();
        }
      })
      .catch((e: any) =>
        setFeedback({ kind: 'error', message: e.message || 'Failed to load invoice' }),
      );
  };

  const handleRefund = () => {
    if (!order) return;
    const body: { amount?: string; reason?: string } = {};
    if (refundAmount.trim()) body.amount = refundAmount.trim();
    if (refundReason.trim()) body.reason = refundReason.trim();
    runAction(
      'refund',
      () =>
        apiFetchAuthenticated(`/orders/${order.id}/refund`, {
          method: 'POST',
          body: JSON.stringify(body),
        }),
      body.amount
        ? `Refund of ${formatMoney(order.currency, body.amount)} submitted.`
        : 'Full refund submitted.',
    );
  };

  const handleStatusSave = () => {
    if (!order || statusDraft === order.status) return;
    runAction(
      'status',
      () =>
        apiFetchAuthenticated(`/orders/${order.id}/status`, {
          method: 'PUT',
          body: JSON.stringify({ status: statusDraft }),
        }),
      `Order status updated to "${statusDraft.replace('_', ' ')}".`,
    );
  };

  const handleFulfill = (event: FormEvent) => {
    event.preventDefault();
    if (!order) return;
    const code = trackingNumber.trim();
    if (!code) return;
    if (!window.confirm(`Mark order ${order.order_number} as shipped via ${carrier} with tracking ${code}?`)) return;
    runAction(
      'fulfillment',
      () =>
        apiFetchAuthenticated(`/orders/${order.id}/fulfillments`, {
          method: 'POST',
          body: JSON.stringify({ carrier, tracking_number: code }),
        }),
      `Order marked as shipped via ${carrier}.`,
      () => setTrackingNumber(''),
    );
  };

  if (loading && !order) {
    return <p className="font-serif italic text-xl text-rcommerce-500">Loading order…</p>;
  }
  if (error && !order) {
    return (
      <div className="py-8">
        <p className="text-rcommerce-danger">{error}</p>
        <a href="/admin" className="btn-secondary mt-6">Back to Dashboard</a>
      </div>
    );
  }
  if (!order) return null;

  const canCancel = CANCELLABLE_STATUSES.includes(order.status);
  const canRefund = order.payment_status === 'paid' && order.status !== 'refunded';
  const fulfillmentStatus = order.fulfillment_status || 'pending';
  const fulfillments = order.fulfillments ?? [];
  const canFulfill =
    ['confirmed', 'processing'].includes(order.status) &&
    ['pending', 'processing'].includes(fulfillmentStatus);
  const noteLines = parseOrderNotes(order.notes);
  const lastNote = noteLines.length > 0 ? noteLines[noteLines.length - 1] : null;
  const tracking = fulfillments.length === 0 && lastNote ? extractTracking(lastNote.text) : null;
  const discountValue = parseFloat(String(order.discount_total ?? 0));
  const hasDiscount = Number.isFinite(discountValue) && discountValue > 0;

  return (
    <div className="space-y-8">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div>
          <a href="/admin" className="text-sm text-rcommerce-accent hover:underline">← Admin dashboard</a>
          <h1 className="font-serif text-4xl font-semibold text-rcommerce-900 mt-2">
            {order.order_number}
          </h1>
          <p className="text-sm text-rcommerce-600 mt-1">{formatDateTime(order.created_at)}</p>
        </div>
        <div className="flex gap-2 pt-2">
          <span className={`inline-block px-3 py-1 text-xs font-semibold uppercase tracking-wide ${statusBadgeClass(order.status)}`}>
            {order.status.replace('_', ' ')}
          </span>
          <span className={`inline-block px-3 py-1 text-xs font-semibold uppercase tracking-wide ${paymentBadgeClass(order.payment_status)}`}>
            {order.payment_status}
          </span>
        </div>
      </div>

      {feedback && (
        <div
          className={`border px-4 py-3 text-sm ${
            feedback.kind === 'success'
              ? 'border-emerald-300 bg-emerald-50 text-emerald-800'
              : 'border-rcommerce-danger bg-red-50 text-rcommerce-danger'
          }`}
          role="status"
        >
          {feedback.message}
        </div>
      )}
      {error && (
        <p className="border border-rcommerce-danger bg-red-50 text-rcommerce-danger px-4 py-3 text-sm">{error}</p>
      )}

      <section className="border border-rcommerce-200">
        <h2 className="px-4 sm:px-6 py-4 border-b border-rcommerce-200 text-lg font-semibold">Items</h2>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[560px] text-sm">
            <thead>
              <tr className="text-left text-xs uppercase tracking-button text-rcommerce-500 bg-rcommerce-100">
                <th className="px-4 sm:px-6 py-3 font-semibold">Item</th>
                <th className="px-4 py-3 font-semibold">SKU</th>
                <th className="px-4 py-3 font-semibold text-right">Qty</th>
                <th className="px-4 py-3 font-semibold text-right">Price</th>
                <th className="px-4 sm:px-6 py-3 font-semibold text-right">Total</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-rcommerce-200">
              {visibleItems(order.items).map((item) => (
                <tr key={item.id}>
                  <td className="px-4 sm:px-6 py-3 font-medium text-rcommerce-900">
                    {item.name}
                    {item.variant_title && (
                      <span className="block text-xs font-normal text-rcommerce-accent mt-0.5">{item.variant_title}</span>
                    )}
                  </td>
                  <td className="px-4 py-3 text-rcommerce-600">{item.sku ?? '—'}</td>
                  <td className="px-4 py-3 text-right">{item.quantity}</td>
                  <td className="px-4 py-3 text-right">{formatMoney(order.currency, item.price)}</td>
                  <td className="px-4 sm:px-6 py-3 text-right font-semibold">
                    {formatMoney(order.currency, item.total)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <div className="px-4 sm:px-6 py-4 border-t border-rcommerce-200 flex justify-end">
          <dl className="w-full max-w-xs space-y-1 text-sm">
            <div className="flex justify-between">
              <dt className="text-rcommerce-600">Subtotal</dt>
              <dd>{formatMoney(order.currency, order.subtotal)}</dd>
            </div>
            {hasDiscount && (
              <div className="flex justify-between">
                <dt className="text-rcommerce-600">
                  {order.coupon_code ? `Discount (${order.coupon_code})` : 'Discount'}
                </dt>
                <dd>{formatMoney(order.currency, -Math.abs(discountValue))}</dd>
              </div>
            )}
            <div className="flex justify-between">
              <dt className="text-rcommerce-600">Shipping</dt>
              <dd>{formatMoney(order.currency, order.shipping_total)}</dd>
            </div>
            <div className="flex justify-between">
              <dt className="text-rcommerce-600">Tax</dt>
              <dd>{formatMoney(order.currency, order.tax_total)}</dd>
            </div>
            <div className="flex justify-between pt-2 mt-2 border-t border-rcommerce-200 font-semibold text-base">
              <dt>Total</dt>
              <dd>{formatMoney(order.currency, order.total)}</dd>
            </div>
          </dl>
        </div>
      </section>

      <div className="grid md:grid-cols-2 gap-4">
        <section className="border border-rcommerce-200 p-4 sm:p-6 space-y-5">
          <h2 className="text-lg font-semibold">Customer</h2>
          <div className="text-sm">
            <p className="font-semibold text-rcommerce-900">{order.customer_email}</p>
            {order.customer_id && <p className="text-rcommerce-500 text-xs mt-1">{order.customer_id}</p>}
          </div>
          <AddressBlock title="Shipping Address" address={order.shipping_address} />
          <AddressBlock title="Billing Address" address={order.billing_address} />
        </section>

        <section className="border border-rcommerce-200 p-4 sm:p-6 space-y-5">
          <h2 className="text-lg font-semibold">Payments</h2>
          {order.payments.length === 0 ? (
            <p className="text-sm text-rcommerce-500">No payment records.</p>
          ) : (
            <ul className="space-y-3">
              {order.payments.map((payment) => (
                <li key={payment.id} className="text-sm border border-rcommerce-200 p-3">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-semibold">{formatMoney(payment.currency, payment.amount)}</span>
                    <span className={`inline-block px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide ${paymentBadgeClass(payment.status)}`}>
                      {payment.status}
                    </span>
                  </div>
                  <p className="text-rcommerce-600 mt-1 capitalize">
                    {payment.gateway}
                    {payment.card_brand && ` · ${payment.card_brand}`}
                    {payment.card_last_four && ` ··${payment.card_last_four}`}
                  </p>
                  {payment.processed_at && (
                    <p className="text-rcommerce-500 text-xs mt-1">{formatDateTime(payment.processed_at)}</p>
                  )}
                  {payment.refunds && payment.refunds.length > 0 && (
                    <ul className="mt-2 space-y-1 border-t border-rcommerce-200 pt-2">
                      {payment.refunds.map((refund) => (
                        <li key={refund.id} className="text-xs">
                          <div className="flex items-center justify-between gap-2">
                            <span className="font-semibold text-red-700">−{formatMoney(refund.currency, refund.amount)}</span>
                            <span className={`inline-block px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${paymentBadgeClass(refund.status)}`}>
                              {refund.status}
                            </span>
                          </div>
                          {refund.reason && <p className="text-rcommerce-600 mt-0.5">{refund.reason}</p>}
                          <p className="text-rcommerce-500 mt-0.5">
                            {formatDateTime(refund.settled_at ?? refund.created_at)}
                            {refund.gateway_refund_id && ` · ${refund.gateway_refund_id}`}
                          </p>
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>

      <section className="border border-rcommerce-200 p-4 sm:p-6 space-y-5">
        <div className="flex items-center justify-between gap-2">
          <h2 className="text-lg font-semibold">Fulfilment</h2>
          <span className={`inline-block px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide ${fulfillmentBadgeClass(fulfillmentStatus)}`}>
            {fulfillmentStatus.replace('_', ' ')}
          </span>
        </div>

        <div className="text-sm">
          {fulfillments.length > 0 ? (
            <ul className="space-y-3">
              {fulfillments.map((fulfillment) => (
                <li key={fulfillment.id} className="border border-rcommerce-200 p-3">
                  <p className="text-rcommerce-700">
                    <span className="font-semibold">{fulfillment.tracking_company ?? 'Carrier'}</span>{' '}
                    {fulfillment.tracking_url ? (
                      <a
                        href={fulfillment.tracking_url}
                        target="_blank"
                        rel="noreferrer"
                        className="text-rcommerce-accent hover:underline"
                      >
                        {fulfillment.tracking_number}
                      </a>
                    ) : (
                      <span>{fulfillment.tracking_number ?? 'No tracking number'}</span>
                    )}
                  </p>
                  {fulfillment.shipped_at && (
                    <p className="text-rcommerce-500 text-xs mt-1">
                      Shipped {formatDateTime(fulfillment.shipped_at)}
                    </p>
                  )}
                </li>
              ))}
            </ul>
          ) : tracking ? (
            <p className="text-rcommerce-700">
              Tracking: {tracking.carrier ?? 'Carrier'}{' '}
              <a
                href={tracking.url}
                target="_blank"
                rel="noreferrer"
                className="text-rcommerce-accent hover:underline"
              >
                {tracking.code}
              </a>
            </p>
          ) : (
            <p className="text-rcommerce-500">No tracking information yet.</p>
          )}
        </div>

        {canFulfill && (
          <form onSubmit={handleFulfill} className="border border-rcommerce-200 p-4 space-y-4">
            <h3 className="text-sm font-semibold">Manual fulfilment</h3>
            <div className="grid sm:grid-cols-2 gap-3">
              <div>
                <label htmlFor="fulfillment-carrier" className="block text-xs font-semibold uppercase tracking-button text-rcommerce-500 mb-2">
                  Carrier
                </label>
                <select
                  id="fulfillment-carrier"
                  value={carrier}
                  onChange={(event) => setCarrier(event.target.value)}
                  className="input"
                >
                  {['DHL', 'UPS', 'FedEx', 'SF Express'].map((option) => (
                    <option key={option} value={option}>{option}</option>
                  ))}
                </select>
              </div>
              <div>
                <label htmlFor="fulfillment-tracking" className="block text-xs font-semibold uppercase tracking-button text-rcommerce-500 mb-2">
                  Tracking number
                </label>
                <input
                  id="fulfillment-tracking"
                  type="text"
                  value={trackingNumber}
                  onChange={(event) => setTrackingNumber(event.target.value)}
                  maxLength={128}
                  pattern="[A-Za-z0-9-]+"
                  title="Letters, numbers, and hyphens only"
                  autoComplete="off"
                  required
                  className="input"
                />
              </div>
            </div>
            <button
              type="submit"
              disabled={busy !== null || !trackingNumber.trim()}
              className="btn-primary"
            >
              {busy === 'fulfillment' ? 'Shipping…' : 'Mark as shipped'}
            </button>
          </form>
        )}

        {noteLines.length > 0 && (
          <div className="space-y-2 pt-4 border-t border-rcommerce-200">
            <h3 className="text-xs font-semibold uppercase tracking-button text-rcommerce-500">Order notes</h3>
            <ul className="space-y-2 text-sm">
              {noteLines.map((line, i) => (
                <li key={i} className="text-rcommerce-700">
                  {line.at && (
                    <span className="text-rcommerce-500 text-xs mr-2">{formatDateTime(line.at)}</span>
                  )}
                  {stripHtml(line.text)}
                </li>
              ))}
            </ul>
          </div>
        )}
      </section>

      <section className="border border-rcommerce-200 p-4 sm:p-6 space-y-6">
        <h2 className="text-lg font-semibold">Actions</h2>

        <div className="flex flex-wrap items-end gap-3">
          <div>
            <label htmlFor="order-status" className="block text-xs font-semibold uppercase tracking-button text-rcommerce-500 mb-2">
              Order Status
            </label>
            <select
              id="order-status"
              value={statusDraft}
              onChange={(e) => setStatusDraft(e.target.value)}
              className="input w-auto min-w-[180px]"
            >
              {ORDER_STATUSES.map((status) => (
                <option key={status} value={status}>
                  {status.replace('_', ' ')}
                </option>
              ))}
            </select>
          </div>
          <button
            onClick={handleStatusSave}
            disabled={busy !== null || statusDraft === order.status}
            className="btn-primary"
          >
            {busy === 'status' ? 'Saving…' : 'Save Status'}
          </button>
        </div>

        <div className="flex flex-wrap gap-3 pt-4 border-t border-rcommerce-200">
          <button onClick={handleInvoice} className="btn-secondary">
            Download Invoice
          </button>
          <button
            onClick={handleCancel}
            disabled={busy !== null || !canCancel}
            className="btn-secondary"
            title={canCancel ? undefined : `Cannot cancel an order with status "${order.status}"`}
          >
            {busy === 'cancel' ? 'Cancelling…' : 'Cancel Order'}
          </button>
        </div>

        {canRefund && (
          <div className="pt-4 border-t border-rcommerce-200 space-y-3">
            <h3 className="text-sm font-semibold">Refund Payment</h3>
            <div className="grid sm:grid-cols-2 gap-3">
              <div>
                <label htmlFor="refund-amount" className="block text-xs font-semibold uppercase tracking-button text-rcommerce-500 mb-2">
                  Amount (optional)
                </label>
                <input
                  id="refund-amount"
                  type="text"
                  inputMode="decimal"
                  placeholder={`Full amount (${formatMoney(order.currency, order.total)})`}
                  value={refundAmount}
                  onChange={(e) => setRefundAmount(e.target.value)}
                  className="input"
                />
              </div>
              <div>
                <label htmlFor="refund-reason" className="block text-xs font-semibold uppercase tracking-button text-rcommerce-500 mb-2">
                  Reason (optional)
                </label>
                <input
                  id="refund-reason"
                  type="text"
                  placeholder="Reason for refund"
                  value={refundReason}
                  onChange={(e) => setRefundReason(e.target.value)}
                  className="input"
                />
              </div>
            </div>
            <button onClick={handleRefund} disabled={busy !== null} className="btn-primary">
              {busy === 'refund' ? 'Processing…' : 'Refund'}
            </button>
          </div>
        )}
      </section>
    </div>
  );
}
