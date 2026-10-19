import { useEffect, useState } from 'react';
import type { Customer, Address } from '../lib/types';
import AddressManager from './AddressManager';
import OrderHistory from './OrderHistory';
import LoginForm from './LoginForm';
import { apiFetchAuthenticated, clearAuth, getAuthToken, getRefreshToken } from '../lib/api';

export default function AccountDashboard() {
  const [customer, setCustomer] = useState<Customer | null>(null);
  const [addresses, setAddresses] = useState<Address[]>([]);
  const [loading, setLoading] = useState(true);
  const [unauthenticated, setUnauthenticated] = useState(false);

  const fetchProfile = () => {
    if (!getAuthToken() && !getRefreshToken()) {
      setUnauthenticated(true);
      setLoading(false);
      return;
    }
    setLoading(true);
    apiFetchAuthenticated<{ customer: Customer; addresses: Address[] }>('/customers/me')
      .then((data) => {
        setCustomer({ ...data.customer, addresses: data.addresses });
        setAddresses(data.addresses);
      })
      .catch((e: any) => {
        if (e.status === 401 || e.status === 403) {
          clearAuth();
          setUnauthenticated(true);
        }
      })
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    fetchProfile();
  }, []);

  if (loading) {
    return <p className="font-serif italic text-xl text-rcommerce-500">Loading your account…</p>;
  }

  if (unauthenticated || !customer) {
    return (
      <div className="max-w-md mx-auto py-8">
        <div className="text-center mb-8">
          <p className="eyebrow text-xl">Welcome back</p>
          <h2 className="font-serif text-3xl font-semibold text-rcommerce-900 mt-1">Sign in to your account</h2>
          <p className="mt-2 text-sm text-rcommerce-600">
            New here?{' '}
            <a href="/account/register" className="text-rcommerce-accent hover:underline">Create an account</a>
          </p>
        </div>
        <LoginForm redirectTo="/account" />
      </div>
    );
  }

  return (
    <div className="space-y-8">
      <div>
        <p className="eyebrow text-xl">Welcome back,</p>
        <h1 className="font-serif text-4xl font-semibold text-rcommerce-900 mt-1">
          {customer.first_name || customer.email}
        </h1>
      </div>

      <section className="border border-rcommerce-200 p-6">
        <div className="flex items-center justify-between">
          <div>
            <h2 className="text-xl font-semibold">Profile</h2>
            <p className="text-rcommerce-600 mt-1">
              {customer.first_name} {customer.last_name} — {customer.email}
            </p>
          </div>
          <button
            onClick={() => {
              clearAuth();
              window.location.href = '/account/logout';
            }}
            className="btn-secondary"
          >
            Sign Out
          </button>
        </div>
      </section>

      <section className="border border-rcommerce-200 p-6">
        <h2 className="text-xl font-semibold mb-4">Orders</h2>
        <OrderHistory />
      </section>

      <section className="border border-rcommerce-200 p-6">
        <h2 className="text-xl font-semibold mb-4">Addresses</h2>
        <AddressManager addresses={addresses} onUpdated={fetchProfile} />
      </section>
    </div>
  );
}
