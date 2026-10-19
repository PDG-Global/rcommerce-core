import { useState, useEffect } from 'react';
import type { Cart } from '../lib/types';
import { getOrCreateCart, openCart } from '../lib/cart';
import { visibleItemCount } from '../lib/bundles';

export default function CartButton() {
  const [cart, setCart] = useState<Cart | null>(null);

  useEffect(() => {
    const refresh = () => {
      getOrCreateCart()
        .then(setCart)
        .catch(() => {});
    };
    refresh();
    window.addEventListener('cart-updated', refresh);
    return () => window.removeEventListener('cart-updated', refresh);
  }, []);

  const count = visibleItemCount(cart?.items);

  return (
    <button
      onClick={openCart}
      aria-label="Open cart"
      className="relative p-2 text-rcommerce-900 hover:text-rcommerce-accent transition-colors"
    >
      <svg xmlns="http://www.w3.org/2000/svg" width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <path d="M6 7h12l1.5 13.5a1 1 0 0 1-1 1.1H5.5a1 1 0 0 1-1-1.1L6 7Z" />
        <path d="M9 10V6a3 3 0 0 1 6 0v4" />
      </svg>
      {count > 0 && (
        <span className="absolute -top-0.5 -right-0.5 min-w-[18px] h-[18px] px-1 bg-rcommerce-accent text-white text-[10px] font-bold rounded-full flex items-center justify-center">
          {count}
        </span>
      )}
    </button>
  );
}
