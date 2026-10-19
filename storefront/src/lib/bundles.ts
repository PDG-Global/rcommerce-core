/**
 * Bundle expansion handling. A bundle line renders expanded from the backend:
 * a priced PARENT line (pack name, pack quantity) plus $0 COMPONENT lines for
 * the individual units. Component lines must never appear in item lists —
 * only the parent line shows. Totals are unaffected (components are $0).
 *
 * Flag shapes:
 *  - Cart API items: `custom_attributes.is_bundle_component === true`
 *    (the parent line carries `is_bundle_parent`).
 *  - Order API items: top-level `is_bundle_component: true`
 *    (parent carries `is_bundle_parent: true` or nothing).
 */

function customAttributesFlag(item: Record<string, unknown>): boolean {
  const ca = item.custom_attributes;
  if (typeof ca === 'string') {
    try {
      return JSON.parse(ca)?.is_bundle_component === true;
    } catch {
      return false;
    }
  }
  if (ca && typeof ca === 'object') {
    return (ca as Record<string, unknown>).is_bundle_component === true;
  }
  return false;
}

export function isBundleComponent(item: unknown): boolean {
  if (!item || typeof item !== 'object') return false;
  const record = item as Record<string, unknown>;
  return record.is_bundle_component === true || customAttributesFlag(record);
}

/** Item lines that should be displayed (bundle components removed). */
export function visibleItems<T>(items: T[] | null | undefined): T[] {
  return (items ?? []).filter((item) => !isBundleComponent(item));
}

/** Display item count: sums quantities of visible (non-component) lines. */
export function visibleItemCount(items: { quantity: number }[] | null | undefined): number {
  return visibleItems(items).reduce((sum, item) => sum + item.quantity, 0);
}
