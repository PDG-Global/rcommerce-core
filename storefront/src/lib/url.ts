import type { Product } from './types';

/**
 * Returns the canonical product URL path.
 * Prefers /products/{category}/{slug}, falls back to /products/{slug}.
 */
export function productUrl(product: Pick<Product, 'slug' | 'categories'>): string {
  const category = product.categories?.[0]?.slug;
  return category ? `/products/${category}/${product.slug}` : `/products/${product.slug}`;
}
