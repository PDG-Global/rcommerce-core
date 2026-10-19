import { useEffect, useState } from 'react';
import type { Customer } from '../lib/types';
import { apiFetchAuthenticated, clearAuth, scheduleTokenRefresh } from '../lib/api';

export function useAuth() {
  const [customer, setCustomer] = useState<Customer | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    apiFetchAuthenticated<{ customer: Customer; addresses: any[] }>('/customers/me')
      .then((data) => {
        if (!cancelled) setCustomer({ ...data.customer, addresses: data.addresses });
      })
      .catch(() => {
        if (!cancelled) setCustomer(null);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    scheduleTokenRefresh();
    return () => {
      cancelled = true;
    };
  }, []);

  const logout = () => {
    clearAuth();
    window.location.href = '/account/logout';
  };

  return { customer, loading, isAuthenticated: !!customer, logout };
}
