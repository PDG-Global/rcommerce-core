import { brand } from './config';
import { defaultContent } from './default';
import { mergeContent, type BrandContent, type DeepPartial } from './content';

interface PrivateOverlayModule {
  content?: DeepPartial<BrandContent>;
}

// Store-private content overlays (src/lib/brand/private/, excluded from the
// public repo sync). Globbed so a checkout without the directory still
// builds the generic default store. Each overlay is keyed by its module
// basename, which is the store's PUBLIC_BRAND_SLUG.
const overlayModules = import.meta.glob<PrivateOverlayModule>('./private/*.ts', {
  eager: true,
});

const OVERRIDES: Record<string, DeepPartial<BrandContent>> = {};
for (const [path, mod] of Object.entries(overlayModules)) {
  const slug = path.replace(/^\.\/private\/(.*)\.ts$/, '$1');
  if (mod.content) OVERRIDES[slug] = mod.content;
}

/**
 * The active brand's user-visible content: the generic defaults deep-merged
 * with the override module for the active brand slug (if any). Unknown slugs
 * fall back to the defaults unchanged.
 */
export const content: BrandContent = mergeContent(defaultContent, OVERRIDES[brand.slug]);

export { brand };
export type { BrandContent, DeepPartial, Link, FooterColumn } from './content';
export { defaultContent } from './default';
