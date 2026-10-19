/**
 * Strips HTML tags from a string, decodes common HTML entities,
 * and collapses whitespace. Used for SEO/meta/feed fields where
 * raw HTML must never leak (meta descriptions, JSON-LD, RSS feeds).
 */
export function stripHtml(html: string | null | undefined): string {
  if (!html) return '';

  let text = String(html);

  // Decode common named/numeric entities before tag stripping (&amp; etc.)
  text = text.replace(/&[a-zA-Z0-9#]+;/g, (entity) => decodeEntity(entity));

  // Strip tags
  text = text.replace(/<[^>]*>/g, ' ');

  // Decode entities again in case they were inside attribute-free text
  text = text.replace(/&[a-zA-Z0-9#]+;/g, (entity) => decodeEntity(entity));

  // Collapse whitespace
  return text.replace(/\s+/g, ' ').trim();
}

function decodeEntity(entity: string): string {
  const named: Record<string, string> = {
    '&amp;': '&',
    '&lt;': '<',
    '&gt;': '>',
    '&quot;': '"',
    '&apos;': "'",
    '&nbsp;': ' ',
    '&ndash;': '–',
    '&mdash;': '—',
    '&lsquo;': '‘',
    '&rsquo;': '’',
    '&ldquo;': '“',
    '&rdquo;': '”',
    '&hellip;': '…',
    '&copy;': '©',
    '&reg;': '®',
    '&trade;': '™',
    '&deg;': '°',
    '&plusmn;': '±',
    '&times;': '×',
    '&divide;': '÷',
    '&euro;': '€',
    '&pound;': '£',
    '&yen;': '¥',
    '&cent;': '¢',
  };

  const lower = entity.toLowerCase();
  if (named[lower]) return named[lower];

  // Numeric: &#123; or &#x1F600;
  const dec = entity.match(/^&#(\d+);$/);
  if (dec) {
    try {
      return String.fromCodePoint(parseInt(dec[1], 10));
    } catch {
      return entity;
    }
  }
  const hex = entity.match(/^&#[xX]([0-9a-fA-F]+);$/);
  if (hex) {
    try {
      return String.fromCodePoint(parseInt(hex[1], 16));
    } catch {
      return entity;
    }
  }

  return entity;
}

/**
 * Strips HTML and truncates to a maximum length, trimming at a
 * word boundary when possible and adding an ellipsis if truncated.
 */
export function stripHtmlTruncated(html: string | null | undefined, maxLength: number): string {
  const text = stripHtml(html);
  if (text.length <= maxLength) return text;
  const sliced = text.slice(0, maxLength);
  const lastSpace = sliced.lastIndexOf(' ');
  return (lastSpace > maxLength * 0.6 ? sliced.slice(0, lastSpace) : sliced).trimEnd() + '…';
}
