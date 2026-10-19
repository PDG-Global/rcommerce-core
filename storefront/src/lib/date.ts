const MONTHS = [
  'January',
  'February',
  'March',
  'April',
  'May',
  'June',
  'July',
  'August',
  'September',
  'October',
  'November',
  'December',
];

/// Deterministic date rendering for SSR + hydration. Uses UTC components and
/// manual formatting so the Cloudflare worker (UTC) and the client (any
/// locale/timezone) produce identical text — toLocaleDateString() mismatches
/// break React hydration with error #418.
export function formatDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return `${d.getUTCDate()} ${MONTHS[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
}
