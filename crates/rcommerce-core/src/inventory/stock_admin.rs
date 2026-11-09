//! Stock management for the back-office: stock levels per location,
//! adjustments, goods receipts, reorder points and the movement history.
//!
//! Stock is kept in two places that must agree: `inventory_levels` (per
//! location, `available_quantity` = free stock) and `products` /
//! `product_variants` (`inventory_quantity`, the total the cart checks). Every
//! change here updates both and records a `stock_movements` row, in one
//! transaction, so a failure leaves nothing half-applied.
//!
//! Each change locks its (product, variant, location) key with a transaction
//! advisory lock: the unique index treats a NULL variant as distinct, so two
//! first receipts of a simple product could otherwise insert two rows.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::inventory::tracking::StockMovementType;
use crate::{Error, Result};

/// Why stock was changed by hand
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdjustmentReason {
    /// Fixing a booking mistake
    Correction,
    /// Broken, spoiled or stolen goods
    Damage,
    /// Goods found that were not booked
    Found,
    /// Goods brought back by a customer outside an order refund
    Return,
    /// Difference found by a stock count
    Count,
}

impl AdjustmentReason {
    /// The movement type recorded for this reason and direction
    fn movement_type(self, quantity: i32) -> StockMovementType {
        match self {
            AdjustmentReason::Damage => StockMovementType::Lost,
            AdjustmentReason::Found => StockMovementType::Found,
            AdjustmentReason::Return => StockMovementType::Return,
            AdjustmentReason::Correction | AdjustmentReason::Count if quantity < 0 => {
                StockMovementType::Out
            }
            AdjustmentReason::Correction | AdjustmentReason::Count => StockMovementType::In,
        }
    }

    /// The reason as stored in `stock_movements.reason`
    pub fn as_str(self) -> &'static str {
        match self {
            AdjustmentReason::Correction => "correction",
            AdjustmentReason::Damage => "damage",
            AdjustmentReason::Found => "found",
            AdjustmentReason::Return => "return",
            AdjustmentReason::Count => "count",
        }
    }

    /// Whether this reason can move stock in this direction (damage only
    /// removes stock; found and return only add it)
    fn allows(self, quantity: i32) -> bool {
        match self {
            AdjustmentReason::Damage => quantity < 0,
            AdjustmentReason::Found | AdjustmentReason::Return => quantity > 0,
            AdjustmentReason::Correction | AdjustmentReason::Count => true,
        }
    }
}

/// One product (or variant) at one location, as listed in the back-office
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct StockLevelRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub sku: Option<String>,
    pub title: String,
    pub location_id: Uuid,
    pub location_code: String,
    pub location_name: String,
    /// Free stock (what can still be sold)
    pub available: i32,
    /// Stock committed to orders and not yet shipped
    pub reserved: i32,
    pub incoming: i32,
    pub reorder_point: i32,
    pub reorder_quantity: i32,
    pub last_counted_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// Filters of the stock level list
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LevelFilter {
    pub location_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    /// Case-insensitive match on product title, product SKU or variant SKU
    pub q: Option<String>,
    /// Only rows at or below their reorder point
    #[serde(default)]
    pub low_only: bool,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

impl LevelFilter {
    /// (limit, offset), 50 per page by default, at most 200
    fn window(&self) -> (i64, i64) {
        let per_page = self.per_page.unwrap_or(50).clamp(1, 200);
        let page = self.page.unwrap_or(1).max(1);
        (per_page, (page - 1) * per_page)
    }
}

const LEVEL_COLUMNS: &str = "lv.id, lv.product_id, lv.variant_id, COALESCE(v.sku, p.sku) AS sku, \
     CASE WHEN v.id IS NULL THEN p.title ELSE p.title || ' – ' || v.title END AS title, \
     lv.location_id, l.code AS location_code, l.name AS location_name, \
     lv.available_quantity AS available, lv.reserved_quantity AS reserved, \
     lv.incoming_quantity AS incoming, lv.reorder_point, lv.reorder_quantity, \
     lv.last_counted_at, lv.updated_at";

const LEVEL_FROM: &str = "FROM inventory_levels lv \
     JOIN products p ON p.id = lv.product_id \
     JOIN inventory_locations l ON l.id = lv.location_id \
     LEFT JOIN product_variants v ON v.id = lv.variant_id \
     WHERE ($1::uuid IS NULL OR lv.location_id = $1) \
       AND ($2::uuid IS NULL OR lv.product_id = $2) \
       AND ($3::text IS NULL OR p.title ILIKE '%' || $3 || '%' OR p.sku ILIKE '%' || $3 || '%' \
            OR v.sku ILIKE '%' || $3 || '%') \
       AND (NOT $4 OR lv.available_quantity <= lv.reorder_point)";

/// A page of stock levels (by product title, then location) and the total
/// number of matching rows
pub async fn list_levels(pool: &PgPool, filter: &LevelFilter) -> Result<(Vec<StockLevelRow>, i64)> {
    let (limit, offset) = filter.window();
    let q = filter.q.as_deref().map(str::trim).filter(|q| !q.is_empty());
    let rows = sqlx::query_as::<_, StockLevelRow>(&format!(
        "SELECT {LEVEL_COLUMNS} {LEVEL_FROM} ORDER BY p.title, v.title NULLS FIRST, l.name \
         LIMIT $5 OFFSET $6"
    ))
    .bind(filter.location_id)
    .bind(filter.product_id)
    .bind(q)
    .bind(filter.low_only)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) {LEVEL_FROM}"))
        .bind(filter.location_id)
        .bind(filter.product_id)
        .bind(q)
        .bind(filter.low_only)
        .fetch_one(pool)
        .await?;
    Ok((rows, total))
}

/// One stock level by id
pub async fn get_level(pool: &PgPool, id: Uuid) -> Result<Option<StockLevelRow>> {
    let row = sqlx::query_as::<_, StockLevelRow>(&format!(
        "SELECT {LEVEL_COLUMNS} FROM inventory_levels lv \
         JOIN products p ON p.id = lv.product_id \
         JOIN inventory_locations l ON l.id = lv.location_id \
         LEFT JOIN product_variants v ON v.id = lv.variant_id WHERE lv.id = $1"
    ))
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// What a stock change applies to
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct StockKey {
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub location_id: Uuid,
}

/// Who changed stock and why, recorded on the movement
#[derive(Debug, Clone, Default)]
pub struct MovementNote {
    pub reason: Option<String>,
    pub reference: Option<String>,
    pub notes: Option<String>,
    pub cost_per_unit: Option<Decimal>,
    pub actor: Option<Uuid>,
}

/// A manual stock adjustment
#[derive(Debug, Clone, Deserialize)]
pub struct AdjustmentInput {
    #[serde(flatten)]
    pub key: StockKey,
    /// Units to add (positive) or remove (negative); never 0
    pub quantity: i32,
    pub reason: AdjustmentReason,
    pub notes: Option<String>,
}

/// Apply a manual adjustment; refused when it would take free stock below
/// zero. Returns the updated stock level.
pub async fn adjust(pool: &PgPool, input: &AdjustmentInput, actor: Uuid) -> Result<StockLevelRow> {
    if input.quantity == 0 {
        return Err(Error::validation("quantity must not be 0"));
    }
    if !input.reason.allows(input.quantity) {
        return Err(Error::validation(format!(
            "a {} adjustment cannot {} stock",
            input.reason.as_str(),
            if input.quantity < 0 { "remove" } else { "add" }
        )));
    }
    check_lengths(input.notes.as_deref(), None)?;
    let note = MovementNote {
        reason: Some(input.reason.as_str().to_string()),
        notes: input.notes.clone(),
        actor: Some(actor),
        ..Default::default()
    };
    let mut tx = pool.begin().await?;
    let movement = input.reason.movement_type(input.quantity);
    let level_id = apply_change(&mut tx, input.key, input.quantity, movement, &note).await?;
    tx.commit().await?;
    get_level(pool, level_id)
        .await?
        .ok_or_else(|| Error::not_found("Stock level not found"))
}

/// One line of a goods receipt
#[derive(Debug, Clone, Deserialize)]
pub struct ReceiptLine {
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    /// Units received; more than 0
    pub quantity: i32,
    pub cost_per_unit: Option<Decimal>,
}

/// A delivery booked into one location
#[derive(Debug, Clone, Deserialize)]
pub struct ReceiptInput {
    pub location_id: Uuid,
    /// Supplier delivery note or purchase order number
    pub reference: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<ReceiptLine>,
}

/// At most this many lines per receipt
pub const MAX_RECEIPT_LINES: usize = 500;

/// Book a goods receipt: all lines or none. Returns the updated stock levels,
/// in line order.
pub async fn receive(
    pool: &PgPool,
    input: &ReceiptInput,
    actor: Uuid,
) -> Result<Vec<StockLevelRow>> {
    if input.lines.is_empty() || input.lines.len() > MAX_RECEIPT_LINES {
        return Err(Error::validation(format!(
            "a receipt has 1 to {MAX_RECEIPT_LINES} lines"
        )));
    }
    if let Some(line) = input.lines.iter().find(|l| l.quantity <= 0) {
        return Err(Error::validation(format!(
            "received quantity must be positive (product {})",
            line.product_id
        )));
    }
    check_lengths(input.notes.as_deref(), input.reference.as_deref())?;
    let mut tx = pool.begin().await?;
    let mut level_ids = Vec::with_capacity(input.lines.len());
    for line in &input.lines {
        let key = StockKey {
            product_id: line.product_id,
            variant_id: line.variant_id,
            location_id: input.location_id,
        };
        let note = MovementNote {
            reason: Some("receipt".to_string()),
            reference: input.reference.clone(),
            notes: input.notes.clone(),
            cost_per_unit: line.cost_per_unit,
            actor: Some(actor),
        };
        level_ids
            .push(apply_change(&mut tx, key, line.quantity, StockMovementType::In, &note).await?);
    }
    tx.commit().await?;
    levels_in_order(pool, &level_ids).await
}

/// The stock levels with these ids, in the given order
async fn levels_in_order(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<StockLevelRow>> {
    let mut rows = Vec::with_capacity(ids.len());
    for id in ids {
        if let Some(row) = get_level(pool, *id).await? {
            rows.push(row);
        }
    }
    Ok(rows)
}

/// Limits on free text recorded with a movement
fn check_lengths(notes: Option<&str>, reference: Option<&str>) -> Result<()> {
    if notes.is_some_and(|n| n.chars().count() > 2000) {
        return Err(Error::validation("notes must be at most 2000 characters"));
    }
    if reference.is_some_and(|r| r.chars().count() > 255) {
        return Err(Error::validation(
            "reference must be at most 255 characters",
        ));
    }
    Ok(())
}

/// Change free stock of one key by `quantity` inside `tx`: lock the key,
/// update or create its level row, keep the product/variant total in step and
/// record the movement. Returns the level id.
pub async fn apply_change(
    tx: &mut Transaction<'_, Postgres>,
    key: StockKey,
    quantity: i32,
    movement: StockMovementType,
    note: &MovementNote,
) -> Result<Uuid> {
    check_key(tx, key).await?;
    sqlx::query(
        "SELECT pg_advisory_xact_lock(hashtext('rcommerce:stock:' || $1::text || ':' || \
         COALESCE($2::text, '') || ':' || $3::text))",
    )
    .bind(key.product_id)
    .bind(key.variant_id)
    .bind(key.location_id)
    .execute(&mut **tx)
    .await?;
    let level_id = upsert_level(tx, key, quantity, note.cost_per_unit).await?;
    update_totals(tx, key, quantity).await?;
    record_movement(tx, key, quantity, movement, note).await?;
    Ok(level_id)
}

/// The product, variant (of that product) and location must exist
async fn check_key(tx: &mut Transaction<'_, Postgres>, key: StockKey) -> Result<()> {
    let (product, variant_ok, location): (bool, bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM products WHERE id = $1), \
                ($2::uuid IS NULL OR EXISTS (SELECT 1 FROM product_variants WHERE id = $2 AND product_id = $1)), \
                EXISTS (SELECT 1 FROM inventory_locations WHERE id = $3)",
    )
    .bind(key.product_id)
    .bind(key.variant_id)
    .bind(key.location_id)
    .fetch_one(&mut **tx)
    .await?;
    if !product {
        return Err(Error::not_found(format!(
            "Product {} not found",
            key.product_id
        )));
    }
    if !variant_ok {
        return Err(Error::not_found("Variant not found for this product"));
    }
    if !location {
        return Err(Error::not_found(format!(
            "Location {} not found",
            key.location_id
        )));
    }
    Ok(())
}

/// Add `quantity` to the key's free stock, creating the row for a first
/// receipt; refused when free stock would go below zero
async fn upsert_level(
    tx: &mut Transaction<'_, Postgres>,
    key: StockKey,
    quantity: i32,
    cost: Option<Decimal>,
) -> Result<Uuid> {
    let current: Option<(Uuid, i32)> = sqlx::query_as(
        "SELECT id, available_quantity FROM inventory_levels \
         WHERE product_id = $1 AND variant_id IS NOT DISTINCT FROM $2 AND location_id = $3 \
         ORDER BY created_at, id LIMIT 1",
    )
    .bind(key.product_id)
    .bind(key.variant_id)
    .bind(key.location_id)
    .fetch_optional(&mut **tx)
    .await?;
    let free = current.map_or(0, |(_, available)| available);
    let Some(after) = free.checked_add(quantity).filter(|after| *after >= 0) else {
        return Err(Error::validation(format!(
            "only {free} unit(s) in stock here; cannot remove {}",
            -quantity
        )));
    };
    match current {
        Some((id, _)) => {
            sqlx::query(
                "UPDATE inventory_levels SET available_quantity = $2,                  cost_per_unit = COALESCE($3, cost_per_unit), updated_at = NOW() WHERE id = $1",
            )
            .bind(id)
            .bind(after)
            .bind(cost)
            .execute(&mut **tx)
            .await?;
            Ok(id)
        }
        None => insert_level(tx, key, after, cost).await,
    }
}

/// The key's first stock level row
async fn insert_level(
    tx: &mut Transaction<'_, Postgres>,
    key: StockKey,
    available: i32,
    cost: Option<Decimal>,
) -> Result<Uuid> {
    Ok(sqlx::query_scalar(
        "INSERT INTO inventory_levels (product_id, variant_id, location_id, available_quantity, cost_per_unit)          VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(key.product_id)
    .bind(key.variant_id)
    .bind(key.location_id)
    .bind(available)
    .bind(cost)
    .fetch_one(&mut **tx)
    .await?)
}

/// Keep `inventory_quantity` of the product (and variant) equal to the change
async fn update_totals(
    tx: &mut Transaction<'_, Postgres>,
    key: StockKey,
    quantity: i32,
) -> Result<()> {
    sqlx::query(
        "UPDATE products SET inventory_quantity = inventory_quantity + $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(key.product_id)
    .bind(quantity)
    .execute(&mut **tx)
    .await?;
    if let Some(variant_id) = key.variant_id {
        sqlx::query(
            "UPDATE product_variants SET inventory_quantity = inventory_quantity + $2, updated_at = NOW() \
             WHERE id = $1",
        )
        .bind(variant_id)
        .bind(quantity)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn record_movement(
    tx: &mut Transaction<'_, Postgres>,
    key: StockKey,
    quantity: i32,
    movement: StockMovementType,
    note: &MovementNote,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO stock_movements (product_id, variant_id, location_id, quantity, movement_type, \
         cost_per_unit, reference, notes, reason, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(key.product_id)
    .bind(key.variant_id)
    .bind(key.location_id)
    .bind(quantity)
    .bind(movement)
    .bind(note.cost_per_unit)
    .bind(note.reference.as_deref())
    .bind(note.notes.as_deref())
    .bind(note.reason.as_deref())
    .bind(note.actor)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// New reorder settings of a stock level
#[derive(Debug, Clone, Deserialize)]
pub struct ReorderInput {
    /// At or below this free stock the row is "low"
    pub reorder_point: i32,
    /// Suggested quantity to order
    pub reorder_quantity: i32,
}

/// Change a level's reorder point and quantity
pub async fn set_reorder(
    pool: &PgPool,
    level_id: Uuid,
    input: &ReorderInput,
) -> Result<StockLevelRow> {
    if input.reorder_point < 0 || input.reorder_quantity < 0 {
        return Err(Error::validation(
            "reorder point and quantity must not be negative",
        ));
    }
    let updated = sqlx::query(
        "UPDATE inventory_levels SET reorder_point = $2, reorder_quantity = $3, updated_at = NOW() \
         WHERE id = $1",
    )
    .bind(level_id)
    .bind(input.reorder_point)
    .bind(input.reorder_quantity)
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(Error::not_found("Stock level not found"));
    }
    get_level(pool, level_id)
        .await?
        .ok_or_else(|| Error::not_found("Stock level not found"))
}

/// One entry of the movement history
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct MovementRow {
    pub id: Uuid,
    pub product_id: Uuid,
    pub variant_id: Option<Uuid>,
    pub sku: Option<String>,
    pub title: String,
    pub location_id: Uuid,
    pub location_code: String,
    pub quantity: i32,
    pub movement_type: String,
    pub reason: Option<String>,
    pub reference: Option<String>,
    pub notes: Option<String>,
    pub cost_per_unit: Option<Decimal>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// Filters of the movement history
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MovementFilter {
    pub location_id: Option<Uuid>,
    pub product_id: Option<Uuid>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

const MOVEMENT_FROM: &str = "FROM stock_movements m \
     JOIN products p ON p.id = m.product_id \
     JOIN inventory_locations l ON l.id = m.location_id \
     LEFT JOIN product_variants v ON v.id = m.variant_id \
     WHERE ($1::uuid IS NULL OR m.location_id = $1) AND ($2::uuid IS NULL OR m.product_id = $2) \
       AND ($3::timestamptz IS NULL OR m.created_at >= $3) AND ($4::timestamptz IS NULL OR m.created_at < $4)";

/// A page of stock movements, newest first, and the total number of matches
pub async fn list_movements(
    pool: &PgPool,
    filter: &MovementFilter,
) -> Result<(Vec<MovementRow>, i64)> {
    let per_page = filter.per_page.unwrap_or(50).clamp(1, 200);
    let offset = (filter.page.unwrap_or(1).max(1) - 1) * per_page;
    let rows = sqlx::query_as::<_, MovementRow>(&format!(
        "SELECT m.id, m.product_id, m.variant_id, COALESCE(v.sku, p.sku) AS sku, \
         CASE WHEN v.id IS NULL THEN p.title ELSE p.title || ' – ' || v.title END AS title, \
         m.location_id, l.code AS location_code, m.quantity, m.movement_type::text AS movement_type, \
         m.reason, m.reference, m.notes, m.cost_per_unit, m.created_by, m.created_at \
         {MOVEMENT_FROM} ORDER BY m.created_at DESC, m.id LIMIT $5 OFFSET $6"
    ))
    .bind(filter.location_id)
    .bind(filter.product_id)
    .bind(filter.from)
    .bind(filter.to)
    .bind(per_page)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    let total: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) {MOVEMENT_FROM}"))
        .bind(filter.location_id)
        .bind(filter.product_id)
        .bind(filter.from)
        .bind(filter.to)
        .fetch_one(pool)
        .await?;
    Ok((rows, total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_map_to_movement_types_by_direction() {
        assert_eq!(
            AdjustmentReason::Damage.movement_type(-2),
            StockMovementType::Lost
        );
        assert_eq!(
            AdjustmentReason::Found.movement_type(3),
            StockMovementType::Found
        );
        assert_eq!(
            AdjustmentReason::Count.movement_type(-1),
            StockMovementType::Out
        );
        assert_eq!(
            AdjustmentReason::Count.movement_type(4),
            StockMovementType::In
        );
        assert_eq!(
            AdjustmentReason::Correction.movement_type(-4),
            StockMovementType::Out
        );
    }

    #[test]
    fn reasons_only_move_stock_their_way() {
        assert!(!AdjustmentReason::Damage.allows(5));
        assert!(AdjustmentReason::Damage.allows(-5));
        assert!(!AdjustmentReason::Found.allows(-1));
        assert!(!AdjustmentReason::Return.allows(-1));
        assert!(AdjustmentReason::Count.allows(-1) && AdjustmentReason::Count.allows(1));
    }

    #[test]
    fn level_pages_are_bounded() {
        let filter = LevelFilter {
            page: Some(3),
            per_page: Some(500),
            ..Default::default()
        };
        assert_eq!(filter.window(), (200, 400));
        assert_eq!(LevelFilter::default().window(), (50, 0));
    }
}
