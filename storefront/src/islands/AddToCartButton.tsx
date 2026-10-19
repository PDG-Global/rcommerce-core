import { useState, useMemo, useEffect } from 'react';
import { addToCart, getOrCreateCart, openCart } from '../lib/cart';
import type { PackOption, Product } from '../lib/types';

interface Props {
  product: Product;
  disabled?: boolean;
  packOptions?: PackOption[];
  /** Pre-select the option values pointing at this variant (e.g. Deck only). */
  defaultVariantId?: string;
}

export default function AddToCartButton({ product, disabled = false, packOptions = [], defaultVariantId }: Props) {
  const [loading, setLoading] = useState(false);
  const [quantity, setQuantity] = useState(1);
  const [selectedValues, setSelectedValues] = useState<Record<string, string>>(() => {
    if (!defaultVariantId) return {};
    const initial: Record<string, string> = {};
    for (const option of product.options || []) {
      const value = option.values.find((v) => v.variant_id === defaultVariantId);
      if (value) initial[option.id] = value.variant_id;
    }
    return initial;
  });
  const [selectedPackId, setSelectedPackId] = useState<string | null>(null);

  const selectedPack = packOptions.find((o) => o.product_id === selectedPackId) || null;

  const requiresSelection = (product.options || []).length > 0;

  const variantById = useMemo(
    () => new Map(product.variants.map((v) => [v.id, v])),
    [product.variants],
  );

  const selectedVariant = useMemo(() => {
    if (!requiresSelection) return null;
    const valueIds = Object.values(selectedValues);
    if (valueIds.length !== (product.options || []).length) return null;
    // All selected option values should point to the same variant id
    const first = valueIds[0];
    if (!valueIds.every((id) => id === first)) return null;
    return product.variants.find((variant) => variant.id === first) || null;
  }, [selectedValues, product.options, product.variants, requiresSelection]);

  // Keep the SSR-rendered price display in sync with the selected purchase option
  useEffect(() => {
    const priceEl = document.getElementById('product-price');
    if (priceEl) {
      priceEl.textContent = selectedVariant
        ? `${product.currency} ${selectedVariant.price}`
        : selectedPack
          ? `${selectedPack.currency} ${selectedPack.price}`
          : `${product.currency} ${product.price}`;
    }
    const compareEl = document.getElementById('product-compare-price');
    if (compareEl) {
      compareEl.style.display = selectedPack || selectedVariant ? 'none' : '';
    }
  }, [selectedPack, selectedVariant, product.currency, product.price]);

  const isComplete = !requiresSelection || selectedVariant !== null;
  // Stock is judged on the actually-selected variant when there is one —
  // the base product quantity says nothing about a sold-out variant.
  const isOutOfStock = selectedPack
    ? selectedPack.inventory_quantity <= 0
    : selectedVariant
      ? selectedVariant.inventory_quantity <= 0
      : product.inventory_quantity <= 0;
  // The base product being sold out only blocks the base purchase option,
  // not in-stock packs.
  const isDisabled = (selectedPack ? false : disabled) || isOutOfStock || loading || !isComplete;

  const handleOptionChange = (optionId: string, variantId: string) => {
    setSelectedValues((prev) => ({ ...prev, [optionId]: variantId }));
  };

  const handleClick = async () => {
    if (!isComplete || isOutOfStock) return;
    setLoading(true);
    try {
      const cart = await getOrCreateCart();
      await addToCart(cart.id, selectedPack?.product_id ?? product.id, quantity, selectedVariant?.id || undefined);
      openCart();
    } catch (e: any) {
      alert(e.message || 'Failed to add to cart');
    } finally {
      setLoading(false);
    }
  };

  const buttonText = () => {
    if (isOutOfStock) return 'Out of Stock';
    if (loading) return 'Adding...';
    if (!isComplete) return 'Select options';
    return 'Add to Cart';
  };

  return (
    <div className="mt-8 space-y-6">
      {packOptions.length > 0 && (
        <div>
          <label className="block text-[12px] font-semibold uppercase tracking-button text-rcommerce-900 mb-2.5">
            Purchase option
          </label>
          <div className="flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => setSelectedPackId(null)}
              disabled={disabled || product.inventory_quantity <= 0}
              className={`px-4 py-2.5 border text-sm transition-colors ${
                selectedPack === null
                  ? 'border-rcommerce-accent bg-rcommerce-accent text-white'
                  : 'border-rcommerce-300 text-rcommerce-700 hover:border-rcommerce-accent'
              } ${product.inventory_quantity <= 0 ? 'opacity-50 cursor-not-allowed' : ''}`}
            >
              Wheel — {product.currency} {product.price}
              {product.inventory_quantity <= 0 && ' · Sold out'}
            </button>
            {packOptions.map((option) => {
              const soldOut = option.inventory_quantity <= 0;
              const isSelected = selectedPackId === option.product_id;
              return (
                <button
                  key={option.product_id}
                  type="button"
                  onClick={() => setSelectedPackId(option.product_id)}
                  disabled={soldOut}
                  className={`px-4 py-2.5 border text-sm transition-colors ${
                    isSelected
                      ? 'border-rcommerce-accent bg-rcommerce-accent text-white'
                      : 'border-rcommerce-300 text-rcommerce-700 hover:border-rcommerce-accent'
                  } ${soldOut ? 'opacity-50 cursor-not-allowed' : ''}`}
                >
                  {option.label} — {option.currency} {option.price}
                  {soldOut && ' · Sold out'}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {(product.options || []).map((option) => (
        <div key={option.id}>
          <label className="block text-[12px] font-semibold uppercase tracking-button text-rcommerce-900 mb-2.5">
            {option.name}
          </label>
          <div className="flex flex-wrap gap-2">
            {option.values.map((value) => {
              const valueVariant = value.variant_id ? variantById.get(value.variant_id) : undefined;
              const valueSoldOut = !!valueVariant && valueVariant.inventory_quantity <= 0;
              const isSelected = selectedValues[option.id] === value.variant_id;
              return (
                <button
                  key={value.id}
                  type="button"
                  onClick={() => handleOptionChange(option.id, value.variant_id)}
                  disabled={isOutOfStock || valueSoldOut}
                  className={`px-4 py-2.5 border text-sm transition-colors ${
                    isSelected
                      ? 'border-rcommerce-900 bg-rcommerce-900 text-white'
                      : 'border-rcommerce-300 text-rcommerce-700 hover:border-rcommerce-900'
                  } ${isOutOfStock || valueSoldOut ? 'opacity-50 cursor-not-allowed' : ''}`}
                >
                  {value.value}
                  {valueSoldOut && ' · Sold out'}
                </button>
              );
            })}
          </div>
        </div>
      ))}

      <div>
        <label className="block text-[12px] font-semibold uppercase tracking-button text-rcommerce-900 mb-2.5">
          Quantity
        </label>
        <div className="inline-flex items-center border border-rcommerce-300">
          <button
            type="button"
            onClick={() => setQuantity((q) => Math.max(1, q - 1))}
            disabled={isOutOfStock}
            aria-label="Decrease quantity"
            className="w-11 h-11 flex items-center justify-center text-lg text-rcommerce-600 hover:bg-rcommerce-100 disabled:opacity-50"
          >
            −
          </button>
          <span className="w-12 text-center text-sm font-medium">{quantity}</span>
          <button
            type="button"
            onClick={() => setQuantity((q) => q + 1)}
            disabled={isOutOfStock}
            aria-label="Increase quantity"
            className="w-11 h-11 flex items-center justify-center text-lg text-rcommerce-600 hover:bg-rcommerce-100 disabled:opacity-50"
          >
            +
          </button>
        </div>
      </div>

      <button
        onClick={handleClick}
        disabled={isDisabled}
        className="w-full btn-primary disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {buttonText()}
      </button>
    </div>
  );
}
