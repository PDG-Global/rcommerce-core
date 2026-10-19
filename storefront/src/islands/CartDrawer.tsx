import { useState, useEffect } from 'react';
import type { Cart, CartItem } from '../lib/types';
import { getOrCreateCart, updateCartItem, removeCartItem, formatMoney } from '../lib/cart';
import { visibleItems, visibleItemCount } from '../lib/bundles';

export default function CartDrawer() {
  const [isOpen, setIsOpen] = useState(false);
  const [cart, setCart] = useState<Cart | null>(null);

  const refreshCart = async () => {
    const c = await getOrCreateCart();
    setCart(c);
  };

  useEffect(() => {
    refreshCart();

    const onOpen = () => setIsOpen(true);
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setIsOpen(false);
    };
    window.addEventListener('open-cart', onOpen);
    window.addEventListener('cart-updated', refreshCart);
    document.addEventListener('keydown', onKeyDown);
    return () => {
      window.removeEventListener('open-cart', onOpen);
      window.removeEventListener('cart-updated', refreshCart);
      document.removeEventListener('keydown', onKeyDown);
    };
  }, []);

  useEffect(() => {
    document.body.style.overflow = isOpen ? 'hidden' : '';
    return () => {
      document.body.style.overflow = '';
    };
  }, [isOpen]);

  const handleUpdate = async (itemId: string, qty: number) => {
    if (!cart) return;
    if (qty <= 0) {
      await removeCartItem(cart.id, itemId);
    } else {
      await updateCartItem(cart.id, itemId, qty);
    }
    await refreshCart();
  };

  const items = visibleItems(cart?.items);
  const totalItems = visibleItemCount(cart?.items);

  return (
    <>
      {isOpen && (
        <div className="fixed inset-0 z-50">
          <div onClick={() => setIsOpen(false)} className="absolute inset-0 bg-rcommerce-900/50 backdrop-blur-sm" />
          <div
            role="dialog"
            aria-modal="true"
            aria-label="Shopping cart"
            className="absolute top-0 right-0 h-full w-full md:w-[440px] bg-white shadow-2xl flex flex-col"
          >
            <div className="px-6 py-5 border-b border-rcommerce-200 flex justify-between items-center">
              <h2 className="font-serif text-xl text-rcommerce-900">
                Your Cart <span className="text-rcommerce-400">({totalItems})</span>
              </h2>
              <button onClick={() => setIsOpen(false)} aria-label="Close cart" className="p-1 text-rcommerce-500 hover:text-rcommerce-900">
                <svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 6 6 18" />
                  <path d="m6 6 12 12" />
                </svg>
              </button>
            </div>

            <div className="flex-1 overflow-y-auto px-6 py-6">
              {!cart || items.length === 0 ? (
                <div className="h-full flex flex-col items-center justify-center text-center gap-4">
                  <svg xmlns="http://www.w3.org/2000/svg" width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" className="text-rcommerce-300">
                    <path d="M6 7h12l1.5 13.5a1 1 0 0 1-1 1.1H5.5a1 1 0 0 1-1-1.1L6 7Z" />
                    <path d="M9 10V6a3 3 0 0 1 6 0v4" />
                  </svg>
                  <p className="font-serif italic text-lg text-rcommerce-500">Your cart is empty.</p>
                  <a href="/products" className="btn-secondary mt-2">Shop Products</a>
                </div>
              ) : (
                <div className="divide-y divide-rcommerce-200">
                  {items.map((item: CartItem) => (
                    <div key={item.id} className="flex gap-4 py-5 first:pt-0 last:pb-0">
                      <a href={`/products/${item.product_id}`} className="w-20 h-20 shrink-0 bg-rcommerce-100 overflow-hidden">
                        {item.image_url ? (
                          <img src={item.image_url} alt={item.title} className="w-full h-full object-cover" loading="lazy" />
                        ) : (
                          <div className="w-full h-full" />
                        )}
                      </a>
                      <div className="flex-1 min-w-0">
                        <a href={`/products/${item.product_id}`} className="font-medium text-sm text-rcommerce-900 hover:text-rcommerce-accent line-clamp-2">
                          {item.title}
                        </a>
                        <p className="mt-1 text-sm text-rcommerce-600">
                          {cart.currency} {formatMoney(item.unit_price)}
                        </p>
                        <div className="mt-2 flex items-center justify-between">
                          <div className="inline-flex items-center border border-rcommerce-300">
                            <button onClick={() => handleUpdate(item.id, item.quantity - 1)} aria-label="Decrease quantity" className="w-7 h-7 flex items-center justify-center text-rcommerce-600 hover:bg-rcommerce-100">−</button>
                            <span className="w-8 text-center text-sm">{item.quantity}</span>
                            <button onClick={() => handleUpdate(item.id, item.quantity + 1)} aria-label="Increase quantity" className="w-7 h-7 flex items-center justify-center text-rcommerce-600 hover:bg-rcommerce-100">+</button>
                          </div>
                          <button onClick={() => handleUpdate(item.id, 0)} className="text-xs text-rcommerce-400 underline underline-offset-2 hover:text-rcommerce-danger">
                            Remove
                          </button>
                        </div>
                      </div>
                      <div className="text-sm font-semibold text-rcommerce-900 whitespace-nowrap">
                        {cart.currency} {formatMoney(item.total)}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>

            {cart && items.length > 0 && (
              <div className="px-6 py-5 border-t border-rcommerce-200 bg-rcommerce-50">
                <div className="flex justify-between items-center mb-1">
                  <span className="text-sm text-rcommerce-600">Subtotal</span>
                  <span className="font-serif text-xl text-rcommerce-900">{cart.currency} {formatMoney(cart.subtotal)}</span>
                </div>
                <p className="text-xs text-rcommerce-500 mb-4">Shipping and taxes calculated at checkout.</p>
                <a href="/checkout" className="block w-full btn-primary text-center">Checkout</a>
                <a href="/cart" className="block w-full mt-2 text-center text-[13px] font-semibold uppercase tracking-button text-rcommerce-700 hover:text-rcommerce-accent py-2">
                  View Cart
                </a>
              </div>
            )}
          </div>
        </div>
      )}
    </>
  );
}
