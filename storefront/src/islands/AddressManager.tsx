import { useState } from 'react';
import type { Address } from '../lib/types';
import { apiFetchAuthenticated } from '../lib/api';
import CountryStateFields from './CountryStateFields';

interface Props {
  addresses: Address[];
  onUpdated: () => void;
}

const emptyAddress: Omit<Address, 'id' | 'customer_id'> = {
  first_name: '',
  last_name: '',
  company: null,
  phone: '',
  address1: '',
  address2: null,
  city: '',
  state: null,
  country: 'US',
  zip: '',
  is_default_shipping: false,
  is_default_billing: false,
};

export default function AddressManager({ addresses, onUpdated }: Props) {
  const [editing, setEditing] = useState<Address | null>(null);
  const [form, setForm] = useState(emptyAddress);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const startNew = () => {
    setEditing(null);
    setForm(emptyAddress);
  };

  const startEdit = (addr: Address) => {
    setEditing(addr);
    setForm({ ...addr });
  };

  const update = (field: keyof typeof form, value: string | boolean) => {
    setForm((prev) => ({ ...prev, [field]: value }));
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError(null);
    try {
      const payload = {
        ...form,
        company: form.company || null,
        address2: form.address2 || null,
        state: form.state || null,
        phone: (form.phone || '').trim(),
      };
      const method = editing ? 'PUT' : 'POST';
      const path = editing
        ? `/customers/me/addresses/${editing.id}`
        : '/customers/me/addresses';
      await apiFetchAuthenticated(path, {
        method,
        body: JSON.stringify(payload),
      });
      setEditing(null);
      setForm(emptyAddress);
      onUpdated();
    } catch (e: any) {
      setError(e.message || 'Failed to save address');
    } finally {
      setLoading(false);
    }
  };

  const deleteAddress = async (id: string) => {
    if (!confirm('Delete this address?')) return;
    try {
      await apiFetchAuthenticated(`/customers/me/addresses/${id}`, {
        method: 'DELETE',
      });
      onUpdated();
    } catch (e: any) {
      alert(e.message || 'Failed to delete address');
    }
  };

  return (
    <div className="space-y-6">
      <h3 className="text-lg font-semibold">Saved Addresses</h3>

      {addresses.length === 0 && (
        <p className="text-rcommerce-600">No saved addresses yet.</p>
      )}

      <div className="space-y-4">
        {addresses.map((addr) => (
          <div key={addr.id} className="border border-rcommerce-200 p-4 rounded">
            <p className="font-medium">{addr.first_name} {addr.last_name}</p>
            <p className="text-sm text-rcommerce-600">{addr.address1}</p>
            {addr.address2 && <p className="text-sm text-rcommerce-600">{addr.address2}</p>}
            <p className="text-sm text-rcommerce-600">{addr.city}, {addr.state || ''} {addr.zip}, {addr.country}</p>
            <div className="mt-3 flex gap-3">
              <button onClick={() => startEdit(addr)} className="text-sm text-rcommerce-accent hover:underline">Edit</button>
              <button onClick={() => deleteAddress(addr.id)} className="text-sm text-rcommerce-danger hover:underline">Delete</button>
            </div>
          </div>
        ))}
      </div>

      <button onClick={startNew} className="btn-secondary">Add New Address</button>

      {(!editing || form.first_name !== '' || editing) && (
        <form onSubmit={handleSubmit} className="border-t border-rcommerce-200 pt-6 space-y-4">
          <h4 className="font-medium">{editing ? 'Edit Address' : 'New Address'}</h4>
          {error && <p className="text-sm text-rcommerce-danger">{error}</p>}
          <div className="grid grid-cols-2 gap-4">
            <input placeholder="First name" value={form.first_name} onChange={(e) => update('first_name', e.target.value)} required className="input" />
            <input placeholder="Last name" value={form.last_name} onChange={(e) => update('last_name', e.target.value)} required className="input" />
          </div>
          <input placeholder="Company (optional)" value={form.company || ''} onChange={(e) => update('company', e.target.value)} className="input" />
          <input placeholder="Address" value={form.address1} onChange={(e) => update('address1', e.target.value)} required className="input" />
          <input placeholder="Apartment, suite, etc. (optional)" value={form.address2 || ''} onChange={(e) => update('address2', e.target.value)} className="input" />
          <div className="grid grid-cols-2 gap-4">
            <input placeholder="City" value={form.city} onChange={(e) => update('city', e.target.value)} required className="input" />
            <input placeholder="Postal code" value={form.zip} onChange={(e) => update('zip', e.target.value)} required className="input" />
          </div>
          <div className="grid grid-cols-2 gap-4">
            <CountryStateFields
              country={form.country}
              onCountryChange={(value) => setForm((prev) => ({ ...prev, country: value, state: null }))}
              state={form.state || ''}
              onStateChange={(value) => update('state', value)}
              className="input"
            />
          </div>
          <input type="tel" placeholder="Phone" value={form.phone || ''} onChange={(e) => update('phone', e.target.value)} required className="input" />
          <div className="flex gap-4">
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={form.is_default_shipping} onChange={(e) => update('is_default_shipping', e.target.checked)} />
              Default shipping
            </label>
            <label className="flex items-center gap-2 text-sm">
              <input type="checkbox" checked={form.is_default_billing} onChange={(e) => update('is_default_billing', e.target.checked)} />
              Default billing
            </label>
          </div>
          <div className="flex gap-3">
            <button type="submit" disabled={loading} className="btn-primary">{loading ? 'Saving...' : 'Save Address'}</button>
            {editing && <button type="button" onClick={() => { setEditing(null); setForm(emptyAddress); }} className="btn-secondary">Cancel</button>}
          </div>
        </form>
      )}
    </div>
  );
}
