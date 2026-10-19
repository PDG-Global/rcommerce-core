-- Migration 011: widen api_keys.key_prefix to VARCHAR(64).
-- The WC-integration keys use full 40-char hex prefixes; fresh installs
-- created from the old 001 used VARCHAR(16) and reject them.

ALTER TABLE api_keys ALTER COLUMN key_prefix TYPE VARCHAR(64);
