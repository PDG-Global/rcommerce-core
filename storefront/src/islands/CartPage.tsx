import { useState, useEffect } from 'react';
import type { Cart, CartItem } from '../lib/types';
import { getOrCreateCart, updateCartItem, removeCartItem, applyCoupon, formatMoney } from '../lib/cart';
import { visibleItems, visibleItemCount } from '../lib/bundles';

export default function CartPage() {
  const [cart, setCart] = useState<Cart | null>(null);
  const [loading, setLoading] = useState(true);
  const [coupon, setCoupon] = useState('');
  const [couponError, setCouponError] = useState<string | null>(null);
  const [couponLoading, setCouponLoading] = useState(false);

  const refreshCart = async () => {
    const c = await getOrCreateCart();
    setCart(c);
  };

  useEffect(() => {
    (async () => {
      try {
        await refreshCart();
      } finally {
        setLoading(false);
      }
    })();
  }, []);

  const handleUpdate = async (itemId: string, qty: number) => {
    if (!cart) return;
    if (qty <= 0) {
      await removeCartItem(cart.id, itemId);
    } else {
      await updateCartItem(cart.id, itemId, qty);
    }
    await refreshCart();
  };

  const handleCoupon = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!cart || !coupon.trim()) return;
    setCouponLoading(true);
    setCouponError(null);
    try {
      await applyCoupon(cart.id, coupon.trim());
      setCoupon('');
      await refreshCart();
    } catch (err: any) {
      setCouponError(err.message || 'Invalid coupon code');
    } finally {
      setCouponLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="py-24 text-center">
        <p className="font-serif italic text-xl text-rcommerce-500">Loading your cart…</p>
      </div>
    );
  }

  if (!cart || visibleItems(cart.items).length === 0) {
    return (
      <div className="py-24 text-center">
        <svg xmlns="http://www.w3.org/2000/svg" width="56" height="56" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" className="mx-auto text-rcommerce-300">
          <path d="M6 7h12l1.5 13.5a1 1 0 0 1-1 1.1H5.5a1 1 0 0 1-1-1.1L6 7Z" />
          <path d="M9 10V6a3 3 0 0 1 6 0v4" />
        </svg>
        <p className="mt-6 font-serif italic text-2xl text-rcommerce-500">Your cart is empty.</p>
        <a href="/products" className="btn-primary mt-8 inline-block">Shop Products</a>
      </div>
    );
  }

  const items = visibleItems(cart.items);
  const totalItems = visibleItemCount(cart.items);
  const hasDiscount = parseFloat(cart.discount_total || '0') > 0;

  return (
    <div className="grid grid-cols-1 lg:grid-cols-3 gap-12">
      <div className="lg:col-span-2">
        <div className="hidden sm:grid grid-cols-12 gap-4 pb-4 border-b border-rcommerce-200 text-[11px] font-semibold uppercase tracking-button text-rcommerce-500">
          <span className="col-span-6">Product</span>
          <span className="col-span-2 text-center">Quantity</span>
          <span className="col-span-2 text-right">Total</span>
          <span className="col-span-2"></span>
        </div>
        <div className="divide-y divide-rcommerce-200">
          {items.map((item: CartItem) => (
            <div key={item.id} className="py-6 grid grid-cols-1 sm:grid-cols-12 gap-4 items-center">
              <div className="sm:col-span-6 flex gap-4">
                <a href={`/products/${item.product_id}`} className="w-24 h-24 shrink-0 bg-rcommerce-100 overflow-hidden">
                  {item.image_url ? (
                    <img src={item.image_url} alt={item.title} className="w-full h-full object-cover" loading="lazy" />
                  ) : (
                    <div className="w-full h-full" />
                  )}
                </a>
                <div className="min-w-0">
                  <a href={`/products/${item.product_id}`} className="font-medium text-rcommerce-900 hover:text-rcommerce-accent line-clamp-2">
                    {item.title}
                  </a>
                  {item.sku && <p className="mt-1 text-xs text-rcommerce-400">SKU: {item.sku}</p>}
                  <p className="mt-1 text-sm text-rcommerce-600">{cart.currency} {formatMoney(item.unit_price)}</p>
                </div>
              </div>
              <div className="sm:col-span-2 flex sm:justify-center">
                <div className="inline-flex items-center border border-rcommerce-300">
                  <button onClick={() => handleUpdate(item.id, item.quantity - 1)} aria-label="Decrease quantity" className="w-9 h-9 flex items-center justify-center text-rcommerce-600 hover:bg-rcommerce-100">−</button>
                  <span className="w-10 text-center text-sm font-medium">{item.quantity}</span>
                  <button onClick={() => handleUpdate(item.id, item.quantity + 1)} aria-label="Increase quantity" className="w-9 h-9 flex items-center justify-center text-rcommerce-600 hover:bg-rcommerce-100">+</button>
                </div>
              </div>
              <div className="sm:col-span-2 sm:text-right font-semibold text-rcommerce-900">
                {cart.currency} {formatMoney(item.total)}
              </div>
              <div className="sm:col-span-2 sm:text-right">
                <button onClick={() => handleUpdate(item.id, 0)} className="text-xs text-rcommerce-400 underline underline-offset-2 hover:text-rcommerce-danger">
                  Remove
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="lg:col-span-1">
        <div className="bg-rcommerce-50 border border-rcommerce-200 p-6 lg:sticky lg:top-24">
          <h2 className="font-serif text-xl text-rcommerce-900">Order Summary</h2>
          <p className="mt-1 text-sm text-rcommerce-500">{totalItems} {totalItems === 1 ? 'item' : 'items'}</p>

          <form onSubmit={handleCoupon} className="mt-6">
            <label htmlFor="coupon" className="block text-[11px] font-semibold uppercase tracking-button text-rcommerce-500 mb-2">
              Discount code
            </label>
            <div className="flex gap-2">
              <input
                id="coupon"
                type="text"
                value={coupon}
                onChange={(e) => setCoupon(e.target.value)}
                placeholder="Enter code"
                className="input flex-1"
              />
              <button type="submit" disabled={couponLoading || !coupon.trim()} className="btn-secondary px-4 shrink-0">
                {couponLoading ? '…' : 'Apply'}
              </button>
            </div>
            {couponError && <p className="mt-2 text-sm text-rcommerce-danger">{couponError}</p>}
          </form>

          <div className="mt-6 pt-6 border-t border-rcommerce-200 space-y-2.5">
            <div className="flex justify-between text-sm">
              <span className="text-rcommerce-600">Subtotal</span>
              <span className="font-medium text-rcommerce-900">{cart.currency} {formatMoney(cart.subtotal)}</span>
            </div>
            {hasDiscount && (
              <div className="flex justify-between text-sm">
                <span className="text-rcommerce-600">Discount</span>
                <span className="font-medium text-rcommerce-success">−{cart.currency} {formatMoney(cart.discount_total)}</span>
              </div>
            )}
            <div className="flex justify-between text-sm">
              <span className="text-rcommerce-600">Shipping</span>
              <span className="text-rcommerce-500">Calculated at checkout</span>
            </div>
            <div className="flex justify-between pt-3 border-t border-rcommerce-200">
              <span className="font-semibold text-rcommerce-900">Total</span>
              <span className="font-serif text-2xl text-rcommerce-900">{cart.currency} {formatMoney(cart.total)}</span>
            </div>
          </div>

          <a href="/checkout" className="block w-full btn-primary text-center mt-6">
            Checkout ⟶
          </a>
          <a href="/products" className="block w-full mt-3 text-center text-[13px] font-semibold uppercase tracking-button text-rcommerce-700 hover:text-rcommerce-accent py-2">
            Continue shopping
          </a>
        </div>
      </div>
    </div>
  );
}
