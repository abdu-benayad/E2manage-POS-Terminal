//! Products Repository
//!
//! Handles product queries including FTS5 full-text search.

use rusqlite::{params, OptionalExtension, Result as SqliteResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{decimal_from_sqlite, decimal_to_sqlite, Database};

/// Product row from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductRow {
    pub id: String,
    pub sku: String,
    pub barcode: Option<String>,
    pub name: String,
    pub name_ar: Option<String>,
    pub description: Option<String>,
    pub price: Decimal,
    pub cost: Decimal,
    pub tax_rate: Decimal,
    pub tax_inclusive: bool,
    pub category_id: Option<String>,
    pub category_name: Option<String>,
    pub unit: String,
    pub stock_qty: i32,
    pub min_stock: i32,
    pub allow_negative_stock: bool,
    pub image_url: Option<String>,
    pub is_weighable: bool,
    pub is_serialized: bool,
    pub is_active: bool,
    /// Product type classification (Phase 3 Track H)
    pub product_type: String,
    /// Whether this product tracks inventory
    pub track_inventory: bool,
    /// Product nature: TANGIBLE, INTANGIBLE, HYBRID
    pub product_nature: String,
}

impl Default for ProductRow {
    fn default() -> Self {
        Self {
            id: String::new(),
            sku: String::new(),
            barcode: None,
            name: String::new(),
            name_ar: None,
            description: None,
            price: Decimal::ZERO,
            cost: Decimal::ZERO,
            tax_rate: Decimal::ZERO,
            tax_inclusive: false,
            category_id: None,
            category_name: None,
            unit: "UNIT".to_string(),
            stock_qty: 0,
            min_stock: 0,
            allow_negative_stock: false,
            image_url: None,
            is_weighable: false,
            is_serialized: false,
            is_active: true,
            product_type: "PHYSICAL_GOOD".to_string(),
            track_inventory: true,
            product_nature: "TANGIBLE".to_string(),
        }
    }
}

/// Category row from database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryRow {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub name_ar: Option<String>,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub image_url: Option<String>,
    pub display_order: i32,
    pub is_active: bool,
}

impl Database {
    // ========================================================================
    // PRODUCT OPERATIONS
    // ========================================================================

    /// Saves or updates a product
    pub fn save_product(&self, product: &ProductRow) -> SqliteResult<()> {
        self.execute(
            r#"INSERT OR REPLACE INTO products
               (id, sku, barcode, name, name_ar, description, price, cost, tax_rate,
                tax_inclusive, category_id, category_name, unit, stock_qty, min_stock,
                allow_negative_stock, image_url, is_weighable, is_serialized, is_active,
                product_type, track_inventory, product_nature, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, datetime('now'))"#,
            &[
                &product.id,
                &product.sku,
                &product.barcode,
                &product.name,
                &product.name_ar,
                &product.description,
                &decimal_to_sqlite(&product.price),
                &decimal_to_sqlite(&product.cost),
                &decimal_to_sqlite(&product.tax_rate),
                &product.tax_inclusive,
                &product.category_id,
                &product.category_name,
                &product.unit,
                &product.stock_qty,
                &product.min_stock,
                &product.allow_negative_stock,
                &product.image_url,
                &product.is_weighable,
                &product.is_serialized,
                &product.is_active,
                &product.product_type,
                &product.track_inventory,
                &product.product_nature,
            ],
        )?;
        Ok(())
    }

    /// Bulk saves products (more efficient for sync)
    pub fn save_products(&self, products: &[ProductRow]) -> SqliteResult<usize> {
        let conn = self.connection();
        let conn = conn.lock();

        let tx = conn.unchecked_transaction()?;
        let mut count = 0;

        {
            let mut stmt = conn.prepare(
                r#"INSERT OR REPLACE INTO products
                   (id, sku, barcode, name, name_ar, description, price, cost, tax_rate,
                    tax_inclusive, category_id, category_name, unit, stock_qty, min_stock,
                    allow_negative_stock, image_url, is_weighable, is_serialized, is_active,
                    product_type, track_inventory, product_nature, updated_at)
                   VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, datetime('now'))"#,
            )?;

            for product in products {
                stmt.execute(params![
                    product.id,
                    product.sku,
                    product.barcode,
                    product.name,
                    product.name_ar,
                    product.description,
                    decimal_to_sqlite(&product.price),
                    decimal_to_sqlite(&product.cost),
                    decimal_to_sqlite(&product.tax_rate),
                    product.tax_inclusive,
                    product.category_id,
                    product.category_name,
                    product.unit,
                    product.stock_qty,
                    product.min_stock,
                    product.allow_negative_stock,
                    product.image_url,
                    product.is_weighable,
                    product.is_serialized,
                    product.is_active,
                    product.product_type,
                    product.track_inventory,
                    product.product_nature,
                ])?;
                count += 1;
            }
        }

        tx.commit()?;
        Ok(count)
    }

    /// Searches products using FTS5 (fast full-text search)
    pub fn search_products_fts(&self, query: &str, limit: i32) -> SqliteResult<Vec<ProductRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        // Use FTS5 for fast searching
        let mut stmt = conn.prepare(
            r#"SELECT p.id, p.sku, p.barcode, p.name, p.name_ar, p.description, p.price, p.cost,
                      p.tax_rate, p.tax_inclusive, p.category_id, p.category_name, p.unit,
                      p.stock_qty, p.min_stock, p.allow_negative_stock, p.image_url,
                      p.is_weighable, p.is_serialized, p.is_active,
                      p.product_type, p.track_inventory, p.product_nature
               FROM products p
               JOIN products_fts fts ON p.rowid = fts.rowid
               WHERE products_fts MATCH ?1 AND p.is_active = 1
               ORDER BY rank
               LIMIT ?2"#,
        )?;

        // Format query for FTS5 - escape special characters and add prefix matching
        let fts_query = format!("{}*", query.replace('"', "\"\""));

        let rows = stmt.query_map(params![fts_query, limit], |row| {
            Ok(ProductRow {
                id: row.get(0)?,
                sku: row.get(1)?,
                barcode: row.get(2)?,
                name: row.get(3)?,
                name_ar: row.get(4)?,
                description: row.get(5)?,
                price: decimal_from_sqlite(row.get::<_, f64>(6)?),
                cost: decimal_from_sqlite(row.get::<_, f64>(7)?),
                tax_rate: decimal_from_sqlite(row.get::<_, f64>(8)?),
                tax_inclusive: row.get(9)?,
                category_id: row.get(10)?,
                category_name: row.get(11)?,
                unit: row.get(12)?,
                stock_qty: row.get(13)?,
                min_stock: row.get(14)?,
                allow_negative_stock: row.get(15)?,
                image_url: row.get(16)?,
                is_weighable: row.get(17)?,
                is_serialized: row.get(18)?,
                is_active: row.get(19)?,
                product_type: row
                    .get::<_, Option<String>>(20)?
                    .unwrap_or_else(|| "PHYSICAL_GOOD".to_string()),
                track_inventory: row.get::<_, Option<bool>>(21)?.unwrap_or(true),
                product_nature: row
                    .get::<_, Option<String>>(22)?
                    .unwrap_or_else(|| "TANGIBLE".to_string()),
            })
        })?;

        rows.collect()
    }

    /// Searches products using LIKE (fallback for partial/fuzzy matching)
    pub fn search_products(&self, query: &str, limit: i32) -> SqliteResult<Vec<ProductRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, sku, barcode, name, name_ar, description, price, cost,
                      tax_rate, tax_inclusive, category_id, category_name, unit,
                      stock_qty, min_stock, allow_negative_stock, image_url,
                      is_weighable, is_serialized, is_active,
                      product_type, track_inventory, product_nature
               FROM products
               WHERE is_active = 1
                 AND (name LIKE ?1 OR name_ar LIKE ?1 OR sku LIKE ?1 OR barcode LIKE ?1)
               ORDER BY name
               LIMIT ?2"#,
        )?;

        let search = format!("%{}%", query);
        let rows = stmt.query_map(params![search, limit], |row| {
            Ok(ProductRow {
                id: row.get(0)?,
                sku: row.get(1)?,
                barcode: row.get(2)?,
                name: row.get(3)?,
                name_ar: row.get(4)?,
                description: row.get(5)?,
                price: decimal_from_sqlite(row.get::<_, f64>(6)?),
                cost: decimal_from_sqlite(row.get::<_, f64>(7)?),
                tax_rate: decimal_from_sqlite(row.get::<_, f64>(8)?),
                tax_inclusive: row.get(9)?,
                category_id: row.get(10)?,
                category_name: row.get(11)?,
                unit: row.get(12)?,
                stock_qty: row.get(13)?,
                min_stock: row.get(14)?,
                allow_negative_stock: row.get(15)?,
                image_url: row.get(16)?,
                is_weighable: row.get(17)?,
                is_serialized: row.get(18)?,
                is_active: row.get(19)?,
                product_type: row
                    .get::<_, Option<String>>(20)?
                    .unwrap_or_else(|| "PHYSICAL_GOOD".to_string()),
                track_inventory: row.get::<_, Option<bool>>(21)?.unwrap_or(true),
                product_nature: row
                    .get::<_, Option<String>>(22)?
                    .unwrap_or_else(|| "TANGIBLE".to_string()),
            })
        })?;

        rows.collect()
    }

    /// Gets a product by exact barcode match
    pub fn get_product_by_barcode(&self, barcode: &str) -> SqliteResult<Option<ProductRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, sku, barcode, name, name_ar, description, price, cost,
                      tax_rate, tax_inclusive, category_id, category_name, unit,
                      stock_qty, min_stock, allow_negative_stock, image_url,
                      is_weighable, is_serialized, is_active,
                      product_type, track_inventory, product_nature
               FROM products WHERE barcode = ?1 AND is_active = 1"#,
            [barcode],
            |row| {
                Ok(ProductRow {
                    id: row.get(0)?,
                    sku: row.get(1)?,
                    barcode: row.get(2)?,
                    name: row.get(3)?,
                    name_ar: row.get(4)?,
                    description: row.get(5)?,
                    price: decimal_from_sqlite(row.get::<_, f64>(6)?),
                    cost: decimal_from_sqlite(row.get::<_, f64>(7)?),
                    tax_rate: decimal_from_sqlite(row.get::<_, f64>(8)?),
                    tax_inclusive: row.get(9)?,
                    category_id: row.get(10)?,
                    category_name: row.get(11)?,
                    unit: row.get(12)?,
                    stock_qty: row.get(13)?,
                    min_stock: row.get(14)?,
                    allow_negative_stock: row.get(15)?,
                    image_url: row.get(16)?,
                    is_weighable: row.get(17)?,
                    is_serialized: row.get(18)?,
                    is_active: row.get(19)?,
                    product_type: row
                        .get::<_, Option<String>>(20)?
                        .unwrap_or_else(|| "PHYSICAL_GOOD".to_string()),
                    track_inventory: row.get::<_, Option<bool>>(21)?.unwrap_or(true),
                    product_nature: row
                        .get::<_, Option<String>>(22)?
                        .unwrap_or_else(|| "TANGIBLE".to_string()),
                })
            },
        )
        .optional()
    }

    /// Gets a product by ID
    pub fn get_product_by_id(&self, id: &str) -> SqliteResult<Option<ProductRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            r#"SELECT id, sku, barcode, name, name_ar, description, price, cost,
                      tax_rate, tax_inclusive, category_id, category_name, unit,
                      stock_qty, min_stock, allow_negative_stock, image_url,
                      is_weighable, is_serialized, is_active,
                      product_type, track_inventory, product_nature
               FROM products WHERE id = ?1"#,
            [id],
            |row| {
                Ok(ProductRow {
                    id: row.get(0)?,
                    sku: row.get(1)?,
                    barcode: row.get(2)?,
                    name: row.get(3)?,
                    name_ar: row.get(4)?,
                    description: row.get(5)?,
                    price: decimal_from_sqlite(row.get::<_, f64>(6)?),
                    cost: decimal_from_sqlite(row.get::<_, f64>(7)?),
                    tax_rate: decimal_from_sqlite(row.get::<_, f64>(8)?),
                    tax_inclusive: row.get(9)?,
                    category_id: row.get(10)?,
                    category_name: row.get(11)?,
                    unit: row.get(12)?,
                    stock_qty: row.get(13)?,
                    min_stock: row.get(14)?,
                    allow_negative_stock: row.get(15)?,
                    image_url: row.get(16)?,
                    is_weighable: row.get(17)?,
                    is_serialized: row.get(18)?,
                    is_active: row.get(19)?,
                    product_type: row
                        .get::<_, Option<String>>(20)?
                        .unwrap_or_else(|| "PHYSICAL_GOOD".to_string()),
                    track_inventory: row.get::<_, Option<bool>>(21)?.unwrap_or(true),
                    product_nature: row
                        .get::<_, Option<String>>(22)?
                        .unwrap_or_else(|| "TANGIBLE".to_string()),
                })
            },
        )
        .optional()
    }

    /// Gets products by category
    pub fn get_products_by_category(
        &self,
        category_id: &str,
        limit: i32,
        offset: i32,
    ) -> SqliteResult<Vec<ProductRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, sku, barcode, name, name_ar, description, price, cost,
                      tax_rate, tax_inclusive, category_id, category_name, unit,
                      stock_qty, min_stock, allow_negative_stock, image_url,
                      is_weighable, is_serialized, is_active,
                      product_type, track_inventory, product_nature
               FROM products
               WHERE category_id = ?1 AND is_active = 1
               ORDER BY name
               LIMIT ?2 OFFSET ?3"#,
        )?;

        let rows = stmt.query_map(params![category_id, limit, offset], |row| {
            Ok(ProductRow {
                id: row.get(0)?,
                sku: row.get(1)?,
                barcode: row.get(2)?,
                name: row.get(3)?,
                name_ar: row.get(4)?,
                description: row.get(5)?,
                price: decimal_from_sqlite(row.get::<_, f64>(6)?),
                cost: decimal_from_sqlite(row.get::<_, f64>(7)?),
                tax_rate: decimal_from_sqlite(row.get::<_, f64>(8)?),
                tax_inclusive: row.get(9)?,
                category_id: row.get(10)?,
                category_name: row.get(11)?,
                unit: row.get(12)?,
                stock_qty: row.get(13)?,
                min_stock: row.get(14)?,
                allow_negative_stock: row.get(15)?,
                image_url: row.get(16)?,
                is_weighable: row.get(17)?,
                is_serialized: row.get(18)?,
                is_active: row.get(19)?,
                product_type: row
                    .get::<_, Option<String>>(20)?
                    .unwrap_or_else(|| "PHYSICAL_GOOD".to_string()),
                track_inventory: row.get::<_, Option<bool>>(21)?.unwrap_or(true),
                product_nature: row
                    .get::<_, Option<String>>(22)?
                    .unwrap_or_else(|| "TANGIBLE".to_string()),
            })
        })?;

        rows.collect()
    }

    /// Gets all active products with pagination
    pub fn get_all_products(&self, limit: i32, offset: i32) -> SqliteResult<Vec<ProductRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, sku, barcode, name, name_ar, description, price, cost,
                      tax_rate, tax_inclusive, category_id, category_name, unit,
                      stock_qty, min_stock, allow_negative_stock, image_url,
                      is_weighable, is_serialized, is_active,
                      product_type, track_inventory, product_nature
               FROM products
               WHERE is_active = 1
               ORDER BY name
               LIMIT ?1 OFFSET ?2"#,
        )?;

        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(ProductRow {
                id: row.get(0)?,
                sku: row.get(1)?,
                barcode: row.get(2)?,
                name: row.get(3)?,
                name_ar: row.get(4)?,
                description: row.get(5)?,
                price: decimal_from_sqlite(row.get::<_, f64>(6)?),
                cost: decimal_from_sqlite(row.get::<_, f64>(7)?),
                tax_rate: decimal_from_sqlite(row.get::<_, f64>(8)?),
                tax_inclusive: row.get(9)?,
                category_id: row.get(10)?,
                category_name: row.get(11)?,
                unit: row.get(12)?,
                stock_qty: row.get(13)?,
                min_stock: row.get(14)?,
                allow_negative_stock: row.get(15)?,
                image_url: row.get(16)?,
                is_weighable: row.get(17)?,
                is_serialized: row.get(18)?,
                is_active: row.get(19)?,
                product_type: row
                    .get::<_, Option<String>>(20)?
                    .unwrap_or_else(|| "PHYSICAL_GOOD".to_string()),
                track_inventory: row.get::<_, Option<bool>>(21)?.unwrap_or(true),
                product_nature: row
                    .get::<_, Option<String>>(22)?
                    .unwrap_or_else(|| "TANGIBLE".to_string()),
            })
        })?;

        rows.collect()
    }

    /// Gets the total product count
    pub fn get_product_count(&self) -> SqliteResult<i64> {
        let conn = self.connection();
        let conn = conn.lock();

        conn.query_row(
            "SELECT COUNT(*) FROM products WHERE is_active = 1",
            [],
            |row| row.get(0),
        )
    }

    /// Deletes a product by ID
    pub fn delete_product(&self, id: &str) -> SqliteResult<bool> {
        let deleted = self.execute("DELETE FROM products WHERE id = ?1", &[&id])?;
        Ok(deleted > 0)
    }

    /// Deletes multiple products by their IDs (for delta sync)
    pub fn delete_products(&self, ids: &[String]) -> SqliteResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let conn = self.connection();
        let conn = conn.lock();
        let tx = conn.unchecked_transaction()?;

        let mut deleted = 0;
        {
            let mut stmt = conn.prepare("DELETE FROM products WHERE id = ?1")?;
            for id in ids {
                deleted += stmt.execute(params![id])?;
            }
        }

        tx.commit()?;
        Ok(deleted)
    }

    /// Marks all products as inactive (soft delete for full sync)
    pub fn deactivate_all_products(&self) -> SqliteResult<usize> {
        self.execute("UPDATE products SET is_active = 0", &[])
    }

    /// Deletes multiple categories by their IDs (for delta sync)
    pub fn delete_categories(&self, ids: &[String]) -> SqliteResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let conn = self.connection();
        let conn = conn.lock();
        let tx = conn.unchecked_transaction()?;

        let mut deleted = 0;
        {
            let mut stmt = conn.prepare("DELETE FROM categories WHERE id = ?1")?;
            for id in ids {
                deleted += stmt.execute(params![id])?;
            }
        }

        tx.commit()?;
        Ok(deleted)
    }

    // ========================================================================
    // CATEGORY OPERATIONS
    // ========================================================================

    /// Saves or updates a category
    pub fn save_category(&self, category: &CategoryRow) -> SqliteResult<()> {
        self.execute(
            r#"INSERT OR REPLACE INTO categories
               (id, parent_id, name, name_ar, color, icon, image_url, display_order, is_active, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))"#,
            &[
                &category.id,
                &category.parent_id,
                &category.name,
                &category.name_ar,
                &category.color,
                &category.icon,
                &category.image_url,
                &category.display_order,
                &category.is_active,
            ],
        )?;
        Ok(())
    }

    /// Gets all categories
    pub fn get_categories(&self) -> SqliteResult<Vec<CategoryRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, parent_id, name, name_ar, color, icon, image_url, display_order, is_active
               FROM categories
               WHERE is_active = 1
               ORDER BY display_order, name"#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(CategoryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                name_ar: row.get(3)?,
                color: row.get(4)?,
                icon: row.get(5)?,
                image_url: row.get(6)?,
                display_order: row.get(7)?,
                is_active: row.get(8)?,
            })
        })?;

        rows.collect()
    }

    /// Gets root categories (no parent)
    pub fn get_root_categories(&self) -> SqliteResult<Vec<CategoryRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, parent_id, name, name_ar, color, icon, image_url, display_order, is_active
               FROM categories
               WHERE parent_id IS NULL AND is_active = 1
               ORDER BY display_order, name"#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(CategoryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                name_ar: row.get(3)?,
                color: row.get(4)?,
                icon: row.get(5)?,
                image_url: row.get(6)?,
                display_order: row.get(7)?,
                is_active: row.get(8)?,
            })
        })?;

        rows.collect()
    }

    /// Gets child categories of a parent
    pub fn get_child_categories(&self, parent_id: &str) -> SqliteResult<Vec<CategoryRow>> {
        let conn = self.connection();
        let conn = conn.lock();

        let mut stmt = conn.prepare(
            r#"SELECT id, parent_id, name, name_ar, color, icon, image_url, display_order, is_active
               FROM categories
               WHERE parent_id = ?1 AND is_active = 1
               ORDER BY display_order, name"#,
        )?;

        let rows = stmt.query_map([parent_id], |row| {
            Ok(CategoryRow {
                id: row.get(0)?,
                parent_id: row.get(1)?,
                name: row.get(2)?,
                name_ar: row.get(3)?,
                color: row.get(4)?,
                icon: row.get(5)?,
                image_url: row.get(6)?,
                display_order: row.get(7)?,
                is_active: row.get(8)?,
            })
        })?;

        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migrations::run_migrations;
    use std::str::FromStr;

    fn setup_db() -> Database {
        let db = Database::in_memory().unwrap();
        {
            let conn = db.connection();
            let conn = conn.lock();
            run_migrations(&conn).unwrap();
        }
        db
    }

    #[test]
    fn test_save_and_get_product() {
        let db = setup_db();

        let product = ProductRow {
            id: "prod-1".to_string(),
            sku: "SKU001".to_string(),
            barcode: Some("1234567890123".to_string()),
            name: "Test Product".to_string(),
            name_ar: Some("منتج اختبار".to_string()),
            price: Decimal::from_str("9.99").unwrap(),
            ..Default::default()
        };

        db.save_product(&product).unwrap();

        let found = db.get_product_by_id("prod-1").unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.name, "Test Product");
        assert_eq!(found.price, Decimal::from_str("9.99").unwrap());
    }

    #[test]
    fn test_search_products() {
        let db = setup_db();

        // Insert test products
        for i in 1..=5 {
            let product = ProductRow {
                id: format!("prod-{}", i),
                sku: format!("SKU00{}", i),
                barcode: Some(format!("123456789012{}", i)),
                name: format!("Product {}", i),
                name_ar: Some(format!("منتج {}", i)),
                price: Decimal::from(i * 10),
                ..Default::default()
            };
            db.save_product(&product).unwrap();
        }

        // Search by name
        let results = db.search_products("Product", 10).unwrap();
        assert_eq!(results.len(), 5);

        // Search by specific number
        let results = db.search_products("Product 3", 10).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_get_product_by_barcode() {
        let db = setup_db();

        let product = ProductRow {
            id: "prod-1".to_string(),
            sku: "SKU001".to_string(),
            barcode: Some("1234567890123".to_string()),
            name: "Test Product".to_string(),
            price: Decimal::from_str("9.99").unwrap(),
            ..Default::default()
        };

        db.save_product(&product).unwrap();

        let found = db.get_product_by_barcode("1234567890123").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().sku, "SKU001");

        let not_found = db.get_product_by_barcode("9999999999999").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_bulk_save_products() {
        let db = setup_db();

        let products: Vec<ProductRow> = (1..=100)
            .map(|i| ProductRow {
                id: format!("prod-{}", i),
                sku: format!("SKU{:04}", i),
                barcode: Some(format!("1234567{:06}", i)),
                name: format!("Product {}", i),
                price: Decimal::from(i),
                ..Default::default()
            })
            .collect();

        let count = db.save_products(&products).unwrap();
        assert_eq!(count, 100);

        let total = db.get_product_count().unwrap();
        assert_eq!(total, 100);
    }

    #[test]
    fn test_categories() {
        let db = setup_db();

        let category = CategoryRow {
            id: "cat-1".to_string(),
            parent_id: None,
            name: "Food".to_string(),
            name_ar: Some("طعام".to_string()),
            color: Some("#FF5722".to_string()),
            icon: Some("food".to_string()),
            image_url: None,
            display_order: 1,
            is_active: true,
        };

        db.save_category(&category).unwrap();

        let categories = db.get_categories().unwrap();
        assert_eq!(categories.len(), 1);
        assert_eq!(categories[0].name, "Food");

        let root = db.get_root_categories().unwrap();
        assert_eq!(root.len(), 1);
    }
}
