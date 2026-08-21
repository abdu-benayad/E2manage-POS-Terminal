//! Sync DTOs - Data transfer objects for synchronization
//!
//! These types represent the JSON responses from the E2Manage backend sync endpoints.

use serde::{Deserialize, Serialize};

use pos_models::OperatorRole;

/// Response from /api/pos/sync/catalog endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogResponse {
    /// List of products
    pub products: Vec<ProductDto>,
    /// List of categories (may be empty - categories come from products)
    #[serde(default)]
    pub categories: Vec<CategoryDto>,
    /// Catalog version string (for display/logging)
    #[serde(default)]
    pub version: Option<String>,
    /// Last updated timestamp
    #[serde(default)]
    pub last_updated: Option<String>,
}

/// Response from /api/pos/sync/catalog/delta endpoint
///
/// Returns only products that have been modified (updated or deleted) since
/// the provided timestamp. This is used for efficient incremental sync.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogDeltaResponse {
    /// Products that were updated since the given timestamp
    #[serde(default)]
    pub updated: Vec<ProductDto>,
    /// IDs of products that were deleted since the given timestamp
    #[serde(default)]
    pub deleted: Vec<String>,
    /// Categories that were updated (if includeCategories=true)
    #[serde(default)]
    pub categories_updated: Vec<CategoryDto>,
    /// IDs of categories that were deleted (if includeCategories=true)
    #[serde(default)]
    pub categories_deleted: Vec<String>,
    /// Current catalog version (ETag)
    #[serde(default)]
    pub version: String,
    /// Timestamp for this delta response
    #[serde(default)]
    pub synced_at: String,
    /// Total number of products in the catalog (for validation)
    #[serde(default)]
    pub total_product_count: i32,
}

/// Product DTO from sync endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductDto {
    pub id: String,
    #[serde(default)]
    pub code: Option<String>,
    pub sku: String,
    #[serde(default)]
    pub barcode: Option<String>,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub price: f64,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub tax_rate: f64,
    #[serde(default)]
    pub tax_inclusive: bool,
    #[serde(default)]
    pub taxable: bool,
    #[serde(default)]
    pub category_id: Option<String>,
    #[serde(default)]
    pub category_name: Option<String>,
    /// Unit can be string or object - we'll handle both
    #[serde(default, deserialize_with = "deserialize_unit")]
    pub unit: String,
    /// Stock quantity (API sends as stockQuantity)
    #[serde(default, alias = "stock_qty")]
    pub stock_quantity: i32,
    #[serde(default)]
    pub min_stock: i32,
    #[serde(default)]
    pub allow_negative_stock: bool,
    #[serde(default)]
    pub allow_decimal_qty: bool,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub is_weighable: bool,
    #[serde(default)]
    pub is_serialized: bool,
    #[serde(default = "default_true")]
    pub is_active: bool,
    /// Product type classification (Phase 3 Track H)
    #[serde(default = "default_product_type")]
    pub product_type: String,
    /// Whether this product tracks inventory
    #[serde(default = "default_true")]
    pub track_inventory: bool,
    /// Product nature: TANGIBLE, INTANGIBLE, HYBRID
    #[serde(default = "default_product_nature")]
    pub product_nature: String,
}

/// Unit object from API
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnitDto {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub name_ar: Option<String>,
}

/// Deserialize unit field - can be string or object
fn deserialize_unit<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct UnitVisitor;

    impl<'de> Visitor<'de> for UnitVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("string or unit object")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<String, E> {
            Ok(v.to_string())
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<String, E> {
            Ok(v)
        }

        fn visit_map<M>(self, map: M) -> Result<String, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            // Deserialize as UnitDto and extract code or symbol
            let unit: UnitDto =
                de::Deserialize::deserialize(de::value::MapAccessDeserializer::new(map))?;
            Ok(unit
                .code
                .or(unit.symbol)
                .unwrap_or_else(|| "UNIT".to_string()))
        }

        fn visit_unit<E: de::Error>(self) -> Result<String, E> {
            Ok("UNIT".to_string())
        }
    }

    deserializer.deserialize_any(UnitVisitor)
}

fn default_true() -> bool {
    true
}

fn default_product_type() -> String {
    "PHYSICAL_GOOD".to_string()
}

fn default_product_nature() -> String {
    "TANGIBLE".to_string()
}

/// Category DTO from sync endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryDto {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub display_order: i32,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

/// Response from /api/pos/sync/operators endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorsResponse {
    /// List of operators
    pub operators: Vec<OperatorDto>,
}

/// Operator DTO from sync endpoint (HR Employee integrated)
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorDto {
    /// POS Operator Profile ID
    pub id: String,
    /// HR Employee ID
    #[serde(default)]
    pub employee_id: Option<String>,
    /// HR Employee Number (e.g., "EMP001")
    #[serde(default)]
    pub employee_number: Option<String>,
    /// Full name (English)
    pub name: String,
    /// Full name (Arabic)
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    /// BCrypt hashed PIN
    #[serde(default)]
    pub pin_hash: Option<String>,
    /// POS role, as the server's `POS_OperatorRole` enum defines it.
    ///
    /// No serde default. The server's column is non-null and `getOperators` always emits it, so
    /// an absent or unrecognised role means the contract moved — and the default this replaces
    /// (`"CASHIER"`) was a privilege decision made by a fallback.
    pub role: OperatorRole,
    /// HR Department name
    #[serde(default)]
    pub department: Option<String>,
    /// HR Position/Job title
    #[serde(default)]
    pub position: Option<String>,
    /// Permissions object from API
    #[serde(default)]
    pub permissions: Option<OperatorPermissions>,
    /// Whether employee is active in HR
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Operator permissions from API
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OperatorPermissions {
    #[serde(default)]
    pub can_void: bool,
    #[serde(default)]
    pub can_refund: bool,
    #[serde(default)]
    pub can_discount: bool,
    #[serde(default)]
    pub can_open_drawer: bool,
    #[serde(default)]
    pub can_manage_shift: bool,
    #[serde(default)]
    pub can_view_reports: bool,
    #[serde(default)]
    pub max_discount_percent: Option<f64>,
}

/// Response from /api/pos/sync/screens endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreensResponse {
    /// List of screen definitions
    pub screens: Vec<ScreenDefinitionDto>,
    /// Screens version string
    #[serde(default)]
    pub version: Option<String>,
}

/// Screen definition DTO
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenDefinitionDto {
    pub screen_id: String,
    pub name: String,
    #[serde(default)]
    pub version: i32,
    /// JSON definition for the screen layout
    pub definition: serde_json::Value,
}

/// Response from /api/pos/sync/customers endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomersResponse {
    /// List of customers
    pub customers: Vec<CustomerDto>,
}

/// Customer DTO from sync endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomerDto {
    pub id: String,
    #[serde(default)]
    pub code: Option<String>,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub credit_limit: f64,
    #[serde(default)]
    pub current_balance: f64,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

/// Response from /api/pos/sync/payment-methods endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethodsResponse {
    /// List of payment methods
    pub payment_methods: Vec<PaymentMethodDto>,
}

/// Payment method DTO from sync endpoint
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaymentMethodDto {
    pub id: String,
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub name_ar: Option<String>,
    #[serde(rename = "type")]
    pub method_type: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub requires_reference: bool,
    #[serde(default)]
    pub requires_customer: bool,
    #[serde(default = "default_true")]
    pub is_active: bool,
}

// ============================================================================
// Conversion implementations from DTOs to database rows
// ============================================================================

impl ProductDto {
    /// Converts to ProductRow for database storage
    pub fn to_product_row(&self) -> pos_db::ProductRow {
        use rust_decimal::Decimal;
        pos_db::ProductRow {
            id: self.id.clone(),
            sku: self.sku.clone(),
            barcode: self.barcode.clone(),
            name: self.name.clone(),
            name_ar: self.name_ar.clone(),
            description: self.description.clone(),
            price: Decimal::from_f64_retain(self.price).unwrap_or(Decimal::ZERO),
            cost: Decimal::from_f64_retain(self.cost).unwrap_or(Decimal::ZERO),
            tax_rate: Decimal::from_f64_retain(self.tax_rate).unwrap_or(Decimal::ZERO),
            tax_inclusive: self.tax_inclusive,
            category_id: self.category_id.clone(),
            category_name: self.category_name.clone(),
            unit: self.unit.clone(),
            stock_qty: self.stock_quantity,
            min_stock: self.min_stock,
            allow_negative_stock: self.allow_negative_stock,
            image_url: self.image_url.clone(),
            is_weighable: self.is_weighable,
            is_serialized: self.is_serialized,
            is_active: self.is_active,
            product_type: self.product_type.clone(),
            track_inventory: self.track_inventory,
            product_nature: self.product_nature.clone(),
        }
    }
}

impl CategoryDto {
    /// Converts to CategoryRow for database storage
    pub fn to_category_row(&self) -> pos_db::CategoryRow {
        pos_db::CategoryRow {
            id: self.id.clone(),
            parent_id: self.parent_id.clone(),
            name: self.name.clone(),
            name_ar: self.name_ar.clone(),
            color: self.color.clone(),
            icon: self.icon.clone(),
            image_url: self.image_url.clone(),
            display_order: self.display_order,
            is_active: self.is_active,
        }
    }
}

impl OperatorDto {
    /// Converts to OperatorRow for database storage
    pub fn to_operator_row(&self) -> pos_db::OperatorRow {
        // Serialize permissions to JSON string
        let permissions_json = self
            .permissions
            .as_ref()
            .map(|p| serde_json::to_string(p).unwrap_or_default());

        // Use employee_number as code, or generate from ID if not available
        let code = self
            .employee_number
            .clone()
            .unwrap_or_else(|| format!("OP-{}", &self.id[..8.min(self.id.len())]));

        pos_db::OperatorRow {
            id: self.id.clone(),
            code,
            employee_id: self.employee_id.clone(),
            employee_number: self.employee_number.clone(),
            name: self.name.clone(),
            name_ar: self.name_ar.clone(),
            pin_hash: self.pin_hash.clone().unwrap_or_default(),
            role: self.role,
            department: self.department.clone(),
            position: self.position.clone(),
            permissions_json,
            is_active: self.is_active,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_catalog_response() {
        let json = r#"{
            "products": [
                {
                    "id": "p1",
                    "sku": "SKU001",
                    "name": "Test Product",
                    "price": 10.50
                }
            ],
            "categories": [
                {
                    "id": "c1",
                    "name": "Category 1"
                }
            ],
            "version": "v1.0"
        }"#;

        let response: CatalogResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.products.len(), 1);
        assert_eq!(response.categories.len(), 1);
        assert_eq!(response.version, Some("v1.0".to_string()));
        assert_eq!(response.products[0].id, "p1");
        assert_eq!(response.products[0].price, 10.50);
    }

    #[test]
    fn test_deserialize_product_with_defaults() {
        let json = r#"{
            "id": "p1",
            "sku": "SKU001",
            "name": "Minimal Product"
        }"#;

        let product: ProductDto = serde_json::from_str(json).unwrap();
        assert_eq!(product.id, "p1");
        assert_eq!(product.price, 0.0);
        assert!(product.unit.is_empty() || product.unit == "UNIT"); // default when unit field is missing
        assert!(product.is_active);
        assert!(product.barcode.is_none());
    }

    #[test]
    fn test_deserialize_operators_response() {
        let json = r#"{
            "operators": [
                {
                    "id": "op1",
                    "employeeNumber": "C001",
                    "name": "Ahmed",
                    "pinHash": "$2b$12$hash...",
                    "role": "SUPERVISOR"
                }
            ]
        }"#;

        let response: OperatorsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.operators.len(), 1);
        assert_eq!(response.operators[0].name, "Ahmed");
        assert_eq!(response.operators[0].role, OperatorRole::Supervisor);
    }

    #[test]
    fn test_operator_without_a_role_is_a_contract_breach() {
        // This assertion is inverted from what it was. The DTO used to carry
        // `#[serde(default = "default_role")]`, so an operator arriving with no role became a
        // cashier — and this test asserted that, which made it a test that encoded the defect.
        //
        // `POS_OperatorProfile.role` is non-null and `getOperators` always emits it, so an
        // absent role means the contract moved. Reading it as the least-privileged role would be
        // a privilege decision made by a fallback, and the fallback is exactly what hides the
        // fact that the two systems now disagree.
        let json = r#"{"operators":[{"id":"op1","name":"Ahmed","pinHash":"$2b$12$h"}]}"#;

        let refused = serde_json::from_str::<OperatorsResponse>(json).unwrap_err();
        assert!(
            refused.to_string().contains("missing field `role`"),
            "got: {refused}"
        );
    }

    #[test]
    fn test_operator_with_an_unknown_role_is_refused() {
        // The server's enum admits three values. A fourth means the contract moved.
        let json = r#"{"operators":[{"id":"op1","name":"Ahmed","role":"ADMIN"}]}"#;

        assert!(serde_json::from_str::<OperatorsResponse>(json).is_err());
    }

    #[test]
    fn test_deserialize_category_with_all_fields() {
        let json = r##"{
            "id": "c1",
            "parentId": "c0",
            "name": "Food",
            "nameAr": "طعام",
            "color": "#FF5722",
            "icon": "food",
            "displayOrder": 1,
            "isActive": true
        }"##;

        let category: CategoryDto = serde_json::from_str(json).unwrap();
        assert_eq!(category.id, "c1");
        assert_eq!(category.parent_id, Some("c0".to_string()));
        assert_eq!(category.name_ar, Some("طعام".to_string()));
        assert_eq!(category.color, Some("#FF5722".to_string()));
    }

    #[test]
    fn test_product_to_row_conversion() {
        let dto = ProductDto {
            id: "p1".to_string(),
            code: None,
            sku: "SKU001".to_string(),
            barcode: Some("123456".to_string()),
            name: "Test".to_string(),
            name_ar: Some("اختبار".to_string()),
            description: None,
            price: 15.99,
            cost: 10.00,
            currency: None,
            tax_rate: 5.0,
            tax_inclusive: false,
            taxable: true,
            category_id: Some("c1".to_string()),
            category_name: Some("Food".to_string()),
            unit: "UNIT".to_string(),
            stock_quantity: 100,
            min_stock: 10,
            allow_negative_stock: false,
            allow_decimal_qty: false,
            image_url: None,
            is_weighable: false,
            is_serialized: false,
            is_active: true,
            product_type: "SERVICE".to_string(),
            track_inventory: false,
            product_nature: "INTANGIBLE".to_string(),
        };

        let row = dto.to_product_row();
        assert_eq!(row.id, "p1");
        assert_eq!(
            row.price,
            rust_decimal::Decimal::from_f64_retain(15.99).unwrap()
        );
        assert_eq!(row.name_ar, Some("اختبار".to_string()));
    }

    #[test]
    fn test_operator_to_row_conversion() {
        let dto = OperatorDto {
            id: "op1".to_string(),
            employee_id: Some("emp-1".to_string()),
            employee_number: Some("EMP001".to_string()),
            name: "Ahmed".to_string(),
            name_ar: Some("أحمد".to_string()),
            email: None,
            pin_hash: Some("$2b$hash".to_string()),
            role: OperatorRole::Cashier,
            department: Some("Sales".to_string()),
            position: Some("Cashier".to_string()),
            permissions: Some(OperatorPermissions {
                can_void: true,
                ..Default::default()
            }),
            is_active: true,
            updated_at: None,
        };

        let row = dto.to_operator_row();
        assert_eq!(row.id, "op1");
        assert_eq!(row.code, "EMP001");
        assert_eq!(row.employee_id, Some("emp-1".to_string()));
        assert_eq!(row.department, Some("Sales".to_string()));
        assert!(row.permissions_json.is_some());
    }

    #[test]
    fn test_deserialize_screens_response() {
        let json = r#"{
            "screens": [
                {
                    "screenId": "main",
                    "name": "Main Screen",
                    "version": 1,
                    "definition": {"layout": "grid"}
                }
            ],
            "version": "v1"
        }"#;

        let response: ScreensResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.screens.len(), 1);
        assert_eq!(response.screens[0].screen_id, "main");
    }

    #[test]
    fn test_deserialize_payment_methods() {
        let json = r#"{
            "paymentMethods": [
                {
                    "id": "pm1",
                    "code": "CASH",
                    "name": "Cash",
                    "type": "CASH"
                }
            ]
        }"#;

        let response: PaymentMethodsResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.payment_methods.len(), 1);
        assert_eq!(response.payment_methods[0].method_type, "CASH");
    }
}
