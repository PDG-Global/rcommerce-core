import { useState, useEffect } from 'react';
import { apiFetchJson } from '../lib/api';

export default function ResetPasswordForm() {
  const [token, setToken] = useState('');
  const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState('');
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
    }, 2000);
    return () => clearTimeout(timer);
  }, [success]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    if (password.length < 8) {
      setError('Password must be at least 8 characters');
      return;
    }
    if (password !== confirm) {
      setError('Passwords do not match');
      return;
    }
    setLoading(true);
    try {
      await apiFetchJson('/auth/password-reset/confirm', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ token, password }),
      });
      setSuccess(true);
    } catch (e: any) {
      setError(e.message || 'Failed to reset password');
    } finally {
      setLoading(false);
    }
  };

  if (!token) {
    return <p className="text-rcommerce-danger text-sm">Invalid or missing reset token.</p>;
  }

  if (success) {
    return <p className="text-green-700 text-sm">Password reset successful. Redirecting to login...</p>;
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      {error && <p className="text-sm text-rcommerce-danger" aria-live="polite">{error}</p>}
      <div>
        <label className="block text-sm font-medium text-rcommerce-700 mb-1">New Password</label>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          minLength={8}
          className="input"
          autoComplete="new-password"
        />
      </div>
      <div>
        <label className="block text-sm font-medium text-rcommerce-700 mb-1">Confirm Password</label>
        <input
          type="password"
          value={confirm}
          onChange={(e) => setConfirm(e.target.value)}
          required
          minLength={8}
          className="input"
          autoComplete="new-password"
        />
      </div>
      <button type="submit" disabled={loading} className="w-full btn-primary">
        {loading ? 'Resetting...' : 'Reset Password'}
      </button>
    </form>
  );
}
