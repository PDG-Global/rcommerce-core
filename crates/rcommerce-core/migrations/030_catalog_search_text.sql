-- Text form used by catalogue search on both sides of the comparison: lower case,
-- accents removed, decimal comma read as a dot, spaces removed. "25 kg" and "25kg",
-- "1,2 mm" and "1.2mm", "Béton" and "beton" become equal.
CREATE OR REPLACE FUNCTION catalog_search_text(input TEXT)
RETURNS TEXT
LANGUAGE SQL
IMMUTABLE
PARALLEL SAFE
AS $$
    SELECT regexp_replace(
        regexp_replace(
            translate(
                lower(coalesce(input, '')),
                'àáâãäåçèéêëìíîïñòóôõöøùúûüýÿ',
                'aaaaaaceeeeiiiinoooooouuuuyy'
            ),
            '([0-9]),([0-9])', '\1.\2', 'g'
        ),
        '\s+', '', 'g'
    )
$$;
