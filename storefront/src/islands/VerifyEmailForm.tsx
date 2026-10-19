import { useState, useEffect } from 'react';
import { apiFetchJson } from '../lib/api';

export default function VerifyEmailForm() {
  const [token, setToken] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    const params = new URLSearchParams(window.location.search);
    const t = params.get('token');
    if (t) setToken(t);
  }, []);

  useEffect(() => {
    if (!success) return;
    const timer = setTimeout(() => {
      window.location.href = '/account/login';
    }, 3000);
    return () => clearTimeout(timer);
  }, [success]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!token) {
      setError('Verification token is missing.');
      return;
    }
    setLoading(true);
    setError(null);
    try {
      await apiFetchJson('/auth/verify-email', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ token }),
      });
      setSuccess(true);
    } catch (e: any) {
      setError(e.message || 'Failed to verify email');
    } finally {
      setLoading(false);
    }
  };

  if (success) {
    return <p className="text-green-700 text-sm">Email verified successfully. Redirecting to login...</p>;
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      {error && <p className="text-sm text-rcommerce-danger" aria-live="polite">{error}</p>}
      <input
        type="hidden"
        value={token}
      />
      <p className="text-rcommerce-600 text-sm">Click the button below to verify your email address.</p>
      <button type="submit" disabled={loading || !token} className="w-full btn-primary">
        {loading ? 'Verifying...' : 'Verify Email'}
      </button>
    </form>
  );
}
