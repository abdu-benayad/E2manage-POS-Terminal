//! Common Test Utilities
//!
//! Shared setup functions and test data for integration tests.

// `#[allow]` rather than `#[expect]`, deliberately: this module is included by `mod common;`
// into several independent integration-test crates, and each uses a different subset of these
// helpers. `#[expect(dead_code)]` would be unfulfilled — and so itself an error — in whichever
// crate happens to use all of them.
#![allow(dead_code)]

use e2manage_pos_terminal::api::ApiClient;
use e2manage_pos_terminal::db::migrations::run_migrations;
use e2manage_pos_terminal::db::{Database, OperatorRow, ProductRow, ShiftRow};
use e2manage_pos_terminal::models::cart::{Cart, CartItem};
use e2manage_pos_terminal::models::product::{Product, ProductUnit};
use e2manage_pos_terminal::models::{OperatorId, OperatorName, OperatorRole, RecordedOperatorName};

/// Builds an operator id for a fixture. The domain type is fallible; a test literal is not,
/// so this keeps the call sites readable without letting a blank id back in.
pub fn op_id(id: &str) -> OperatorId {
    OperatorId::new(id).expect("a fixture id is never blank")
}

/// Builds a recorded operator name for a fixture.
pub fn op_name(name: &str) -> RecordedOperatorName {
    RecordedOperatorName::new(name).expect("a fixture name is never blank")
}

/// Builds an operator's two-script name for a fixture.
pub fn op_full_name(latin: &str, arabic: &str) -> OperatorName {
    OperatorName::new(latin, Some(arabic)).expect("a fixture name is never blank")
}

/// Builds an operator name for a fixture whose HR record carries no Arabic spelling.
pub fn op_latin_name(latin: &str) -> OperatorName {
    OperatorName::new(latin, None::<&str>).expect("a fixture name is never blank")
}
use e2manage_pos_terminal::services::auth_service::AuthService;
use e2manage_pos_terminal::services::cart_service::CartService;
use e2manage_pos_terminal::services::draft_service::DraftService;
use e2manage_pos_terminal::services::offline_service::OfflineService;
use e2manage_pos_terminal::services::product_service::ProductService;
use e2manage_pos_terminal::services::shift_service::ShiftService;
use e2manage_pos_terminal::services::transaction_service::TransactionService;
use rust_decimal::Decimal;
use std::sync::Arc;

/// Creates an in-memory test database with migrations applied.
pub fn setup_test_db() -> Database {
    let db = Database::in_memory().expect("Failed to create in-memory database");
    {
        let conn = db.connection();
        let conn = conn.lock();
        run_migrations(&conn).expect("Failed to run migrations");
    }
    db
}

/// Creates an Arc-wrapped test database (commonly needed by services).
pub fn setup_test_db_arc() -> Arc<Database> {
    Arc::new(setup_test_db())
}

/// Creates a mock API client for testing.
pub fn setup_mock_api() -> ApiClient {
    ApiClient::new("http://localhost:3000")
}

/// Creates an Arc-wrapped mock API client.
pub fn setup_mock_api_arc() -> Arc<ApiClient> {
    Arc::new(setup_mock_api())
}

/// Creates a sample product for testing.
pub fn sample_product() -> Product {
    Product {
        id: "prod-001".to_string(),
        sku: "SKU001".to_string(),
        barcode: Some("1234567890123".to_string()),
        name: "Test Product".to_string(),
        name_ar: Some("منتج تجريبي".to_string()),
        price: Decimal::from(10),
        tax_rate: Decimal::from(15),
        tax_inclusive: false,
        category_id: None, // No category for basic tests
        stock_qty: 100,
        is_active: true,
        unit: ProductUnit::Unit,
        ..Default::default()
    }
}

/// Creates a product with custom values.
pub fn create_product(id: &str, name: &str, price: Decimal, tax_rate: Decimal) -> Product {
    Product {
        id: id.to_string(),
        sku: format!("SKU-{}", id),
        barcode: Some(format!(
            "{:013}",
            id.chars().filter(|c| c.is_numeric()).collect::<String>()
        )),
        name: name.to_string(),
        name_ar: Some(format!("منتج {}", id)),
        price,
        tax_rate,
        tax_inclusive: false,
        unit: ProductUnit::Unit,
        is_active: true,
        ..Default::default()
    }
}

/// Creates a sample operator data structure for database.
///
/// The one place a fixture operator is written out in full. `OperatorRow` has no `Default` —
/// deliberately, because the one it used to have produced a record belonging to nobody — so every
/// other fixture below updates from *this* value with `..sample_operator_row()`. The base is a
/// real operator, which is the whole difference.
pub fn sample_operator_row() -> OperatorRow {
    OperatorRow {
        id: op_id("op-001"),
        code: "OP001".to_string(),
        employee_id: None,
        employee_number: None,
        name: op_full_name("Test Operator", "مشغل تجريبي"),
        role: OperatorRole::Cashier,
        department: None,
        position: None,
        permissions: None,
        is_active: true,
    }
}

/// Creates an operator with custom ID and saves to database.
pub fn create_test_operator(db: &Database, id: &str, name: &str) {
    let op = OperatorRow {
        id: op_id(id),
        code: format!("C{id}"),
        name: op_full_name(name, &format!("مشغل {id}")),
        ..sample_operator_row()
    };
    db.save_operator(&op).expect("Failed to save operator");
}

/// Creates a sample product row for database operations.
pub fn sample_product_row() -> ProductRow {
    ProductRow {
        id: "prod-001".to_string(),
        sku: "SKU001".to_string(),
        barcode: Some("1234567890123".to_string()),
        name: "Test Product".to_string(),
        name_ar: Some("منتج تجريبي".to_string()),
        price: Decimal::from(10),
        tax_rate: Decimal::from(15),
        tax_inclusive: false,
        category_id: None, // No category for basic tests
        stock_qty: 100,
        is_active: true,
        unit: "unit".to_string(),
        ..Default::default()
    }
}

/// Creates a product row with custom values.
pub fn create_product_row(id: &str, name: &str, price: Decimal) -> ProductRow {
    ProductRow {
        id: id.to_string(),
        sku: format!("SKU-{}", id),
        barcode: Some(format!("{:013}", id.len())),
        name: name.to_string(),
        name_ar: Some(format!("منتج {}", id)),
        price,
        tax_rate: Decimal::from(15),
        tax_inclusive: false,
        category_id: None,
        stock_qty: 100,
        is_active: true,
        unit: "unit".to_string(),
        ..Default::default()
    }
}

/// Creates a sample cart with items.
pub fn sample_cart() -> Cart {
    let product = sample_product();
    let mut cart = Cart::new();
    cart.items.push(CartItem::new(&product, Decimal::from(2)));
    cart.customer_id = Some("cust-001".to_string());
    cart.customer_name = Some("Test Customer".to_string());
    cart.recalculate();
    cart
}

/// Creates a cart with a specific product and quantity.
pub fn create_cart_with_item(product: &Product, quantity: Decimal) -> Cart {
    let mut cart = Cart::new();
    cart.items.push(CartItem::new(product, quantity));
    cart.recalculate();
    cart
}

/// Creates a shift in the database.
pub fn create_test_shift(db: &Database, shift_id: &str, operator_id: &OperatorId) -> ShiftRow {
    db.start_shift(
        shift_id,
        &format!("SHIFT-{}", shift_id),
        operator_id,
        Some("TERM-01"),
        Decimal::from(100),
    )
    .expect("Failed to create shift")
}

/// Saves a product to the database.
pub fn save_product_to_db(db: &Database, product: &ProductRow) {
    db.save_product(product).expect("Failed to save product");
}

/// Creates multiple products in the database for testing.
pub fn create_test_products(db: &Database, count: usize) {
    for i in 0..count {
        let product = ProductRow {
            id: format!("prod-{:03}", i),
            sku: format!("SKU{:03}", i),
            barcode: Some(format!("{:013}", i)),
            name: format!("Product {}", i),
            name_ar: Some(format!("منتج {}", i)),
            price: Decimal::from((i + 1) as i64 * 10),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            category_id: None,
            stock_qty: 100,
            is_active: true,
            unit: "unit".to_string(),
            ..Default::default()
        };
        db.save_product(&product).expect("Failed to save product");
    }
}

// ============================================================================
// E2E Test Infrastructure
// ============================================================================

/// Complete application state for E2E testing.
///
/// Wraps all services needed for end-to-end workflow tests with an isolated
/// in-memory database for each test instance.
pub struct TestApp {
    pub db: Arc<Database>,
    pub api: Arc<ApiClient>,
    pub auth_service: AuthService,
    pub product_service: ProductService,
    pub cart_service: CartService,
    pub transaction_service: TransactionService,
    pub shift_service: ShiftService,
    pub draft_service: DraftService,
    pub offline_service: OfflineService,
}

impl TestApp {
    /// Creates a new test application with fresh in-memory database.
    pub fn new() -> Self {
        let db = Arc::new(setup_test_db());
        let api = setup_mock_api_arc();

        Self {
            db: db.clone(),
            api: api.clone(),
            auth_service: AuthService::new(api.clone(), db.clone()),
            product_service: ProductService::new(db.clone()),
            cart_service: CartService::new(),
            transaction_service: TransactionService::new(api.clone(), db.clone()),
            shift_service: ShiftService::new(api.clone(), db.clone()),
            draft_service: DraftService::new(db.clone()),
            offline_service: OfflineService::new(api.clone(), db.clone()),
        }
    }

    /// Seeds test data with proper foreign key order.
    ///
    /// Creates:
    /// - 1 test operator (op-001; the till stores no PIN — see schema v13)
    /// - 20 test products (prod-001 through prod-020)
    pub fn seed_test_data(&self) {
        // 1. Create operator FIRST (required for foreign keys)
        //
        // No PIN. The till holds no credential to verify one against — the platform withdrew
        // `pinHash` and schema v13 dropped the column — so a fixture that seeded a hash would be
        // seeding a shape the store no longer has.
        let operator = OperatorRow {
            name: op_full_name("أحمد محمد", "أحمد محمد"),
            ..sample_operator_row()
        };
        self.db
            .save_operator(&operator)
            .expect("Failed to save operator");

        // 2. Create products (no category_id to avoid FK issues)
        for i in 1..=20 {
            let product = ProductRow {
                id: format!("prod-{:03}", i),
                sku: format!("SKU{:03}", i),
                barcode: Some(format!("{:013}", 1000000000000u64 + i as u64)),
                name: format!("Product {}", i),
                name_ar: Some(format!("منتج {}", i)),
                price: Decimal::from(i as i64 * 10),
                tax_rate: Decimal::from(15),
                tax_inclusive: false,
                category_id: None,
                stock_qty: 100,
                is_active: true,
                unit: "unit".to_string(),
                ..Default::default()
            };
            self.db
                .save_product(&product)
                .expect("Failed to save product");
        }
    }

    /// Seeds minimal test data (1 operator, 5 products).
    pub fn seed_minimal_data(&self) {
        // Create operator. No PIN — see `seed_test_data`.
        let operator = sample_operator_row();
        self.db
            .save_operator(&operator)
            .expect("Failed to save operator");

        // Create 5 products
        for i in 1..=5 {
            let product = ProductRow {
                id: format!("prod-{:03}", i),
                sku: format!("SKU{:03}", i),
                barcode: Some(format!("{:013}", 1000000000000u64 + i as u64)),
                name: format!("Product {}", i),
                name_ar: Some(format!("منتج {}", i)),
                price: Decimal::from(i as i64 * 10),
                tax_rate: Decimal::from(15),
                tax_inclusive: false,
                category_id: None,
                stock_qty: 100,
                is_active: true,
                unit: "unit".to_string(),
                ..Default::default()
            };
            self.db
                .save_product(&product)
                .expect("Failed to save product");
        }
    }

    /// Creates a shift in the database and returns its ID.
    ///
    /// Uses the database directly to avoid async requirements in sync tests.
    pub fn create_shift(
        &self,
        shift_id: &str,
        operator_id: &OperatorId,
        opening_cash: Decimal,
    ) -> String {
        let shift_number = format!("SHIFT-{}", &shift_id[..6.min(shift_id.len())]);
        self.db
            .start_shift(
                shift_id,
                &shift_number,
                operator_id,
                Some("TERM-01"),
                opening_cash,
            )
            .expect("Failed to create shift");
        shift_id.to_string()
    }
}

impl Default for TestApp {
    fn default() -> Self {
        Self::new()
    }
}
