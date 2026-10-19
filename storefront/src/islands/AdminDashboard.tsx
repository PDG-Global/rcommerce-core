import { useEffect, useState } from 'react';
import { apiFetchAuthenticated } from '../lib/api';
import AdminGuard from './AdminGuard';
import {
  fetchAdminStats,
  formatDate,
  formatMoney,
  getCachedAdminStats,
  paymentBadgeClass,
  statusBadgeClass,
  type AdminOrderListItem,
  type AdminStats,
} from '../lib/admin';

function StatCard({ label, value }: { label: string; value: string }) {
  return (
    <div className="border border-rcommerce-200 p-5">
      <p className="text-xs font-semibold uppercase tracking-button text-rcommerce-500">{label}</p>
      <p className="font-serif text-3xl font-semibold text-rcommerce-900 mt-2">{value}</p>
    </div>
  );
}

export default function AdminDashboard() {
  return (
    <AdminGuard>
      <DashboardContent />
    </AdminGuard>
  );
}

function DashboardContent() {
  const [stats, setStats] = useState<AdminStats | null>(getCachedAdminStats());
  const [orders, setOrders] = useState<AdminOrderListItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!stats) {
      fetchAdminStats().then(setStats).catch((e: any) => setError(e.message || 'Failed to load stats'));
    }
    apiFetchAuthenticated<{ orders: AdminOrderListItem[] }>('/orders')
      .then((data) => setOrders(data.orders))
      .catch((e: any) => setError(e.message || 'Failed to load orders'));
  }, []);

  return (
    <div className="space-y-10">
      <div>
        <p className="eyebrow text-xl">Store management</p>
        <h1 className="font-serif text-4xl font-semibold text-rcommerce-900 mt-1">Admin Dashboard</h1>
      </div>

      {error && <p className="text-rcommerce-danger">{error}</p>}

      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <StatCard label="Total Orders" value={stats ? String(stats.total_orders) : '—'} />
        <StatCard label="Pending Orders" value={stats ? String(stats.pending_orders) : '—'} />
        <StatCard label="Customers" value={stats ? String(stats.total_customers) : '—'} />
        <StatCard
          label="Total Revenue"
          value={stats ? formatMoney('USD', stats.total_revenue) : '—'}
        />
      </div>

      <section>
        <div className="flex items-baseline justify-between mb-4">
          <h2 className="text-xl font-semibold">Recent Orders</h2>
          <span className="text-xs uppercase tracking-button text-rcommerce-500">Last 50</span>
        </div>
        {orders === null ? (
          <p className="font-serif italic text-xl text-rcommerce-500">Loading orders…</p>
        ) : orders.length === 0 ? (
          <p className="text-rcommerce-600 py-6">No orders yet.</p>
        ) : (
          <div className="overflow-x-auto border border-rcommerce-200">
            <table className="w-full min-w-[720px] text-sm">
              <thead>
                <tr className="bg-rcommerce-100 text-left text-xs uppercase tracking-button text-rcommerce-500">
                  <th className="px-4 py-3 font-semibold">Order</th>
                  <th className="px-4 py-3 font-semibold">Date</th>
                  <th className="px-4 py-3 font-semibold">Customer</th>
                  <th className="px-4 py-3 font-semibold">Status</th>
                  <th className="px-4 py-3 font-semibold">Payment</th>
                  <th className="px-4 py-3 font-semibold text-right">Total</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-rcommerce-200">
                {orders.map((order) => (
                  <tr key={order.id} className="hover:bg-rcommerce-100/50 transition-colors">
                    <td className="px-4 py-3">
                      <a
                        href={`/admin/orders/${order.id}`}
                        className="font-semibold text-rcommerce-900 hover:text-rcommerce-accent transition-colors"
                      >
                        {order.order_number}
                      </a>
                    </td>
                    <td className="px-4 py-3 text-rcommerce-600 whitespace-nowrap">
                      {formatDate(order.created_at)}
                    </td>
                    <td className="px-4 py-3 text-rcommerce-600">{order.customer_email}</td>
                    <td className="px-4 py-3">
                      <span
                        className={`inline-block px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide ${statusBadgeClass(order.status)}`}
                      >
                        {order.status.replace('_', ' ')}
                      </span>
                    </td>
                    <td className="px-4 py-3">
                      <span
                        className={`inline-block px-2 py-0.5 text-[11px] font-semibold uppercase tracking-wide ${paymentBadgeClass(order.payment_status)}`}
                      >
                        {order.payment_status}
                      </span>
                    </td>
                    <td className="px-4 py-3 text-right font-semibold whitespace-nowrap">
                      {formatMoney(order.currency, order.total)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>
    </div>
  );
}
