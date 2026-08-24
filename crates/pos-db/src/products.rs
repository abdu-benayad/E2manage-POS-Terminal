//! Products Repository
//!
//! Handles product queries including FTS5 full-text search.

use rusqlite::{params, Result as SqliteResult};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::column;
use crate::projection::OnConflict;
use crate::row_mapping;

use super::Database;

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

row_mapping! {
    /// Every column of `products` this till reads or writes, declared once.
    ///
    /// Six reader closures and two 24-column `INSERT` lists were eight hand-maintained orderings
    /// of the same twenty-three columns. Twelve of those reads were split across two lines by
    /// `rustfmt`, which is why a single-line grep undercounted this repository's positional reads
    /// by more than a hundred.
    ///
    /// `description_ar` is a column of the table and not a field of [`ProductRow`], so it is not
    /// here: a mapping names what the till uses. `updated_at` is `managed` — the column is
    /// `NOT NULL` with no SQL default, so the store's `datetime('now')` is load-bearing rather
    /// than decorative.
    pub const PRODUCT_ROW: RowMapping<ProductRow> = for "products" {
        id,
        sku,
        barcode                 via column::OPTIONAL_TEXT,
        name,
        name_ar                 via column::OPTIONAL_TEXT,
        description             via column::OPTIONAL_TEXT,
        price                   via column::DECIMAL,
        cost                    via column::DECIMAL,
        tax_rate                via column::DECIMAL,
        tax_inclusive,
        category_id             via column::OPTIONAL_TEXT,
        category_name           via column::OPTIONAL_TEXT,
        unit,
        stock_qty,
        min_stock,
        allow_negative_stock,
        image_url               via column::OPTIONAL_TEXT,
        is_weighable,
        is_serialized,
        is_active,
        product_type            via column::PRODUCT_TYPE_OR_PHYSICAL_GOOD,
        track_inventory         via column::TRACKS_INVENTORY_UNLESS_SAID_OTHERWISE,
        product_nature          via column::PRODUCT_NATURE_OR_TANGIBLE,
        managed "updated_at" = "datetime('now')",
    } on_conflict OnConflict::Replace;
}

row_mapping! {
    /// Every column of `categories` this till reads or writes, declared once.
    pub const CATEGORY_ROW: RowMapping<CategoryRow> = for "categories" {
        id,
        parent_id               via column::OPTIONAL_TEXT,
        name,
        name_ar                 via column::OPTIONAL_TEXT,
        color                   via column::OPTIONAL_TEXT,
        icon                    via column::OPTIONAL_TEXT,
        image_url               via column::OPTIONAL_TEXT,
        display_order,
        is_active,
        managed "updated_at" = "datetime('now')",
    } on_conflict OnConflict::Replace;
}

impl Database {
    // ========================================================================
    // PRODUCT OPERATIONS
    // ========================================================================

    /// Saves or updates a product.
    pub fn save_product(&self, product: &ProductRow) -> SqliteResult<()> {
        self.insert(&PRODUCT_ROW, product)?;
        Ok(())
    }

    /// Saves every product in one transaction.
    ///
    /// This is the path production uses (`sync_service.rs:645`, `:747`); every fixture writes
    /// through [`Database::save_product`]. Both wrote their own 24-column list until this task,
    /// and they were maintained separately.
    ///
    /// Unlike `operators`, the hand-written version here already opened its own transaction, so
    /// this is a migration with no behaviour change hiding in it. Said explicitly because the
    /// equivalent change in `operators.rs` *was* a behaviour fix and looked identical in the diff.
    pub fn save_products(&self, products: &[ProductRow]) -> SqliteResult<usize> {
        self.insert_all(&PRODUCT_ROW, products)
    }

    /// Searches products using FTS5.
    ///
    /// The one query in this crate that joins, and so the only caller of
    /// [`Database::select_all_qualified`]: every column needs a `p.` prefix against
    /// `FROM products p JOIN products_fts fts`.
    pub fn search_products_fts(&self, query: &str, limit: i32) -> SqliteResult<Vec<ProductRow>> {
        // Escape the FTS5 quoting character and add prefix matching.
        let fts_query = format!("{}*", query.replace('"', "\"\""));
        self.select_all_qualified(
            PRODUCT_ROW.reader(),
            "p",
            "FROM products p
             JOIN products_fts fts ON p.rowid = fts.rowid
             WHERE products_fts MATCH ?1 AND p.is_active = 1
             ORDER BY rank
             LIMIT ?2",
            params![fts_query, limit],
        )
    }

    /// Searches products using `LIKE`, the fallback for partial and fuzzy matching.
    pub fn search_products(&self, query: &str, limit: i32) -> SqliteResult<Vec<ProductRow>> {
        let search = format!("%{query}%");
        self.select_all(
            PRODUCT_ROW.reader(),
            "FROM products
             WHERE is_active = 1
               AND (name LIKE ?1 OR name_ar LIKE ?1 OR sku LIKE ?1 OR barcode LIKE ?1)
             ORDER BY name
             LIMIT ?2",
            params![search, limit],
        )
    }

    /// Gets an active product by barcode.
    pub fn get_product_by_barcode(&self, barcode: &str) -> SqliteResult<Option<ProductRow>> {
        self.select_one(
            PRODUCT_ROW.reader(),
            "FROM products WHERE barcode = ?1 AND is_active = 1",
            [barcode],
        )
    }

    /// Gets a product by ID.
    pub fn get_product_by_id(&self, id: &str) -> SqliteResult<Option<ProductRow>> {
        self.select_one(PRODUCT_ROW.reader(), "FROM products WHERE id = ?1", [id])
    }

    /// Gets a page of the active products in one category.
    pub fn get_products_by_category(
        &self,
        category_id: &str,
        limit: i32,
        offset: i32,
    ) -> SqliteResult<Vec<ProductRow>> {
        self.select_all(
            PRODUCT_ROW.reader(),
            "FROM products
             WHERE category_id = ?1 AND is_active = 1
             ORDER BY name
             LIMIT ?2 OFFSET ?3",
            params![category_id, limit, offset],
        )
    }

    /// Gets a page of active products.
    pub fn get_all_products(&self, limit: i32, offset: i32) -> SqliteResult<Vec<ProductRow>> {
        self.select_all(
            PRODUCT_ROW.reader(),
            "FROM products
             WHERE is_active = 1
             ORDER BY name
             LIMIT ?1 OFFSET ?2",
            params![limit, offset],
        )
    }

    pub fn get_product_count(&self) -> SqliteResult<i64> {
        self.select_scalar("SELECT COUNT(*) FROM products WHERE is_active = 1", [])
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

    /// Saves or updates a category.
    pub fn save_category(&self, category: &CategoryRow) -> SqliteResult<()> {
        self.insert(&CATEGORY_ROW, category)?;
        Ok(())
    }

    /// Gets every active category, in display order.
    pub fn get_categories(&self) -> SqliteResult<Vec<CategoryRow>> {
        self.select_all(
            CATEGORY_ROW.reader(),
            "FROM categories WHERE is_active = 1 ORDER BY display_order, name",
            [],
        )
    }

    /// Gets the active categories that have no parent.
    pub fn get_root_categories(&self) -> SqliteResult<Vec<CategoryRow>> {
        self.select_all(
            CATEGORY_ROW.reader(),
            "FROM categories
             WHERE parent_id IS NULL AND is_active = 1
             ORDER BY display_order, name",
            [],
        )
    }

    /// Gets the active categories under one parent.
    pub fn get_child_categories(&self, parent_id: &str) -> SqliteResult<Vec<CategoryRow>> {
        self.select_all(
            CATEGORY_ROW.reader(),
            "FROM categories
             WHERE parent_id = ?1 AND is_active = 1
             ORDER BY display_order, name",
            [parent_id],
        )
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

    // ------------------------------------------------------------------------------------------
    // `PRODUCT_ROW` and `CATEGORY_ROW`. The three patterns from task 04, and the fallback columns.
    // ------------------------------------------------------------------------------------------

    /// A database holding the two categories the fixtures below point at.
    ///
    /// `products.category_id` and `categories.parent_id` are both real foreign keys and
    /// `PRAGMA foreign_keys` is `ON`, so a fixture that invents a distinct value for either has to
    /// create the row it names. That is a constraint on the *fixture*, not a reason to blank the
    /// column — `category_id` set to `None` would make it indistinguishable from four other
    /// absent columns, which is the whole failure the distinct-value discipline exists to avoid.
    fn setup_db_with_referents() -> Database {
        let db = setup_db();
        for id in ["category-id-column", "parent-id-column"] {
            db.save_category(&CategoryRow {
                id: id.to_string(),
                parent_id: None,
                name: format!("referent {id}"),
                name_ar: None,
                color: None,
                icon: None,
                image_url: None,
                display_order: 0,
                is_active: true,
            })
            .expect("a referent category");
        }
        db
    }

    /// A product whose every column holds a value found nowhere else in the row.
    ///
    /// The three columns with fallbacks — `product_type`, `track_inventory`, `product_nature` —
    /// deliberately carry values **different from what they fall back to**. Given
    /// `PHYSICAL_GOOD`, `true` and `TANGIBLE`, a fallback firing over stored data would be
    /// invisible; given `SERVICE`, `false` and `INTANGIBLE`, it fails.
    fn a_product_with_no_two_columns_alike() -> ProductRow {
        ProductRow {
            id: "id-column".to_string(),
            sku: "sku-column".to_string(),
            barcode: Some("barcode-column".to_string()),
            name: "name-column".to_string(),
            name_ar: Some("name-ar-column".to_string()),
            description: Some("description-column".to_string()),
            price: Decimal::from_str("11.11").unwrap(),
            cost: Decimal::from_str("22.22").unwrap(),
            tax_rate: Decimal::from_str("33.33").unwrap(),
            tax_inclusive: true,
            category_id: Some("category-id-column".to_string()),
            category_name: Some("category-name-column".to_string()),
            unit: "unit-column".to_string(),
            stock_qty: 44,
            min_stock: 55,
            allow_negative_stock: true,
            image_url: Some("image-url-column".to_string()),
            is_weighable: true,
            is_serialized: true,
            // Not `true`: that is the column's SQL `DEFAULT`, so a value that never reached the
            // store would read back as one that did.
            is_active: false,
            product_type: "SERVICE".to_string(),
            track_inventory: false,
            product_nature: "INTANGIBLE".to_string(),
        }
    }

    fn a_category_with_no_two_columns_alike() -> CategoryRow {
        CategoryRow {
            id: "id-column".to_string(),
            parent_id: Some("parent-id-column".to_string()),
            name: "name-column".to_string(),
            name_ar: Some("name-ar-column".to_string()),
            color: Some("color-column".to_string()),
            icon: Some("icon-column".to_string()),
            image_url: Some("image-url-column".to_string()),
            display_order: 66,
            is_active: false,
        }
    }

    #[test]
    fn the_product_mapping_names_every_column_it_writes_in_the_order_it_reads_them() {
        assert_eq!(
            PRODUCT_ROW.reader().select_list(),
            "id, sku, barcode, name, name_ar, description, price, cost, tax_rate, tax_inclusive, \
             category_id, category_name, unit, stock_qty, min_stock, allow_negative_stock, \
             image_url, is_weighable, is_serialized, is_active, product_type, track_inventory, \
             product_nature"
        );
        assert_eq!(PRODUCT_ROW.reader().width(), 23);
        assert_eq!(PRODUCT_ROW.insert_column_names().count(), 24);
        assert_eq!(
            CATEGORY_ROW.reader().select_list(),
            "id, parent_id, name, name_ar, color, icon, image_url, display_order, is_active"
        );
        assert_eq!(CATEGORY_ROW.reader().width(), 9);
        assert_eq!(CATEGORY_ROW.insert_column_names().count(), 10);
    }

    #[test]
    fn the_fts_projection_qualifies_every_column_with_the_join_alias() {
        // The one query in this crate that joins. Unqualified, `id` and `name` are ambiguous
        // against `products_fts`, and SQLite would answer with the wrong table's column rather
        // than an error for at least one of them.
        let qualified = PRODUCT_ROW.reader().select_list_qualified("p");
        assert!(qualified.starts_with("p.id, p.sku, p.barcode"));
        assert_eq!(qualified.matches("p.").count(), 23);
    }

    #[test]
    fn the_fts_search_runs_and_returns_every_column_of_the_matched_product() {
        // Until this test, **nothing in the workspace called `search_products_fts`** — measured:
        // replacing `select_all_qualified` with `select_all` in it left all 148 tests green. It is
        // the one query in this crate that joins, `products_fts` shares five column names with
        // `products` (`id, sku, barcode, name, name_ar`), and an unqualified projection is an
        // "ambiguous column name" error SQLite raises at prepare time. The mapping's most
        // dangerous consumer had no coverage at all.
        let db = setup_db_with_referents();
        let written = ProductRow {
            // The FTS query filters `p.is_active = 1`, so the fixture's deliberate `false` would
            // make this test pass on an empty result — the reading that cannot come out
            // differently. Everything else stays distinct.
            is_active: true,
            ..a_product_with_no_two_columns_alike()
        };
        db.save_product(&written).unwrap();

        // A single token, deliberately. `search_products_fts` passes the term to FTS5 with only the
        // quote character escaped, so a hyphenated term like `name-column` reaches the parser as
        // syntax and comes back `no such column: column` — an SQLite error, not an empty result,
        // for anything a cashier types with a hyphen in it. That is pre-existing behaviour in the
        // escaping at the top of the function, untouched by this task, and it is worth its own
        // issue; this test is about the projection.
        let found = db.search_products_fts("column", 10).unwrap();
        assert_eq!(found.len(), 1, "the FTS index did not match the product");

        // Every column through the qualified projection, not a sample: an alias applied to some
        // columns and not others is a shift, and this is the only reader that can see it.
        let read = &found[0];
        assert_eq!(read.id, written.id);
        assert_eq!(read.sku, written.sku);
        assert_eq!(read.barcode, written.barcode);
        assert_eq!(read.name, written.name);
        assert_eq!(read.name_ar, written.name_ar);
        assert_eq!(read.description, written.description);
        assert_eq!(read.price, written.price);
        assert_eq!(read.cost, written.cost);
        assert_eq!(read.tax_rate, written.tax_rate);
        assert_eq!(read.tax_inclusive, written.tax_inclusive);
        assert_eq!(read.category_id, written.category_id);
        assert_eq!(read.category_name, written.category_name);
        assert_eq!(read.unit, written.unit);
        assert_eq!(read.stock_qty, written.stock_qty);
        assert_eq!(read.min_stock, written.min_stock);
        assert_eq!(read.allow_negative_stock, written.allow_negative_stock);
        assert_eq!(read.image_url, written.image_url);
        assert_eq!(read.is_weighable, written.is_weighable);
        assert_eq!(read.is_serialized, written.is_serialized);
        assert_eq!(read.product_type, written.product_type);
        assert_eq!(read.track_inventory, written.track_inventory);
        assert_eq!(read.product_nature, written.product_nature);

        // The control: a term that matches nothing returns nothing. Without it "one result" and
        // "this query matches everything" read the same.
        assert!(db.search_products_fts("zzzzzz", 10).unwrap().is_empty());
    }

    #[test]
    fn every_column_of_a_fully_distinct_product_survives_the_round_trip() {
        let db = setup_db_with_referents();
        let written = a_product_with_no_two_columns_alike();
        db.save_product(&written).unwrap();

        let read = db
            .get_product_by_id("id-column")
            .unwrap()
            .expect("the product this test just wrote");

        assert_eq!(read.id, written.id);
        assert_eq!(read.sku, written.sku);
        assert_eq!(read.barcode, written.barcode);
        assert_eq!(read.name, written.name);
        assert_eq!(read.name_ar, written.name_ar);
        assert_eq!(read.description, written.description);
        assert_eq!(read.price, written.price);
        assert_eq!(read.cost, written.cost);
        assert_eq!(read.tax_rate, written.tax_rate);
        assert_eq!(read.tax_inclusive, written.tax_inclusive);
        assert_eq!(read.category_id, written.category_id);
        assert_eq!(read.category_name, written.category_name);
        assert_eq!(read.unit, written.unit);
        assert_eq!(read.stock_qty, written.stock_qty);
        assert_eq!(read.min_stock, written.min_stock);
        assert_eq!(read.allow_negative_stock, written.allow_negative_stock);
        assert_eq!(read.image_url, written.image_url);
        assert_eq!(read.is_weighable, written.is_weighable);
        assert_eq!(read.is_serialized, written.is_serialized);
        assert_eq!(read.is_active, written.is_active);
        assert_eq!(read.product_type, written.product_type);
        assert_eq!(read.track_inventory, written.track_inventory);
        assert_eq!(read.product_nature, written.product_nature);
    }

    #[test]
    fn every_column_of_a_fully_distinct_category_survives_the_round_trip() {
        let db = setup_db_with_referents();
        let written = a_category_with_no_two_columns_alike();
        db.save_category(&written).unwrap();

        let read = db
            .select_one(
                CATEGORY_ROW.reader(),
                "FROM categories WHERE id = ?1",
                ["id-column"],
            )
            .unwrap()
            .expect("the category this test just wrote");

        assert_eq!(read.id, written.id);
        assert_eq!(read.parent_id, written.parent_id);
        assert_eq!(read.name, written.name);
        assert_eq!(read.name_ar, written.name_ar);
        assert_eq!(read.color, written.color);
        assert_eq!(read.icon, written.icon);
        assert_eq!(read.image_url, written.image_url);
        assert_eq!(read.display_order, written.display_order);
        assert_eq!(read.is_active, written.is_active);
    }

    /// Reads every column of the single stored product back **by name** and asserts it holds the
    /// value belonging to it. One caller per writer — see task 04's note: a round trip is
    /// invariant under a permutation applied to both halves, and only a third reader that names
    /// columns can see one.
    fn assert_every_product_column_holds_its_own_value(db: &Database) {
        let conn = db.connection();
        let conn = conn.lock();

        for (column, expected) in [
            ("id", "id-column"),
            ("sku", "sku-column"),
            ("barcode", "barcode-column"),
            ("name", "name-column"),
            ("name_ar", "name-ar-column"),
            ("description", "description-column"),
            ("category_id", "category-id-column"),
            ("category_name", "category-name-column"),
            ("unit", "unit-column"),
            ("image_url", "image-url-column"),
            ("product_type", "SERVICE"),
            ("product_nature", "INTANGIBLE"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM products"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        for (column, expected) in [
            ("price", 11.11_f64),
            ("cost", 22.22),
            ("tax_rate", 33.33),
            ("stock_qty", 44.0),
            ("min_stock", 55.0),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM products"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }

        // The five booleans. `is_active` is 0 where the other four are 1, so a swap among them is
        // visible; a row of five identical flags would not be.
        for (column, expected) in [
            ("tax_inclusive", 1_i64),
            ("allow_negative_stock", 1),
            ("is_weighable", 1),
            ("is_serialized", 1),
            ("is_active", 0),
            ("track_inventory", 0),
        ] {
            let stored: i64 = conn
                .query_row(&format!("SELECT {column} FROM products"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(stored, expected, "the `{column}` column");
        }
    }

    #[test]
    fn save_product_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db_with_referents();
        db.save_product(&a_product_with_no_two_columns_alike())
            .unwrap();
        assert_every_product_column_holds_its_own_value(&db);
    }

    #[test]
    fn save_products_puts_each_value_in_the_column_that_carries_its_name() {
        // The path production actually uses (`sync_service.rs:645`, `:747`); every fixture in this
        // file writes through the other one. They were two hand-maintained 24-column lists.
        let db = setup_db_with_referents();
        assert_eq!(
            db.save_products(&[a_product_with_no_two_columns_alike()])
                .unwrap(),
            1
        );
        assert_every_product_column_holds_its_own_value(&db);
    }

    #[test]
    fn save_category_puts_each_value_in_the_column_that_carries_its_name() {
        let db = setup_db_with_referents();
        db.save_category(&a_category_with_no_two_columns_alike())
            .unwrap();

        // Scoped to this row: `setup_db_with_referents` seeded two other categories, and an
        // unscoped `FROM categories` would answer about whichever one SQLite reached first.
        let conn = db.connection();
        let conn = conn.lock();
        for (column, expected) in [
            ("id", "id-column"),
            ("parent_id", "parent-id-column"),
            ("name", "name-column"),
            ("name_ar", "name-ar-column"),
            ("color", "color-column"),
            ("icon", "icon-column"),
            ("image_url", "image-url-column"),
        ] {
            let matched: bool = conn
                .query_row(
                    &format!("SELECT {column} = ?1 FROM categories WHERE id = 'id-column'"),
                    [expected],
                    |row| row.get(0),
                )
                .unwrap();
            assert!(matched, "the `{column}` column does not hold `{expected}`");
        }
        let display_order: i64 = conn
            .query_row(
                "SELECT display_order FROM categories WHERE id = 'id-column'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(display_order, 66);
        let is_active: i64 = conn
            .query_row(
                "SELECT is_active FROM categories WHERE id = 'id-column'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(is_active, 0);
    }

    #[test]
    fn a_second_write_of_the_same_product_id_replaces_the_row() {
        let db = setup_db_with_referents();
        let first = a_product_with_no_two_columns_alike();
        db.save_product(&first).unwrap();
        let renamed = ProductRow {
            name: "second-write".to_string(),
            ..first
        };
        db.save_product(&renamed).unwrap();

        let rows: i64 = db
            .select_scalar("SELECT COUNT(*) FROM products", [])
            .unwrap();
        assert_eq!(rows, 1);
        assert_eq!(
            db.get_product_by_id("id-column").unwrap().unwrap().name,
            "second-write"
        );
    }

    #[test]
    fn a_second_write_of_the_same_category_id_replaces_the_row() {
        let db = setup_db_with_referents();
        let first = a_category_with_no_two_columns_alike();
        db.save_category(&first).unwrap();
        db.save_category(&CategoryRow {
            name: "second-write".to_string(),
            ..first
        })
        .unwrap();

        let rows: i64 = db
            .select_scalar(
                "SELECT COUNT(*) FROM categories WHERE id = ?1",
                ["id-column"],
            )
            .unwrap();
        assert_eq!(rows, 1, "the second write inserted rather than replaced");
    }

    /// One nullable product column, blanked, and what must still hold of its neighbours.
    struct AbsentProductColumn {
        column: &'static str,
        blank: fn(&mut ProductRow),
        assert_absent: fn(&ProductRow),
    }

    #[test]
    fn a_null_in_one_product_column_reaches_that_columns_field_and_no_other() {
        // Per column, never all at once — see the plan's note. Seven `None`s look identical under
        // any permutation of the absent columns, so the all-absent form passes under exactly the
        // defect it was written for.
        let db = setup_db_with_referents();
        let full = a_product_with_no_two_columns_alike();

        let cases = [
            AbsentProductColumn {
                column: "barcode",
                blank: |row| row.barcode = None,
                assert_absent: |row| {
                    assert_eq!(row.barcode, None);
                    assert_eq!(row.name_ar.as_deref(), Some("name-ar-column"));
                },
            },
            AbsentProductColumn {
                column: "name_ar",
                blank: |row| row.name_ar = None,
                assert_absent: |row| {
                    assert_eq!(row.name_ar, None);
                    assert_eq!(row.description.as_deref(), Some("description-column"));
                },
            },
            AbsentProductColumn {
                column: "description",
                blank: |row| row.description = None,
                assert_absent: |row| {
                    assert_eq!(row.description, None);
                    assert_eq!(row.name_ar.as_deref(), Some("name-ar-column"));
                },
            },
            AbsentProductColumn {
                column: "category_id",
                blank: |row| row.category_id = None,
                assert_absent: |row| {
                    assert_eq!(row.category_id, None);
                    assert_eq!(row.category_name.as_deref(), Some("category-name-column"));
                },
            },
            AbsentProductColumn {
                column: "category_name",
                blank: |row| row.category_name = None,
                assert_absent: |row| {
                    assert_eq!(row.category_name, None);
                    assert_eq!(row.category_id.as_deref(), Some("category-id-column"));
                },
            },
            AbsentProductColumn {
                column: "image_url",
                blank: |row| row.image_url = None,
                assert_absent: |row| {
                    assert_eq!(row.image_url, None);
                    assert_eq!(row.unit, "unit-column");
                },
            },
        ];

        for case in cases {
            let mut written = full.clone();
            (case.blank)(&mut written);
            db.save_product(&written).unwrap();

            let stored: Option<String> = db
                .select_scalar(&format!("SELECT {} FROM products", case.column), [])
                .unwrap();
            assert_eq!(stored, None, "`{}` was not written as NULL", case.column);

            let read = db
                .get_product_by_id("id-column")
                .unwrap()
                .expect("the row this iteration wrote");
            (case.assert_absent)(&read);
        }
    }

    #[test]
    fn a_null_in_the_three_defaulting_columns_reads_as_the_default_and_nothing_else_moves() {
        // The fallbacks stay — out of scope by design — so this pins today's behaviour rather than
        // deciding about it. What it also checks is that a NULL in one of them does not disturb a
        // neighbour, which is the part a fallback test alone would miss.
        let db = setup_db_with_referents();
        db.save_product(&a_product_with_no_two_columns_alike())
            .unwrap();
        db.execute(
            "UPDATE products SET product_type = NULL, track_inventory = NULL, product_nature = NULL",
            &[],
        )
        .unwrap();

        let read = db.get_product_by_id("id-column").unwrap().unwrap();
        assert_eq!(read.product_type, "PHYSICAL_GOOD");
        assert!(read.track_inventory);
        assert_eq!(read.product_nature, "TANGIBLE");
        assert!(!read.is_active, "the column before them moved");
        assert!(read.is_serialized, "the column before them moved");
    }
}
