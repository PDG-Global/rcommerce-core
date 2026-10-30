use std::sync::Arc;

use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    models::{AddToCartInput, Cart, CartItem, UpdateCartItemInput},
    repository::{CouponRepository, Database},
    services::{
        cart_service::{BundleCartDetails, ProductDetails},
        CouponService,
    },
    Error, Result,
};

pub(crate) struct PostgresCartMutation {
    db: Database,
    coupon_repo: Arc<dyn CouponRepository>,
    coupon_service: Arc<CouponService>,
}

impl PostgresCartMutation {
    pub(crate) fn new(
        db: Database,
        coupon_repo: Arc<dyn CouponRepository>,
        coupon_service: Arc<CouponService>,
    ) -> Self {
        Self {
            db,
            coupon_repo,
            coupon_service,
        }
    }

    pub(crate) async fn add_item(
        &self,
        cart_id: Uuid,
        input: AddToCartInput,
        product_details: ProductDetails,
    ) -> Result<CartItem> {
        if input.quantity <= 0 {
            return Err(Error::validation("Quantity must be greater than 0"));
        }

        let mut tx = self.db.pool().begin().await?;
        lock_cart(&mut tx, cart_id).await?;
        let items = lock_items(&mut tx, cart_id).await?;
        let existing = items.iter().find(|item| {
            item.product_id == input.product_id && item.variant_id == input.variant_id
        });

        if let Some(item) = existing {
            ensure_regular(item)?;
        }

        let requested = existing
            .map_or(0, |item| item.quantity)
            .checked_add(input.quantity)
            .ok_or_else(|| Error::validation("Quantity is too large"))?;
        ensure_stock(
            &mut tx,
            input.product_id,
            input.variant_id,
            requested,
            &product_details.title,
        )
        .await?;

        let item = match existing {
            Some(item) => update_quantity(&mut tx, item, requested).await?,
            None => {
                let mut item = CartItem {
                    id: Uuid::new_v4(),
                    cart_id,
                    product_id: input.product_id,
                    variant_id: input.variant_id,
                    quantity: input.quantity,
                    unit_price: product_details.unit_price,
                    original_price: product_details.original_price,
                    subtotal: Decimal::ZERO,
                    discount_amount: Decimal::ZERO,
                    total: Decimal::ZERO,
                    sku: product_details.sku,
                    title: product_details.title,
                    variant_title: product_details.variant_title,
                    image_url: product_details.image_url,
                    requires_shipping: product_details.requires_shipping,
                    is_gift_card: product_details.is_gift_card,
                    custom_attributes: input.custom_attributes,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                ensure_regular(&item)?;
                item.calculate_totals();
                insert_item(&mut tx, &item).await?
            }
        };

        self.recalculate_tx(&mut tx, cart_id).await?;
        let item = find_item_by_id(&mut tx, item.id)
            .await?
            .ok_or_else(|| Error::not_found("Cart item not found"))?;
        tx.commit().await?;
        Ok(item)
    }

    pub(crate) async fn add_bundle_item(
        &self,
        cart_id: Uuid,
        bundle_product_id: Uuid,
        quantity: i32,
        bundle_details: BundleCartDetails,
    ) -> Result<Vec<CartItem>> {
        if quantity <= 0 {
            return Err(Error::validation("Quantity must be greater than 0"));
        }
        if bundle_details.components.is_empty() {
            return Err(Error::validation(
                "This bundle has no components configured",
            ));
        }

        let mut tx = self.db.pool().begin().await?;
        lock_cart(&mut tx, cart_id).await?;
        let items = lock_items(&mut tx, cart_id).await?;
        let parent = items
            .iter()
            .find(|item| item.product_id == bundle_product_id && item.variant_id.is_none());

        if let Some(item) = parent {
            ensure_bundle_parent(item, bundle_product_id)?;
        }
        let parent_quantity = parent
            .map_or(0, |item| item.quantity)
            .checked_add(quantity)
            .ok_or_else(|| Error::validation("Quantity is too large"))?;

        for component in &bundle_details.components {
            if component.quantity <= 0 {
                return Err(Error::validation(
                    "Bundle component quantity must be positive",
                ));
            }
            let existing = items
                .iter()
                .find(|item| item.product_id == component.product_id && item.variant_id.is_none());
            if let Some(item) = existing {
                ensure_bundle_component(item, bundle_product_id)?;
            }
            let increment = component
                .quantity
                .checked_mul(quantity)
                .ok_or_else(|| Error::validation("Bundle component quantity is too large"))?;
            let requested = existing
                .map_or(0, |item| item.quantity)
                .checked_add(increment)
                .ok_or_else(|| Error::validation("Bundle component quantity is too large"))?;
            ensure_stock(
                &mut tx,
                component.product_id,
                None,
                requested,
                &component.title,
            )
            .await?;
        }

        let parent_id = parent.map_or_else(Uuid::new_v4, |item| item.id);
        if let Some(item) = parent {
            update_quantity(&mut tx, item, parent_quantity).await?;
        } else {
            let mut item = CartItem {
                id: parent_id,
                cart_id,
                product_id: bundle_product_id,
                variant_id: None,
                quantity,
                unit_price: bundle_details.bundle_price,
                original_price: bundle_details.original_price,
                subtotal: Decimal::ZERO,
                discount_amount: Decimal::ZERO,
                total: Decimal::ZERO,
                sku: bundle_details.sku,
                title: bundle_details.title,
                variant_title: Some("Bundle".to_string()),
                image_url: bundle_details.image_url,
                requires_shipping: bundle_details.requires_shipping,
                is_gift_card: false,
                custom_attributes: Some(serde_json::json!({
                    "is_bundle_parent": true,
                    "bundle_product_id": bundle_product_id.to_string(),
                })),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            item.calculate_totals();
            insert_item(&mut tx, &item).await?;
        }

        for component in &bundle_details.components {
            let existing = items
                .iter()
                .find(|item| item.product_id == component.product_id && item.variant_id.is_none());
            let increment = component.quantity * quantity;
            if let Some(item) = existing {
                let requested = item.quantity + increment;
                update_quantity(&mut tx, item, requested).await?;
            } else {
                let mut item = CartItem {
                    id: Uuid::new_v4(),
                    cart_id,
                    product_id: component.product_id,
                    variant_id: None,
                    quantity: increment,
                    unit_price: component.unit_price,
                    original_price: component.unit_price,
                    subtotal: Decimal::ZERO,
                    discount_amount: Decimal::ZERO,
                    total: Decimal::ZERO,
                    sku: component.sku.clone(),
                    title: component.title.clone(),
                    variant_title: Some(format!("Bundle Component ({}x)", component.quantity)),
                    image_url: component.image_url.clone(),
                    requires_shipping: component.requires_shipping,
                    is_gift_card: false,
                    custom_attributes: Some(serde_json::json!({
                        "bundle_parent_id": bundle_product_id.to_string(),
                        "is_bundle_component": true,
                    })),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                };
                item.calculate_totals();
                insert_item(&mut tx, &item).await?;
            }
        }

        self.recalculate_tx(&mut tx, cart_id).await?;
        let mut stored: Vec<_> = lock_items(&mut tx, cart_id)
            .await?
            .into_iter()
            .filter(|item| {
                item.id == parent_id
                    || (is_bundle_component(item)
                        && bundle_component_parent_id(item).ok() == Some(bundle_product_id))
            })
            .collect();
        tx.commit().await?;
        stored.sort_by_key(|item| (item.id != parent_id, item.created_at));
        Ok(stored)
    }

    pub(crate) async fn update_item(
        &self,
        cart_id: Uuid,
        item_id: Uuid,
        input: UpdateCartItemInput,
    ) -> Result<CartItem> {
        if input.quantity < 0 {
            return Err(Error::validation("Quantity cannot be negative"));
        }

        let mut tx = self.db.pool().begin().await?;
        lock_cart(&mut tx, cart_id).await?;
        let items = lock_items(&mut tx, cart_id).await?;
        let item = items
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| Error::not_found("Cart item not found"))?;
        let original = item.clone();

        if is_bundle_component(item) {
            return Err(Error::validation(
                "Bundle components must be updated through their parent item",
            ));
        }

        if is_bundle_parent(item) {
            let parent_product_id = bundle_parent_product_id(item)?;
            let children: Vec<_> = items
                .iter()
                .filter(|child| {
                    is_bundle_component(child)
                        && bundle_component_parent_id(child).ok() == Some(parent_product_id)
                })
                .collect();

            if input.quantity == 0 {
                for child in children {
                    sqlx::query("DELETE FROM cart_items WHERE id = $1")
                        .bind(child.id)
                        .execute(&mut *tx)
                        .await?;
                }
            } else {
                if original.quantity <= 0 {
                    return Err(Error::internal("Bundle parent has an invalid quantity"));
                }
                for child in children {
                    if child.quantity % original.quantity != 0 {
                        return Err(Error::internal("Bundle component quantity is inconsistent"));
                    }
                    let multiplier = child.quantity / original.quantity;
                    let requested = multiplier.checked_mul(input.quantity).ok_or_else(|| {
                        Error::validation("Bundle component quantity is too large")
                    })?;
                    ensure_stock(
                        &mut tx,
                        child.product_id,
                        child.variant_id,
                        requested,
                        &child.title,
                    )
                    .await?;
                }
                for child in items.iter().filter(|child| {
                    is_bundle_component(child)
                        && bundle_component_parent_id(child).ok() == Some(parent_product_id)
                }) {
                    let multiplier = child.quantity / original.quantity;
                    update_quantity(&mut tx, child, multiplier * input.quantity).await?;
                }
            }
        } else if input.quantity > 0 {
            ensure_stock(
                &mut tx,
                item.product_id,
                item.variant_id,
                input.quantity,
                &item.title,
            )
            .await?;
        }

        if input.quantity == 0 {
            sqlx::query("DELETE FROM cart_items WHERE id = $1")
                .bind(item_id)
                .execute(&mut *tx)
                .await?;
        } else {
            update_quantity(&mut tx, item, input.quantity).await?;
        }

        self.recalculate_tx(&mut tx, cart_id).await?;
        let stored = if input.quantity == 0 {
            original
        } else {
            find_item_by_id(&mut tx, item_id)
                .await?
                .ok_or_else(|| Error::not_found("Cart item not found"))?
        };
        tx.commit().await?;
        Ok(stored)
    }

    pub(crate) async fn remove_item(&self, cart_id: Uuid, item_id: Uuid) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        lock_cart(&mut tx, cart_id).await?;
        let items = lock_items(&mut tx, cart_id).await?;
        let item = items
            .iter()
            .find(|item| item.id == item_id)
            .ok_or_else(|| Error::not_found("Cart item not found"))?;

        if is_bundle_component(item) {
            return Err(Error::validation(
                "Bundle components must be removed through their parent item",
            ));
        }
        if is_bundle_parent(item) {
            let parent_product_id = bundle_parent_product_id(item)?;
            for child in items.iter().filter(|child| {
                is_bundle_component(child)
                    && bundle_component_parent_id(child).ok() == Some(parent_product_id)
            }) {
                sqlx::query("DELETE FROM cart_items WHERE id = $1")
                    .bind(child.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query("DELETE FROM cart_items WHERE id = $1")
            .bind(item_id)
            .execute(&mut *tx)
            .await?;
        self.recalculate_tx(&mut tx, cart_id).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn clear_cart(&self, cart_id: Uuid) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        lock_cart(&mut tx, cart_id).await?;
        sqlx::query("DELETE FROM cart_items WHERE cart_id = $1")
            .bind(cart_id)
            .execute(&mut *tx)
            .await?;
        self.recalculate_tx(&mut tx, cart_id).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn recalculate_cart(&self, cart_id: Uuid) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        lock_cart(&mut tx, cart_id).await?;
        self.recalculate_tx(&mut tx, cart_id).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn recalculate_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        cart_id: Uuid,
    ) -> Result<()> {
        let cart = lock_cart(tx, cart_id).await?;
        let mut items = lock_items(tx, cart_id).await?;
        let subtotal: Decimal = items
            .iter()
            .filter(|item| !is_bundle_component(item))
            .map(|item| item.subtotal)
            .sum();
        let discount_total = if let Some(coupon_code) = &cart.coupon_code {
            if let Some(coupon) = self.coupon_repo.find_by_code(coupon_code).await? {
                self.coupon_service
                    .calculate_discount(&coupon, subtotal, &items)
                    .await?
            } else {
                Decimal::ZERO
            }
        } else {
            Decimal::ZERO
        };

        for item in items.iter_mut().filter(|item| !is_bundle_component(item)) {
            let ratio = if subtotal > Decimal::ZERO {
                item.subtotal / subtotal
            } else {
                Decimal::ZERO
            };
            item.discount_amount = (discount_total * ratio).round_dp(2);
            item.total = item.subtotal - item.discount_amount;
            sqlx::query(
                "UPDATE cart_items SET discount_amount = $2, total = $3, updated_at = NOW() WHERE id = $1",
            )
            .bind(item.id)
            .bind(item.discount_amount)
            .bind(item.total)
            .execute(&mut **tx)
            .await?;
        }

        let total = subtotal - discount_total + cart.shipping_total;
        sqlx::query(
            "UPDATE carts SET subtotal = $2, discount_total = $3, tax_total = 0, total = $4, updated_at = NOW() WHERE id = $1",
        )
        .bind(cart_id)
        .bind(subtotal)
        .bind(discount_total)
        .bind(total)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

async fn lock_cart(tx: &mut Transaction<'_, Postgres>, cart_id: Uuid) -> Result<Cart> {
    let cart = sqlx::query_as::<_, Cart>("SELECT * FROM carts WHERE id = $1 FOR UPDATE")
        .bind(cart_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| Error::not_found("Cart not found"))?;
    if cart.converted_to_order {
        return Err(Error::validation(
            "Cart has already been converted to an order",
        ));
    }
    Ok(cart)
}

async fn lock_items(tx: &mut Transaction<'_, Postgres>, cart_id: Uuid) -> Result<Vec<CartItem>> {
    Ok(sqlx::query_as::<_, CartItem>(
        "SELECT * FROM cart_items WHERE cart_id = $1 ORDER BY created_at DESC FOR UPDATE",
    )
    .bind(cart_id)
    .fetch_all(&mut **tx)
    .await?)
}

async fn find_item_by_id(
    tx: &mut Transaction<'_, Postgres>,
    item_id: Uuid,
) -> Result<Option<CartItem>> {
    Ok(
        sqlx::query_as::<_, CartItem>("SELECT * FROM cart_items WHERE id = $1")
            .bind(item_id)
            .fetch_optional(&mut **tx)
            .await?,
    )
}

async fn insert_item(tx: &mut Transaction<'_, Postgres>, item: &CartItem) -> Result<CartItem> {
    Ok(sqlx::query_as::<_, CartItem>(
        r#"
        INSERT INTO cart_items (
            id, cart_id, product_id, variant_id, quantity, unit_price, original_price,
            subtotal, discount_amount, total, sku, title, variant_title, image_url,
            requires_shipping, is_gift_card, custom_attributes, created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19)
        RETURNING *
        "#,
    )
    .bind(item.id)
    .bind(item.cart_id)
    .bind(item.product_id)
    .bind(item.variant_id)
    .bind(item.quantity)
    .bind(item.unit_price)
    .bind(item.original_price)
    .bind(item.subtotal)
    .bind(item.discount_amount)
    .bind(item.total)
    .bind(&item.sku)
    .bind(&item.title)
    .bind(&item.variant_title)
    .bind(&item.image_url)
    .bind(item.requires_shipping)
    .bind(item.is_gift_card)
    .bind(&item.custom_attributes)
    .bind(item.created_at)
    .bind(item.updated_at)
    .fetch_one(&mut **tx)
    .await?)
}

async fn update_quantity(
    tx: &mut Transaction<'_, Postgres>,
    item: &CartItem,
    quantity: i32,
) -> Result<CartItem> {
    let subtotal = item.unit_price * Decimal::from(quantity);
    let total = subtotal - item.discount_amount;
    Ok(sqlx::query_as::<_, CartItem>(
        "UPDATE cart_items SET quantity = $2, subtotal = $3, total = $4, updated_at = NOW() WHERE id = $1 RETURNING *",
    )
    .bind(item.id)
    .bind(quantity)
    .bind(subtotal)
    .bind(total)
    .fetch_one(&mut **tx)
    .await?)
}

async fn ensure_stock(
    tx: &mut Transaction<'_, Postgres>,
    product_id: Uuid,
    variant_id: Option<Uuid>,
    requested: i32,
    title: &str,
) -> Result<()> {
    let available: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(available_quantity), 0) FROM inventory_levels WHERE product_id = $1 AND variant_id IS NOT DISTINCT FROM $2",
    )
    .bind(product_id)
    .bind(variant_id)
    .fetch_one(&mut **tx)
    .await?;
    if i64::from(requested) > available {
        return Err(Error::validation(format!(
            "Only {} units of '{}' are available",
            available.max(0),
            title
        )));
    }
    Ok(())
}

fn ensure_regular(item: &CartItem) -> Result<()> {
    if is_bundle_parent(item) || is_bundle_component(item) {
        return Err(Error::validation(
            "This product already belongs to a bundle in the cart",
        ));
    }
    Ok(())
}

fn ensure_bundle_parent(item: &CartItem, bundle_product_id: Uuid) -> Result<()> {
    if !is_bundle_parent(item) || bundle_parent_product_id(item)? != bundle_product_id {
        return Err(Error::validation(
            "The bundle conflicts with an existing cart item",
        ));
    }
    Ok(())
}

fn ensure_bundle_component(item: &CartItem, bundle_product_id: Uuid) -> Result<()> {
    if !is_bundle_component(item) || bundle_component_parent_id(item)? != bundle_product_id {
        return Err(Error::validation(
            "A bundle component conflicts with an existing cart item",
        ));
    }
    Ok(())
}

fn is_bundle_parent(item: &CartItem) -> bool {
    item.custom_attributes
        .as_ref()
        .and_then(|attributes| attributes.get("is_bundle_parent"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn is_bundle_component(item: &CartItem) -> bool {
    item.custom_attributes
        .as_ref()
        .and_then(|attributes| attributes.get("is_bundle_component"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn bundle_parent_product_id(item: &CartItem) -> Result<Uuid> {
    item.custom_attributes
        .as_ref()
        .and_then(|attributes| attributes.get("bundle_product_id"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| Error::internal("Bundle parent metadata is missing"))?
        .parse()
        .map_err(|_| Error::internal("Bundle parent metadata is invalid"))
}

fn bundle_component_parent_id(item: &CartItem) -> Result<Uuid> {
    item.custom_attributes
        .as_ref()
        .and_then(|attributes| attributes.get("bundle_parent_id"))
        .and_then(|value| value.as_str())
        .ok_or_else(|| Error::internal("Bundle component metadata is missing"))?
        .parse()
        .map_err(|_| Error::internal("Bundle component metadata is invalid"))
}
