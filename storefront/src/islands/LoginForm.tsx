import { useState } from 'react';
import { apiFetchJson, setAuthToken, setRefreshToken, scheduleTokenRefresh, apiFetch } from '../lib/api';
import { mergeGuestCartOnLogin } from '../lib/cart';

interface Props {
  redirectTo?: string;
}

export default function LoginForm({ redirectTo = '/' }: Props) {
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [rememberMe, setRememberMe] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [importedEmail, setImportedEmail] = useState<string | null>(null);
  const [unverifiedEmail, setUnverifiedEmail] = useState<string | null>(null);
  const [resetSent, setResetSent] = useState(false);
  const [resetLoading, setResetLoading] = useState(false);
  const [verificationSent, setVerificationSent] = useState(false);
  const [verificationLoading, setVerificationLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    setImportedEmail(null);
    setUnverifiedEmail(null);
    setResetSent(false);
    setVerificationSent(false);
    try {
      const res = await apiFetchJson<{
        access_token: string;
        refresh_token: string;
        customer: { id: string; email: string; first_name: string; last_name: string };
      }>('/auth/login', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ email, password, remember_me: rememberMe }),
      });
      setAuthToken(res.access_token);
      setRefreshToken(res.refresh_token);
      scheduleTokenRefresh(res.access_token);
      await mergeGuestCartOnLogin();
      window.location.href = redirectTo;
    } catch (e: any) {
      if (e.status === 403 && e.code === 'account_requires_password_reset') {
        setImportedEmail(email);
        setError('This account was imported from our previous store. Please set a new password to continue.');
      } else if (e.status === 403 && e.code === 'email_verification_required') {
        setUnverifiedEmail(email);
        setError('Please verify your email address before signing in.');
      } else {
        setError(e.message || 'Login failed');
      }
    } finally {
      setLoading(false);
    }
  };

  const handleResetPassword = async () => {
    if (!importedEmail) return;
    setResetLoading(true);
    setError(null);
    try {
      const res = await apiFetch('/auth/password-reset', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ email: importedEmail }),
      });
      if (res.ok) {
        setResetSent(true);
        setError(null);
      } else {
        const text = await res.text().catch(() => 'Failed to send reset email');
        setError(text || 'Failed to send reset email');
      }
    } catch (e: any) {
      setError(e.message || 'Failed to send reset email');
    } finally {
      setResetLoading(false);
    }
  };

  const handleResendVerification = async () => {
    if (!unverifiedEmail) return;
    setVerificationLoading(true);
    setError(null);
    try {
      const res = await apiFetch('/auth/resend-verification', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ email: unverifiedEmail }),
      });
      if (res.ok) {
        setVerificationSent(true);
        setError(null);
      } else {
        const text = await res.text().catch(() => 'Failed to send verification email');
        setError(text || 'Failed to send verification email');
      }
    } catch (e: any) {
      setError(e.message || 'Failed to send verification email');
    } finally {
      setVerificationLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      {error && <p className="text-sm text-rcommerce-danger" aria-live="polite">{error}</p>}
      {resetSent && (
        <p className="text-sm text-green-700 bg-green-50 p-3 rounded" aria-live="polite">
          A password reset link has been sent to {importedEmail}. Please check your inbox.
        </p>
      )}
      {verificationSent && (
        <p className="text-sm text-green-700 bg-green-50 p-3 rounded" aria-live="polite">
          A verification link has been sent to {unverifiedEmail}. Please check your inbox.
        </p>
      )}
      <div>
        <label className="block text-sm font-medium text-rcommerce-700 mb-1">Email</label>
        <input
          type="email"
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          required
          className="input"
          autoComplete="email"
        />
      </div>
      <div>
        <label className="block text-sm font-medium text-rcommerce-700 mb-1">Password</label>
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          required
          className="input"
          autoComplete="current-password"
        />
      </div>
      <label className="flex items-center space-x-2 text-sm text-rcommerce-700">
        <input
          type="checkbox"
          checked={rememberMe}
          onChange={(e) => setRememberMe(e.target.checked)}
          className="rounded border-rcommerce-300"
        />
        <span>Remember me</span>
      </label>
      <button type="submit" disabled={loading} className="w-full btn-primary">
        {loading ? 'Signing in...' : 'Sign In'}
      </button>
      {importedEmail && !resetSent && (
        <button
          type="button"
          onClick={handleResetPassword}
          disabled={resetLoading}
          className="w-full btn-secondary"
        >
          {resetLoading ? 'Sending...' : 'Send password reset email'}
        </button>
      )}
      {unverifiedEmail && !verificationSent && (
        <button
          type="button"
          onClick={handleResendVerification}
          disabled={verificationLoading}
          className="w-full btn-secondary"
        >
          {verificationLoading ? 'Sending...' : 'Resend verification email'}
        </button>
      )}
    </form>
  );
}
