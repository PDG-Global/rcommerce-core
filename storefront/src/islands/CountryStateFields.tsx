import { COUNTRIES, subdivisionsFor } from '../lib/countries';

/**
 * WooCommerce-style country + state fields shared by every customer address
 * form. Country is always a dropdown of ISO codes. When the selected country
 * has known regions (US/CA/AU — see src/lib/countries.ts), the state field
 * becomes a required dropdown of that country's regions; otherwise it stays
 * an optional free-text input. Emits the same values the forms always have:
 * a 2-letter country code and a state value (region code when selected from
 * the dropdown).
 */
export default function CountryStateFields({
  country,
  onCountryChange,
  state,
  onStateChange,
  className,
  shippable = null,
  countryAutoComplete,
  stateAutoComplete,
}: {
  country: string;
  onCountryChange: (country: string) => void;
  state: string;
  onStateChange: (state: string) => void;
  className: string;
  /** Optional restriction to a set of shippable country codes (checkout). */
  shippable?: string[] | null;
  countryAutoComplete?: string;
  stateAutoComplete?: string;
}) {
  let options = COUNTRIES;
  if (shippable && shippable.length > 0) {
    const allowed = new Set(shippable);
    options = COUNTRIES.filter((c) => allowed.has(c.code));
    if (country && !allowed.has(country)) {
      // Keep the current selection visible even if it isn't shippable.
      const current = COUNTRIES.find((c) => c.code === country);
      if (current) options = [current, ...options];
    }
  }

  const regions = subdivisionsFor(country);
  // Legacy saved states may hold the region name rather than its code.
  const regionValue = regions.find((r) => r.code === state || r.name === state)?.code ?? state;

  return (
    <>
      <select
        aria-label="Country"
        value={country}
        required
        autoComplete={countryAutoComplete}
        className={className}
        onChange={(e) => onCountryChange(e.target.value)}
      >
        {options.map((c) => (
          <option key={c.code} value={c.code}>{c.name}</option>
        ))}
      </select>
      {regions.length > 0 ? (
        <select
          aria-label="State/Province"
          value={regionValue}
          required
          autoComplete={stateAutoComplete}
          className={className}
          onChange={(e) => onStateChange(e.target.value)}
        >
          <option value="">State/Province</option>
          {regions.map((s) => (
            <option key={s.code} value={s.code}>{s.name}</option>
          ))}
        </select>
      ) : (
        <input
          placeholder="State/Province (optional)"
          value={state}
          autoComplete={stateAutoComplete}
          onChange={(e) => onStateChange(e.target.value)}
          className={className}
        />
      )}
    </>
  );
}
