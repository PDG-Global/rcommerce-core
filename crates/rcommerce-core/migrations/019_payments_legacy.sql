-- Migration 019: legacy marker on payments
--
-- Historical/imported payments that carry no provider fee or settlement data
-- are flagged legacy=true so the accounting export can distinguish
-- "imported, fee data unavailable" from "settlement pending".
-- See docs/api/05-accounting-export-feedback.md (P3).

ALTER TABLE payments ADD COLUMN IF NOT EXISTS legacy BOOLEAN NOT NULL DEFAULT false;
