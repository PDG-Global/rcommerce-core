import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { getAuthToken, getRefreshToken } from '../lib/api';
import { fetchAdminStats, tokenHasAdminPermission } from '../lib/admin';

type GuardState = 'loading' | 'ok' | 'unauthenticated' | 'forbidden' | 'error';

// Internal gate used by the admin islands (AdminDashboard / AdminOrderDetail).
// Not used as an Astro island directly: children of a hydrated island are
// serialized as static HTML and would never hydrate themselves.
export default function AdminGuard({ children }: { children: ReactNode }) {
  const [state, setState] = useState<GuardState>('loading');

  useEffect(() => {
    const token = getAuthToken();
    if (!token && !getRefreshToken()) {
      setState('unauthenticated');
      return;
    }
    // Fast client-side gate from the JWT permissions claim; the server still
    // enforces admin on every API call below.
    if (token && !tokenHasAdminPermission(token)) {
      setState('forbidden');
      return;
    }
    // Verify against the server and warm the stats cache for the dashboard.
    fetchAdminStats()
      .then(() => setState('ok'))
      .catch((e: any) => {
        if (e?.status === 403) {
          setState('forbidden');
        } else if (e?.status === 401) {
          setState('unauthenticated');
        } else {
          setState('error');
        }
      });
  }, []);

  if (state === 'ok') return <>{children}</>;

  if (state === 'unauthenticated') {
    const redirect = encodeURIComponent(window.location.pathname);
    return (
      <div className="max-w-md mx-auto py-16 text-center">
        <p className="eyebrow text-xl">Admin</p>
        <h1 className="font-serif text-3xl font-semibold text-rcommerce-900 mt-1">Sign in required</h1>
        <p className="mt-3 text-sm text-rcommerce-600">
          Please sign in with an admin account to access this area.
        </p>
        <a href={`/account/login?redirect=${redirect}`} className="btn-primary mt-6">
          Sign In
        </a>
      </div>
    );
  }

  if (state === 'forbidden') {
    return (
      <div className="max-w-md mx-auto py-16 text-center">
        <p className="eyebrow text-xl">Admin</p>
        <h1 className="font-serif text-3xl font-semibold text-rcommerce-900 mt-1">Insufficient permissions</h1>
        <p className="mt-3 text-sm text-rcommerce-600">
          Your account does not have admin access. Contact the store owner if you believe this is a mistake.
        </p>
        <a href="/" className="btn-secondary mt-6">
          Back to Store
        </a>
      </div>
    );
  }

  if (state === 'error') {
    return (
      <div className="max-w-md mx-auto py-16 text-center">
        <h1 className="font-serif text-3xl font-semibold text-rcommerce-900">Something went wrong</h1>
        <p className="mt-3 text-sm text-rcommerce-600">Could not verify admin access. Please try again.</p>
        <button onClick={() => window.location.reload()} className="btn-primary mt-6">
          Retry
        </button>
      </div>
    );
  }

  return <p className="font-serif italic text-xl text-rcommerce-500">Loading admin…</p>;
}
