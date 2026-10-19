import { useEffect, useState } from 'react';
import type { Order } from '../lib/types';
import { apiFetchAuthenticated, getAuthToken, getRefreshToken, clearAuth } from '../lib/api';
import { formatDate } from '../lib/date';

export default function OrderHistory() {
  const [orders, setOrders] = useState<Order[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!getAuthToken() && !getRefreshToken()) {
      window.location.href = '/account';
      return;
    }
    apiFetchAuthenticated<{ orders: Order[] }>('/orders')
      .then((data) => setOrders(data.orders))
      .catch((e: any) => {
        if (e.status === 401 || e.status === 403) {
          clearAuth();
          window.location.href = '/account';
          return;
        }
        setError(e.message || 'Failed to load orders');
      })
      .finally(() => setLoading(false));
  }, []);

  if (loading) return <p className="text-rcommerce-600">Loading orders...</p>;
  if (error) {
    return <p className="text-rcommerce-danger">{error}</p>;
  }

  if (orders.length === 0) {
    return (
      <div className="py-6 text-center">
        <p className="text-rcommerce-600">You haven't placed any orders yet.</p>
        <a href="/products" className="mt-3 inline-block text-sm text-rcommerce-accent hover:underline">
          Start shopping →
        </a>
      </div>
    );
  }

  return (
    <div className="divide-y divide-rcommerce-200">
      {orders.map((order) => (
        <a
          key={order.id}
          href={`/orders/${order.id}`}
          className="flex items-center justify-between gap-4 py-4 group"
        >
          <div>
            <p className="font-semibold text-rcommerce-900 group-hover:text-rcommerce-accent transition-colors">
              {order.order_number}
            </p>
            <p className="text-sm text-rcommerce-600">{formatDate(order.created_at)}</p>
          </div>
          <div className="text-right">
            <p className="font-semibold text-rcommerce-900">{order.currency} {parseFloat(order.total).toFixed(2)}</p>
            <span className="inline-block mt-1 px-2 py-0.5 text-xs bg-rcommerce-100 text-rcommerce-700 capitalize">
              {order.status}
            </span>
          </div>
        </a>
      ))}
    </div>
  );
}
