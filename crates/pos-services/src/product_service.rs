//! Product Service
//!
//! Provides product search, lookup, and category management functionality.
//! This service wraps the database layer and provides a clean API for the UI.

use pos_db::{CategoryRow, Database, ProductRow};
use pos_models::product::{Category, Product, ProductSearchResult, ProductUnit};
use std::sync::Arc;
use std::time::Instant;
use thiserror::Error;
use tracing::{debug, info, instrument, warn};

/// Product service errors
#[derive(Debug, Error)]
pub enum ProductError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Product not found: {0}")]
    NotFound(String),

    #[error("Invalid search query")]
    InvalidQuery,
}

/// Product service for search and lookup operations
///
/// # Example
///
/// ```rust,ignore
/// use crate::services::ProductService;
/// use std::sync::Arc;
///
/// let db = Arc::new(init_database(&data_dir)?);
/// let product_service = ProductService::new(db);
///
/// // Search products
/// let results = product_service.search("milk", 20)?;
/// for product in results.products {
///     println!("{}: ${:.2}", product.name, product.price);
/// }
///
/// // Lookup by barcode
/// if let Some(product) = product_service.lookup_barcode("1234567890123")? {
///     println!("Found: {}", product.name);
/// }
/// ```
pub struct ProductService {
    db: Arc<Database>,
}

impl ProductService {
    /// Creates a new ProductService with the given database
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Searches products by text query using FTS5 full-text search
    ///
    /// Searches across product name, Arabic name, SKU, and barcode.
    /// Returns results ordered by relevance.
    ///
    /// # Arguments
    /// * `query` - Search text (min 1 character)
    /// * `limit` - Maximum results to return (default 50)
    ///
    /// # Performance
    /// Uses SQLite FTS5 for fast (<50ms) search even with thousands of products.
    #[instrument(skip(self), fields(query = %query, limit = %limit))]
    pub fn search(&self, query: &str, limit: i32) -> Result<ProductSearchResult, ProductError> {
        let query = query.trim();

        if query.is_empty() {
            return Ok(ProductSearchResult::empty(""));
        }

        let start = Instant::now();

        // Try barcode exact match first (fastest)
        if let Some(product) = self.lookup_barcode(query)? {
            let elapsed = start.elapsed().as_millis() as u64;
            debug!("Barcode match found in {}ms", elapsed);
            return Ok(ProductSearchResult {
                products: vec![product],
                query: query.to_string(),
                total_count: 1,
                search_time_ms: elapsed,
            });
        }

        // Use FTS5 for text search
        let rows = self.db.search_products_fts(query, limit)?;
        let elapsed = start.elapsed().as_millis() as u64;

        let products: Vec<Product> = rows.into_iter().map(product_from_row).collect();
        let count = products.len();

        if count > 0 {
            info!("FTS search found {} products in {}ms", count, elapsed);
        } else {
            debug!("No products found for query '{}' ({}ms)", query, elapsed);
        }

        Ok(ProductSearchResult {
            products,
            query: query.to_string(),
            total_count: count,
            search_time_ms: elapsed,
        })
    }

    /// Searches products using LIKE pattern matching (fallback for partial matches)
    ///
    /// Use this when FTS doesn't find results or for partial/fuzzy matching.
    #[instrument(skip(self), fields(query = %query, limit = %limit))]
    pub fn search_like(&self, query: &str, limit: i32) -> Result<ProductSearchResult, ProductError> {
        let query = query.trim();

        if query.is_empty() {
            return Ok(ProductSearchResult::empty(""));
        }

        let start = Instant::now();
        let rows = self.db.search_products(query, limit)?;
        let elapsed = start.elapsed().as_millis() as u64;

        let products: Vec<Product> = rows.into_iter().map(product_from_row).collect();
        let count = products.len();

        debug!("LIKE search found {} products in {}ms", count, elapsed);

        Ok(ProductSearchResult {
            products,
            query: query.to_string(),
            total_count: count,
            search_time_ms: elapsed,
        })
    }

    /// Combined search: tries FTS first, falls back to LIKE if no results
    #[instrument(skip(self), fields(query = %query, limit = %limit))]
    pub fn smart_search(&self, query: &str, limit: i32) -> Result<ProductSearchResult, ProductError> {
        // Try FTS first
        let result = self.search(query, limit)?;

        if !result.is_empty() {
            return Ok(result);
        }

        // Fallback to LIKE for partial matches
        debug!("FTS returned no results, trying LIKE search");
        self.search_like(query, limit)
    }

    /// Looks up a product by exact barcode match
    ///
    /// # Arguments
    /// * `barcode` - The barcode to search for
    ///
    /// # Returns
    /// * `Some(Product)` if found
    /// * `None` if not found
    #[instrument(skip(self))]
    pub fn lookup_barcode(&self, barcode: &str) -> Result<Option<Product>, ProductError> {
        let barcode = barcode.trim();

        if barcode.is_empty() {
            return Ok(None);
        }

        match self.db.get_product_by_barcode(barcode)? {
            Some(row) => {
                debug!("Product found by barcode: {}", barcode);
                Ok(Some(product_from_row(row)))
            }
            None => {
                debug!("No product found for barcode: {}", barcode);
                Ok(None)
            }
        }
    }

    /// Gets a product by ID
    #[instrument(skip(self))]
    pub fn get_by_id(&self, id: &str) -> Result<Option<Product>, ProductError> {
        match self.db.get_product_by_id(id)? {
            Some(row) => Ok(Some(product_from_row(row))),
            None => Ok(None),
        }
    }

    /// Gets a product by ID, returning an error if not found
    #[instrument(skip(self))]
    pub fn get_by_id_required(&self, id: &str) -> Result<Product, ProductError> {
        self.get_by_id(id)?
            .ok_or_else(|| ProductError::NotFound(id.to_string()))
    }

    /// Gets products by category
    ///
    /// # Arguments
    /// * `category_id` - The category ID to filter by
    /// * `limit` - Maximum products to return
    /// * `offset` - Offset for pagination
    #[instrument(skip(self), fields(category_id = %category_id))]
    pub fn by_category(
        &self,
        category_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<Product>, ProductError> {
        let rows = self.db.get_products_by_category(category_id, limit, offset)?;
        let products: Vec<Product> = rows.into_iter().map(product_from_row).collect();
        debug!("Found {} products in category {}", products.len(), category_id);
        Ok(products)
    }

    /// Gets all categories
    #[instrument(skip(self))]
    pub fn categories(&self) -> Result<Vec<Category>, ProductError> {
        let rows = self.db.get_categories()?;
        let categories: Vec<Category> = rows.into_iter().map(category_from_row).collect();
        debug!("Found {} categories", categories.len());
        Ok(categories)
    }

    /// Gets root categories (no parent)
    #[instrument(skip(self))]
    pub fn root_categories(&self) -> Result<Vec<Category>, ProductError> {
        let rows = self.db.get_root_categories()?;
        let categories: Vec<Category> = rows.into_iter().map(category_from_row).collect();
        debug!("Found {} root categories", categories.len());
        Ok(categories)
    }

    /// Gets child categories of a parent
    #[instrument(skip(self), fields(parent_id = %parent_id))]
    pub fn child_categories(&self, parent_id: &str) -> Result<Vec<Category>, ProductError> {
        let rows = self.db.get_child_categories(parent_id)?;
        let categories: Vec<Category> = rows.into_iter().map(category_from_row).collect();
        debug!("Found {} child categories for {}", categories.len(), parent_id);
        Ok(categories)
    }

    /// Gets all active products with pagination
    #[instrument(skip(self), fields(limit = %limit, offset = %offset))]
    pub fn all_products(&self, limit: i32, offset: i32) -> Result<Vec<Product>, ProductError> {
        let rows = self.db.get_all_products(limit, offset)?;
        let products: Vec<Product> = rows.into_iter().map(product_from_row).collect();
        debug!("Found {} products (limit={}, offset={})", products.len(), limit, offset);
        Ok(products)
    }

    /// Gets the total product count
    pub fn product_count(&self) -> Result<i64, ProductError> {
        Ok(self.db.get_product_count()?)
    }

    /// Checks if the product cache is populated
    pub fn has_products(&self) -> Result<bool, ProductError> {
        Ok(self.product_count()? > 0)
    }

    /// Validates that the product cache is ready for use
    ///
    /// Returns an error message if the cache needs to be populated.
    pub fn validate_cache(&self) -> Result<(), String> {
        match self.product_count() {
            Ok(count) if count > 0 => {
                info!("Product cache valid: {} products", count);
                Ok(())
            }
            Ok(_) => {
                // count is 0 or less
                warn!("Product cache is empty - sync required");
                Err("Product cache is empty. Please sync with server.".to_string())
            }
            Err(e) => {
                warn!("Failed to check product cache: {}", e);
                Err(format!("Database error: {}", e))
            }
        }
    }
}

/// Converts a ProductRow from the database to a Product domain model
fn product_from_row(row: ProductRow) -> Product {
    use pos_models::product::{ProductNature, ProductType};

    Product {
        id: row.id,
        sku: row.sku,
        barcode: row.barcode,
        name: row.name,
        name_ar: row.name_ar,
        description: row.description,
        price: row.price,
        cost: row.cost,
        tax_rate: row.tax_rate,
        tax_inclusive: row.tax_inclusive,
        category_id: row.category_id,
        category_name: row.category_name,
        unit: row.unit.parse().unwrap_or(ProductUnit::Unit),
        stock_qty: row.stock_qty,
        min_stock: row.min_stock,
        allow_negative_stock: row.allow_negative_stock,
        image_url: row.image_url,
        is_weighable: row.is_weighable,
        is_serialized: row.is_serialized,
        is_active: row.is_active,
        product_type: ProductType::from_str(&row.product_type),
        track_inventory: row.track_inventory,
        product_nature: ProductNature::from_str(&row.product_nature),
    }
}

/// Converts a CategoryRow from the database to a Category domain model
fn category_from_row(row: CategoryRow) -> Category {
    Category {
        id: row.id,
        parent_id: row.parent_id,
        name: row.name,
        name_ar: row.name_ar,
        color: row.color,
        icon: row.icon,
        image_url: row.image_url,
        display_order: row.display_order,
        is_active: row.is_active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_db::{init_memory_database, CategoryRow, ProductRow};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn setup() -> (Arc<Database>, ProductService) {
        let db = Arc::new(init_memory_database().unwrap());
        let service = ProductService::new(Arc::clone(&db));
        (db, service)
    }

    fn insert_test_products(db: &Database) {
        // First insert categories (required for foreign key constraints)
        insert_test_categories(db);

        let products = vec![
            ProductRow {
                id: "prod-1".to_string(),
                sku: "SKU001".to_string(),
                barcode: Some("1234567890123".to_string()),
                name: "Fresh Milk 1L".to_string(),
                name_ar: Some("حليب طازج 1 لتر".to_string()),
                price: Decimal::from_str("5.50").unwrap(),
                tax_rate: Decimal::ZERO,
                category_id: Some("cat-dairy".to_string()),
                ..Default::default()
            },
            ProductRow {
                id: "prod-2".to_string(),
                sku: "SKU002".to_string(),
                barcode: Some("1234567890124".to_string()),
                name: "Orange Juice 1L".to_string(),
                name_ar: Some("عصير برتقال 1 لتر".to_string()),
                price: Decimal::from_str("3.99").unwrap(),
                tax_rate: Decimal::ZERO,
                category_id: Some("cat-beverages".to_string()),
                ..Default::default()
            },
            ProductRow {
                id: "prod-3".to_string(),
                sku: "SKU003".to_string(),
                barcode: Some("1234567890125".to_string()),
                name: "Whole Wheat Bread".to_string(),
                name_ar: Some("خبز القمح الكامل".to_string()),
                price: Decimal::from_str("2.50").unwrap(),
                tax_rate: Decimal::ZERO,
                category_id: Some("cat-bakery".to_string()),
                ..Default::default()
            },
            ProductRow {
                id: "prod-4".to_string(),
                sku: "SKU004".to_string(),
                barcode: Some("1234567890126".to_string()),
                name: "Chocolate Milk 500ml".to_string(),
                name_ar: Some("حليب بالشوكولاتة 500مل".to_string()),
                price: Decimal::from_str("4.25").unwrap(),
                tax_rate: Decimal::ZERO,
                category_id: Some("cat-dairy".to_string()),
                ..Default::default()
            },
            ProductRow {
                id: "prod-5".to_string(),
                sku: "MILK123".to_string(),
                barcode: Some("9999999999999".to_string()),
                name: "Low Fat Milk 2L".to_string(),
                name_ar: Some("حليب قليل الدسم 2 لتر".to_string()),
                price: Decimal::from_str("8.99").unwrap(),
                tax_rate: Decimal::ZERO,
                category_id: Some("cat-dairy".to_string()),
                ..Default::default()
            },
        ];

        for product in &products {
            db.save_product(product).unwrap();
        }
    }

    fn insert_test_categories(db: &Database) {
        let categories = vec![
            CategoryRow {
                id: "cat-dairy".to_string(),
                parent_id: None,
                name: "Dairy".to_string(),
                name_ar: Some("ألبان".to_string()),
                color: Some("#4CAF50".to_string()),
                icon: Some("milk".to_string()),
                image_url: None,
                display_order: 1,
                is_active: true,
            },
            CategoryRow {
                id: "cat-beverages".to_string(),
                parent_id: None,
                name: "Beverages".to_string(),
                name_ar: Some("مشروبات".to_string()),
                color: Some("#2196F3".to_string()),
                icon: Some("drink".to_string()),
                image_url: None,
                display_order: 2,
                is_active: true,
            },
            CategoryRow {
                id: "cat-bakery".to_string(),
                parent_id: None,
                name: "Bakery".to_string(),
                name_ar: Some("مخبوزات".to_string()),
                color: Some("#FF9800".to_string()),
                icon: Some("bread".to_string()),
                image_url: None,
                display_order: 3,
                is_active: true,
            },
        ];

        for category in &categories {
            db.save_category(category).unwrap();
        }
    }

    #[test]
    fn test_search_by_name() {
        let (db, service) = setup();
        insert_test_products(&db);

        // Search for "Milk"
        let result = service.search("Milk", 10).unwrap();
        assert_eq!(result.products.len(), 3); // Fresh Milk, Chocolate Milk, Low Fat Milk
        assert!(result.search_time_ms < 100); // Should be fast
    }

    #[test]
    fn test_search_by_arabic_name() {
        let (db, service) = setup();
        insert_test_products(&db);

        // Search for Arabic "حليب" (milk)
        let result = service.search("حليب", 10).unwrap();
        assert!(result.products.len() >= 3); // All milk products
    }

    #[test]
    fn test_search_by_sku() {
        let (db, service) = setup();
        insert_test_products(&db);

        // Search by SKU
        let result = service.search("MILK123", 10).unwrap();
        assert_eq!(result.products.len(), 1);
        assert_eq!(result.products[0].name, "Low Fat Milk 2L");
    }

    #[test]
    fn test_lookup_barcode() {
        let (db, service) = setup();
        insert_test_products(&db);

        // Exact barcode match
        let product = service.lookup_barcode("1234567890123").unwrap();
        assert!(product.is_some());
        assert_eq!(product.unwrap().name, "Fresh Milk 1L");

        // Non-existent barcode
        let not_found = service.lookup_barcode("0000000000000").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_barcode_search_priority() {
        let (db, service) = setup();
        insert_test_products(&db);

        // When query is a barcode, should return exact match immediately
        let result = service.search("1234567890123", 10).unwrap();
        assert_eq!(result.products.len(), 1);
        assert_eq!(result.products[0].barcode, Some("1234567890123".to_string()));
    }

    #[test]
    fn test_search_empty_query() {
        let (_db, service) = setup();

        let result = service.search("", 10).unwrap();
        assert!(result.is_empty());

        let result = service.search("   ", 10).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_search_limit() {
        let (db, service) = setup();
        insert_test_products(&db);

        // Search with limit
        let result = service.search("1L", 2).unwrap();
        assert!(result.products.len() <= 2);
    }

    #[test]
    fn test_smart_search_fallback() {
        let (db, service) = setup();
        insert_test_products(&db);

        // Smart search should find results
        let result = service.smart_search("Milk", 10).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_search_like() {
        let (db, service) = setup();
        insert_test_products(&db);

        // LIKE search for partial match
        let result = service.search_like("Bread", 10).unwrap();
        assert_eq!(result.products.len(), 1);
        assert_eq!(result.products[0].name, "Whole Wheat Bread");
    }

    #[test]
    fn test_get_by_id() {
        let (db, service) = setup();
        insert_test_products(&db);

        let product = service.get_by_id("prod-1").unwrap();
        assert!(product.is_some());
        assert_eq!(product.unwrap().sku, "SKU001");

        let not_found = service.get_by_id("non-existent").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_get_by_id_required() {
        let (db, service) = setup();
        insert_test_products(&db);

        let product = service.get_by_id_required("prod-1").unwrap();
        assert_eq!(product.sku, "SKU001");

        let err = service.get_by_id_required("non-existent");
        assert!(matches!(err, Err(ProductError::NotFound(_))));
    }

    #[test]
    fn test_by_category() {
        let (db, service) = setup();
        insert_test_categories(&db);
        insert_test_products(&db);

        // Get dairy products
        let dairy = service.by_category("cat-dairy", 50, 0).unwrap();
        assert_eq!(dairy.len(), 3); // Fresh Milk, Chocolate Milk, Low Fat Milk

        // Get beverages
        let beverages = service.by_category("cat-beverages", 50, 0).unwrap();
        assert_eq!(beverages.len(), 1); // Orange Juice
    }

    #[test]
    fn test_categories() {
        let (db, service) = setup();
        insert_test_categories(&db);

        let categories = service.categories().unwrap();
        assert_eq!(categories.len(), 3);
    }

    #[test]
    fn test_root_categories() {
        let (db, service) = setup();
        insert_test_categories(&db);

        let roots = service.root_categories().unwrap();
        assert_eq!(roots.len(), 3); // All are root categories
    }

    #[test]
    fn test_product_count() {
        let (db, service) = setup();

        assert_eq!(service.product_count().unwrap(), 0);

        insert_test_products(&db);
        assert_eq!(service.product_count().unwrap(), 5);
    }

    #[test]
    fn test_has_products() {
        let (db, service) = setup();

        assert!(!service.has_products().unwrap());

        insert_test_products(&db);
        assert!(service.has_products().unwrap());
    }

    #[test]
    fn test_validate_cache() {
        let (db, service) = setup();

        // Empty cache should fail
        assert!(service.validate_cache().is_err());

        // Populated cache should pass
        insert_test_products(&db);
        assert!(service.validate_cache().is_ok());
    }

    #[test]
    fn test_product_model_conversion() {
        let (db, service) = setup();

        // Product without category reference (no FK constraint)
        let row = ProductRow {
            id: "test".to_string(),
            sku: "TEST001".to_string(),
            barcode: Some("123".to_string()),
            name: "Test".to_string(),
            name_ar: Some("اختبار".to_string()),
            description: Some("Description".to_string()),
            price: rust_decimal::Decimal::from(10),
            cost: rust_decimal::Decimal::from(5),
            tax_rate: rust_decimal::Decimal::from(15),
            tax_inclusive: false,
            category_id: None, // No category to avoid FK constraint
            category_name: Some("Category".to_string()),
            unit: "KG".to_string(),
            stock_qty: 100,
            min_stock: 10,
            allow_negative_stock: true,
            image_url: None,
            is_weighable: true,
            is_serialized: false,
            is_active: true,
            product_type: "PHYSICAL_GOOD".to_string(),
            track_inventory: true,
            product_nature: "TANGIBLE".to_string(),
        };

        db.save_product(&row).unwrap();

        let product = service.get_by_id("test").unwrap().unwrap();

        assert_eq!(product.id, "test");
        assert_eq!(product.sku, "TEST001");
        assert_eq!(product.unit, ProductUnit::Kg);
        assert!(product.is_weighable);
        assert!(!product.is_serialized);
        assert!(product.allow_negative_stock);
        assert_eq!(product.price_with_tax(), rust_decimal::Decimal::new(115, 1)); // 10 + 15% = 11.5
    }

    #[test]
    fn test_category_model_conversion() {
        let (db, service) = setup();

        // First create parent category
        let parent = CategoryRow {
            id: "cat-parent".to_string(),
            parent_id: None,
            name: "Parent Category".to_string(),
            name_ar: Some("الفئة الرئيسية".to_string()),
            color: Some("#0000FF".to_string()),
            icon: None,
            image_url: None,
            display_order: 1,
            is_active: true,
        };
        db.save_category(&parent).unwrap();

        // Then create child category
        let row = CategoryRow {
            id: "cat-test".to_string(),
            parent_id: Some("cat-parent".to_string()),
            name: "Test Category".to_string(),
            name_ar: Some("فئة اختبار".to_string()),
            color: Some("#FF0000".to_string()),
            icon: Some("test-icon".to_string()),
            image_url: Some("http://example.com/image.png".to_string()),
            display_order: 5,
            is_active: true,
        };

        db.save_category(&row).unwrap();

        let categories = service.categories().unwrap();
        let category = categories.iter().find(|c| c.id == "cat-test").unwrap();

        assert_eq!(category.name, "Test Category");
        assert_eq!(category.display_name("ar"), "فئة اختبار");
        assert_eq!(category.color_or_default(), "#FF0000");
        assert!(!category.is_root());
    }

    #[test]
    fn test_search_performance() {
        let (db, service) = setup();

        // Insert many products for performance test
        let products: Vec<ProductRow> = (1..=100)
            .map(|i| ProductRow {
                id: format!("perf-{}", i),
                sku: format!("PERF{:04}", i),
                barcode: Some(format!("999999{:06}", i)),
                name: format!("Performance Test Product {}", i),
                name_ar: Some(format!("منتج اختبار الأداء {}", i)),
                price: Decimal::from(i as i64),
                ..Default::default()
            })
            .collect();

        db.save_products(&products).unwrap();

        // Search should complete in under 50ms
        let start = std::time::Instant::now();
        let result = service.search("Performance", 50).unwrap();
        let elapsed = start.elapsed().as_millis();

        assert!(elapsed < 100, "Search took {}ms, expected <100ms", elapsed);
        assert_eq!(result.products.len(), 50); // Limited to 50
    }
}
