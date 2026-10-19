import { useState } from 'react';
import { apiFetchJson } from '../lib/api';

export default function RegisterForm() {
  const [form, setForm] = useState({
    email: '',
    password: '',
    first_name: '',
    last_name: '',
    phone: '',
  });
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);

  const update = (field: keyof typeof form, value: string) => {
    setForm((prev) => ({ ...prev, [field]: value }));
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (form.password.length < 8) {
      setError('Password must be at least 8 characters');
      return;
    }
    setLoading(true);
    setError(null);
    try {
      await apiFetchJson<{ message: string }>('/auth/register', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          ...form,
          phone: form.phone || null,
        }),
      });
      setSuccess(true);
    } catch (e: any) {
      setError(e.message || 'Registration failed');
    } finally {
      setLoading(false);
    }
  };

  if (success) {
    return (
      <div className="text-center py-8">
        <h3 className="text-lg font-semibold text-rcommerce-success">Registration successful!</h3>
        <p className="mt-2 text-rcommerce-600">Please check your email and verify your account before signing in.</p>
        <a href="/account/login" className="mt-4 inline-block btn-primary">Go to Login</a>
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      {error && <p className="text-sm text-rcommerce-danger" aria-live="polite">{error}</p>}
      <div className="grid grid-cols-2 gap-4">
        <input placeholder="First name" value={form.first_name} onChange={(e) => update('first_name', e.target.value)} required className="input" />
        <input placeholder="Last name" value={form.last_name} onChange={(e) => update('last_name', e.target.value)} required className="input" />
      </div>
      <input type="email" placeholder="Email" value={form.email} onChange={(e) => update('email', e.target.value)} required className="input" />
      <input type="tel" placeholder="Phone (optional)" value={form.phone} onChange={(e) => update('phone', e.target.value)} className="input" />
      <input type="password" placeholder="Password (min 8 characters)" value={form.password} onChange={(e) => update('password', e.target.value)} required className="input" />
      <button type="submit" disabled={loading} className="w-full btn-primary">
        {loading ? 'Creating account...' : 'Create Account'}
      </button>
    </form>
  );
}
