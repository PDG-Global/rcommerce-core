import { useState } from 'react';
import { addToCart, getOrCreateCart, openCart } from '../lib/cart';
import type { Product } from '../lib/types';

interface Props {
  product: Product;
}

export default function HeroAddToCart({ product }: Props) {
  const [loading, setLoading] = useState(false);
  const outOfStock = product.inventory_quantity <= 0;

  const handleClick = async () => {
    if (outOfStock || loading) return;
    setLoading(true);
    try {
      const cart = await getOrCreateCart();
      await addToCart(cart.id, product.id, 1);
      openCart();
    } catch (e: any) {
      alert(e.message || 'Failed to add to cart');
    } finally {
      setLoading(false);
    }
  };

  return (
    <button
      onClick={handleClick}
      disabled={outOfStock || loading}
      className="w-full mt-6 bg-rcommerce-highlight text-rcommerce-primary font-semibold uppercase text-[14px] tracking-button py-4 px-10 hover:opacity-90 transition-opacity disabled:opacity-50 disabled:cursor-not-allowed"
    >
      {outOfStock ? 'Out of Stock' : loading ? 'Adding...' : 'Add to Cart'}
    </button>
  );
}
