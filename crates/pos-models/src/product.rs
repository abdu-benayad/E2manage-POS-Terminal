//! Product Domain Models
//!
//! This module contains the core domain models for products and categories
//! in the POS terminal.

use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::parse::ParseError;

/// Product type classification
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProductType {
    #[default]
    PhysicalGood,
    Consumable,
    Bundle,
    Service,
    Fee,
    Labor,
    Digital,
    Subscription,
    Rental,
    Warranty,
    RawMaterial,
}

impl ProductType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProductType::PhysicalGood => "PHYSICAL_GOOD",
            ProductType::Consumable => "CONSUMABLE",
            ProductType::Bundle => "BUNDLE",
            ProductType::Service => "SERVICE",
            ProductType::Fee => "FEE",
            ProductType::Labor => "LABOR",
            ProductType::Digital => "DIGITAL",
            ProductType::Subscription => "SUBSCRIPTION",
            ProductType::Rental => "RENTAL",
            ProductType::Warranty => "WARRANTY",
            ProductType::RawMaterial => "RAW_MATERIAL",
        }
    }
}

impl FromStr for ProductType {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().replace('-', "_").as_str() {
            "PHYSICAL_GOOD" => Ok(ProductType::PhysicalGood),
            "CONSUMABLE" => Ok(ProductType::Consumable),
            "BUNDLE" => Ok(ProductType::Bundle),
            "SERVICE" => Ok(ProductType::Service),
            "FEE" => Ok(ProductType::Fee),
            "LABOR" => Ok(ProductType::Labor),
            "DIGITAL" => Ok(ProductType::Digital),
            "SUBSCRIPTION" => Ok(ProductType::Subscription),
            "RENTAL" => Ok(ProductType::Rental),
            "WARRANTY" => Ok(ProductType::Warranty),
            "RAW_MATERIAL" => Ok(ProductType::RawMaterial),
            _ => Err(ParseError::ProductType(s.to_string())),
        }
    }
}

impl std::fmt::Display for ProductType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Product nature classification
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProductNature {
    #[default]
    Tangible,
    Intangible,
    Hybrid,
}

impl ProductNature {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProductNature::Tangible => "TANGIBLE",
            ProductNature::Intangible => "INTANGIBLE",
            ProductNature::Hybrid => "HYBRID",
        }
    }
}

impl FromStr for ProductNature {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "TANGIBLE" => Ok(ProductNature::Tangible),
            "INTANGIBLE" => Ok(ProductNature::Intangible),
            "HYBRID" => Ok(ProductNature::Hybrid),
            _ => Err(ParseError::ProductNature(s.to_string())),
        }
    }
}

impl std::fmt::Display for ProductNature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Product domain model
///
/// Represents a product in the POS system with all pricing and metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Product {
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
    pub unit: ProductUnit,
    pub stock_qty: i32,
    pub min_stock: i32,
    pub allow_negative_stock: bool,
    pub image_url: Option<String>,
    pub is_weighable: bool,
    pub is_serialized: bool,
    pub is_active: bool,
    /// Product type classification (Phase 3 Track H)
    #[serde(default)]
    pub product_type: ProductType,
    /// Whether this product tracks inventory
    #[serde(default = "default_true")]
    pub track_inventory: bool,
    /// Product nature: TANGIBLE, INTANGIBLE, HYBRID
    #[serde(default)]
    pub product_nature: ProductNature,
}

fn default_true() -> bool {
    true
}

impl Product {
    /// Returns the display name based on locale
    ///
    /// # Arguments
    /// * `locale` - The locale code ("ar" for Arabic, anything else for English)
    ///
    /// # Example
    /// ```rust,ignore
    /// let name = product.display_name("ar"); // Returns Arabic name if available
    /// ```
    pub fn display_name(&self, locale: &str) -> &str {
        if locale == "ar" {
            self.name_ar.as_deref().unwrap_or(&self.name)
        } else {
            &self.name
        }
    }

    /// Calculates the price with tax applied
    ///
    /// If the product is tax-inclusive, returns the original price.
    /// Otherwise, calculates price + tax.
    pub fn price_with_tax(&self) -> Decimal {
        if self.tax_inclusive {
            self.price
        } else {
            self.price * (Decimal::ONE + self.tax_rate / Decimal::from(100))
        }
    }

    /// Calculates the tax amount for a given quantity
    pub fn tax_amount(&self, quantity: Decimal) -> Decimal {
        if self.tax_inclusive {
            // Extract tax from inclusive price
            let net = self.price / (Decimal::ONE + self.tax_rate / Decimal::from(100));
            (self.price - net) * quantity
        } else {
            self.price * (self.tax_rate / Decimal::from(100)) * quantity
        }
    }

    /// Calculates the line total for a given quantity
    pub fn line_total(&self, quantity: Decimal) -> Decimal {
        self.price_with_tax() * quantity
    }

    /// Checks if the product is in stock.
    /// Non-inventory products (services, fees, etc.) are always in stock.
    pub fn is_in_stock(&self) -> bool {
        if !self.track_inventory {
            return true;
        }
        self.stock_qty > 0 || self.allow_negative_stock
    }

    /// Checks if the product can be sold with the given quantity.
    /// Non-inventory products can always be sold regardless of stock.
    pub fn can_sell(&self, quantity: i32) -> bool {
        if !self.track_inventory {
            return true;
        }
        self.allow_negative_stock || self.stock_qty >= quantity
    }

    /// Returns true if this product requires weighing.
    /// Only tangible products can require weighing.
    pub fn requires_weighing(&self) -> bool {
        if self.product_nature != ProductNature::Tangible {
            return false;
        }
        self.is_weighable
    }

    /// Returns true if this product requires serial number.
    /// Only tangible products can require serial numbers.
    pub fn requires_serial(&self) -> bool {
        if self.product_nature != ProductNature::Tangible {
            return false;
        }
        self.is_serialized
    }
}

impl Default for Product {
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
            unit: ProductUnit::Unit,
            stock_qty: 0,
            min_stock: 0,
            allow_negative_stock: false,
            image_url: None,
            is_weighable: false,
            is_serialized: false,
            is_active: true,
            product_type: ProductType::PhysicalGood,
            track_inventory: true,
            product_nature: ProductNature::Tangible,
        }
    }
}

/// Product unit of measure
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProductUnit {
    #[default]
    Unit,
    Kg,
    Gram,
    Liter,
    Ml,
    Meter,
    Cm,
    Box,
    Pack,
    Dozen,
    Piece,
}

impl ProductUnit {
    /// Returns the display label for the unit
    pub fn label(&self, locale: &str) -> &'static str {
        match (self, locale) {
            (ProductUnit::Unit, "ar") => "وحدة",
            (ProductUnit::Unit, _) => "Unit",
            (ProductUnit::Kg, "ar") => "كجم",
            (ProductUnit::Kg, _) => "Kg",
            (ProductUnit::Gram, "ar") => "جرام",
            (ProductUnit::Gram, _) => "g",
            (ProductUnit::Liter, "ar") => "لتر",
            (ProductUnit::Liter, _) => "L",
            (ProductUnit::Ml, "ar") => "مل",
            (ProductUnit::Ml, _) => "mL",
            (ProductUnit::Meter, "ar") => "متر",
            (ProductUnit::Meter, _) => "m",
            (ProductUnit::Cm, "ar") => "سم",
            (ProductUnit::Cm, _) => "cm",
            (ProductUnit::Box, "ar") => "صندوق",
            (ProductUnit::Box, _) => "Box",
            (ProductUnit::Pack, "ar") => "علبة",
            (ProductUnit::Pack, _) => "Pack",
            (ProductUnit::Dozen, "ar") => "درزن",
            (ProductUnit::Dozen, _) => "Dozen",
            (ProductUnit::Piece, "ar") => "قطعة",
            (ProductUnit::Piece, _) => "Pc",
        }
    }

    /// Returns true if this unit typically requires decimal quantities
    pub fn allows_decimal(&self) -> bool {
        matches!(
            self,
            ProductUnit::Kg
                | ProductUnit::Gram
                | ProductUnit::Liter
                | ProductUnit::Ml
                | ProductUnit::Meter
                | ProductUnit::Cm
        )
    }
}

impl std::str::FromStr for ProductUnit {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "UNIT" => Ok(ProductUnit::Unit),
            "KG" => Ok(ProductUnit::Kg),
            "GRAM" | "G" => Ok(ProductUnit::Gram),
            "LITER" | "L" => Ok(ProductUnit::Liter),
            "ML" => Ok(ProductUnit::Ml),
            "METER" | "M" => Ok(ProductUnit::Meter),
            "CM" => Ok(ProductUnit::Cm),
            "BOX" => Ok(ProductUnit::Box),
            "PACK" => Ok(ProductUnit::Pack),
            "DOZEN" => Ok(ProductUnit::Dozen),
            "PIECE" | "PC" => Ok(ProductUnit::Piece),
            _ => Ok(ProductUnit::Unit),
        }
    }
}

impl std::fmt::Display for ProductUnit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            ProductUnit::Unit => "UNIT",
            ProductUnit::Kg => "KG",
            ProductUnit::Gram => "GRAM",
            ProductUnit::Liter => "LITER",
            ProductUnit::Ml => "ML",
            ProductUnit::Meter => "METER",
            ProductUnit::Cm => "CM",
            ProductUnit::Box => "BOX",
            ProductUnit::Pack => "PACK",
            ProductUnit::Dozen => "DOZEN",
            ProductUnit::Piece => "PIECE",
        };
        write!(f, "{}", s)
    }
}

/// Category domain model
///
/// Represents a product category for organizing products.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Category {
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

impl Category {
    /// Returns the display name based on locale
    pub fn display_name(&self, locale: &str) -> &str {
        if locale == "ar" {
            self.name_ar.as_deref().unwrap_or(&self.name)
        } else {
            &self.name
        }
    }

    /// Returns true if this is a root category (no parent)
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Returns the color as a hex string, or default if not set
    pub fn color_or_default(&self) -> &str {
        self.color.as_deref().unwrap_or("#6366f1") // Default indigo
    }
}

impl Default for Category {
    fn default() -> Self {
        Self {
            id: String::new(),
            parent_id: None,
            name: String::new(),
            name_ar: None,
            color: None,
            icon: None,
            image_url: None,
            display_order: 0,
            is_active: true,
        }
    }
}

/// Search result containing products and search metadata
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductSearchResult {
    pub products: Vec<Product>,
    pub query: String,
    pub total_count: usize,
    pub search_time_ms: u64,
}

impl ProductSearchResult {
    pub fn empty(query: &str) -> Self {
        Self {
            products: vec![],
            query: query.to_string(),
            total_count: 0,
            search_time_ms: 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.products.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_product_display_name() {
        let product = Product {
            name: "Milk".to_string(),
            name_ar: Some("حليب".to_string()),
            ..Default::default()
        };

        assert_eq!(product.display_name("en"), "Milk");
        assert_eq!(product.display_name("ar"), "حليب");
        assert_eq!(product.display_name("fr"), "Milk"); // Falls back to English
    }

    #[test]
    fn test_product_display_name_no_arabic() {
        let product = Product {
            name: "Milk".to_string(),
            name_ar: None,
            ..Default::default()
        };

        assert_eq!(product.display_name("ar"), "Milk"); // Falls back to English
    }

    #[test]
    fn test_price_with_tax_exclusive() {
        let product = Product {
            price: Decimal::from(100),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            ..Default::default()
        };

        assert_eq!(product.price_with_tax(), Decimal::from(115));
    }

    #[test]
    fn test_price_with_tax_inclusive() {
        let product = Product {
            price: Decimal::from(115),
            tax_rate: Decimal::from(15),
            tax_inclusive: true,
            ..Default::default()
        };

        assert_eq!(product.price_with_tax(), Decimal::from(115));
    }

    #[test]
    fn test_tax_amount_exclusive() {
        let product = Product {
            price: Decimal::from(100),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            ..Default::default()
        };

        assert_eq!(product.tax_amount(Decimal::from(2)), Decimal::from(30));
    }

    #[test]
    fn test_tax_amount_inclusive() {
        let product = Product {
            price: Decimal::from(115),
            tax_rate: Decimal::from(15),
            tax_inclusive: true,
            ..Default::default()
        };

        // Tax from 115 inclusive at 15% = 115 - (115/1.15) = 115 - 100 = 15
        assert_eq!(product.tax_amount(Decimal::ONE), Decimal::from(15));
    }

    #[test]
    fn test_line_total() {
        let product = Product {
            price: Decimal::from(10),
            tax_rate: Decimal::from(10),
            tax_inclusive: false,
            ..Default::default()
        };

        assert_eq!(product.line_total(Decimal::from(3)), Decimal::from(33));
    }

    #[test]
    fn test_is_in_stock() {
        let mut product = Product {
            stock_qty: 10,
            allow_negative_stock: false,
            ..Default::default()
        };

        assert!(product.is_in_stock());

        product.stock_qty = 0;
        assert!(!product.is_in_stock());

        product.allow_negative_stock = true;
        assert!(product.is_in_stock()); // Can sell even at 0
    }

    #[test]
    fn test_can_sell() {
        let product = Product {
            stock_qty: 5,
            allow_negative_stock: false,
            ..Default::default()
        };

        assert!(product.can_sell(5));
        assert!(!product.can_sell(6));

        let product_negative = Product {
            stock_qty: 0,
            allow_negative_stock: true,
            ..Default::default()
        };

        assert!(product_negative.can_sell(100)); // Can oversell
    }

    #[test]
    fn test_product_unit_label() {
        assert_eq!(ProductUnit::Kg.label("en"), "Kg");
        assert_eq!(ProductUnit::Kg.label("ar"), "كجم");
        assert_eq!(ProductUnit::Unit.label("en"), "Unit");
        assert_eq!(ProductUnit::Unit.label("ar"), "وحدة");
    }

    #[test]
    fn test_product_unit_allows_decimal() {
        assert!(ProductUnit::Kg.allows_decimal());
        assert!(ProductUnit::Liter.allows_decimal());
        assert!(!ProductUnit::Unit.allows_decimal());
        assert!(!ProductUnit::Box.allows_decimal());
    }

    #[test]
    fn test_product_unit_from_str() {
        assert_eq!("KG".parse::<ProductUnit>().unwrap(), ProductUnit::Kg);
        assert_eq!("kg".parse::<ProductUnit>().unwrap(), ProductUnit::Kg);
        assert_eq!("UNIT".parse::<ProductUnit>().unwrap(), ProductUnit::Unit);
        assert_eq!("INVALID".parse::<ProductUnit>().unwrap(), ProductUnit::Unit);
        // Default
    }

    #[test]
    fn test_category_display_name() {
        let category = Category {
            name: "Food".to_string(),
            name_ar: Some("طعام".to_string()),
            ..Default::default()
        };

        assert_eq!(category.display_name("en"), "Food");
        assert_eq!(category.display_name("ar"), "طعام");
    }

    #[test]
    fn test_category_is_root() {
        let root = Category {
            parent_id: None,
            ..Default::default()
        };

        let child = Category {
            parent_id: Some("parent-1".to_string()),
            ..Default::default()
        };

        assert!(root.is_root());
        assert!(!child.is_root());
    }

    #[test]
    fn test_category_color_or_default() {
        let with_color = Category {
            color: Some("#FF5722".to_string()),
            ..Default::default()
        };

        let without_color = Category {
            color: None,
            ..Default::default()
        };

        assert_eq!(with_color.color_or_default(), "#FF5722");
        assert_eq!(without_color.color_or_default(), "#6366f1");
    }

    #[test]
    fn test_product_search_result() {
        let result = ProductSearchResult::empty("test");
        assert!(result.is_empty());
        assert_eq!(result.query, "test");
        assert_eq!(result.total_count, 0);
    }

    #[test]
    fn test_product_type_raw_material_from_str() {
        assert_eq!(
            ProductType::from_str("RAW_MATERIAL"),
            Ok(ProductType::RawMaterial)
        );
        // Also accepts lowercase and mixed case
        assert_eq!(
            ProductType::from_str("raw_material"),
            Ok(ProductType::RawMaterial)
        );
    }

    #[test]
    fn test_product_type_raw_material_as_str() {
        assert_eq!(ProductType::RawMaterial.as_str(), "RAW_MATERIAL");
    }

    #[test]
    fn test_product_type_raw_material_display() {
        assert_eq!(format!("{}", ProductType::RawMaterial), "RAW_MATERIAL");
    }

    #[test]
    fn test_product_type_unknown_is_rejected_and_named() {
        // Was: unknown strings silently became PhysicalGood, which is the variant that decides
        // `track_inventory`. The parse now fails and names the offending value; a caller that
        // wants the old behaviour asks for it explicitly.
        assert_eq!(
            ProductType::from_str("TOTALLY_UNKNOWN_TYPE"),
            Err(ParseError::ProductType("TOTALLY_UNKNOWN_TYPE".to_string()))
        );
        let message = ProductType::from_str("TOTALLY_UNKNOWN_TYPE")
            .unwrap_err()
            .to_string();
        assert!(message.contains("TOTALLY_UNKNOWN_TYPE"), "{message}");
        assert_eq!(
            "TOTALLY_UNKNOWN_TYPE"
                .parse::<ProductType>()
                .unwrap_or_default(),
            ProductType::PhysicalGood
        );
    }

    #[test]
    fn product_nature_unknown_is_rejected_and_named() {
        assert_eq!(
            ProductNature::from_str("NOT_A_NATURE"),
            Err(ParseError::ProductNature("NOT_A_NATURE".to_string()))
        );
        assert_eq!(ProductNature::from_str("HYBRID"), Ok(ProductNature::Hybrid));
    }

    #[test]
    fn test_product_type_all_known_variants_round_trip() {
        let variants = [
            (ProductType::PhysicalGood, "PHYSICAL_GOOD"),
            (ProductType::Consumable, "CONSUMABLE"),
            (ProductType::Bundle, "BUNDLE"),
            (ProductType::Service, "SERVICE"),
            (ProductType::Fee, "FEE"),
            (ProductType::Labor, "LABOR"),
            (ProductType::Digital, "DIGITAL"),
            (ProductType::Subscription, "SUBSCRIPTION"),
            (ProductType::Rental, "RENTAL"),
            (ProductType::Warranty, "WARRANTY"),
            (ProductType::RawMaterial, "RAW_MATERIAL"),
        ];

        for (variant, s) in &variants {
            assert_eq!(variant.as_str(), *s, "as_str mismatch for {:?}", variant);
            assert_eq!(
                ProductType::from_str(s),
                Ok(variant.clone()),
                "from_str mismatch for {}",
                s
            );
        }
    }
}
