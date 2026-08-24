//! What a security policy's value *is*, once the till has understood it.
//!
//! # Why this type exists
//!
//! The platform sends each policy as three things: a code, a **declared type**
//! (`BOOLEAN`, `ENUM`, `RANGE`, `LIST`, `REGEX`), and a **value** carried as untyped JSON.
//! The declared type says how to read the value.
//!
//! Nothing in the till read it. [`pos_api::PolicyType`] was deserialised and then consulted
//! exactly once in the whole codebase — an assertion inside its own DTO test. Which interpreter
//! ran was decided by *which check method the caller happened to call*, so asking
//! "is this boolean policy satisfied?" of a policy declared `RANGE` was a well-typed call that
//! nothing rejected: it read the range object as a boolean, failed, and took a permissive default.
//!
//! This type is the missing socket. A `PolicyValue` has been read *against its declaration*, so
//! there is one place where that reading can fail and one place where currency can be declared as
//! [`Decimal`] rather than reached for as a float at the point of use.
//!
//! # The three shapes of not-understood, and why they are three
//!
//! [`PolicyValue::UnknownType`] and [`PolicyValue::Malformed`] are separate on purpose. *"The
//! platform sent me a malformed range"* and *"the platform sent me a policy type I have never heard
//! of"* are different facts about the platform, and a till that cannot tell them apart cannot
//! report either usefully. Collapsing them was the gap adversarial review found in the design.
//!
//! Neither is an error. An unrecognised policy type must not fail the whole fetch — one unfamiliar
//! policy would take every other policy with it, and a till with no policies is a till that permits
//! everything. So [`from_declared`](PolicyValue::from_declared) is **total**: every input yields a
//! variant, never a `Result`, never a panic.
//!
//! # Why this is hand-written and not derived
//!
//! Established by compiling a probe rather than by reasoning about it: the natural encoding
//! (`#[serde(tag = "policyType", content = "policyValue")]` with a `#[serde(other)]` catch-all)
//! cannot express the tolerant arm. `#[serde(other)]` requires a **unit** variant, so an
//! unrecognised type carrying a real value — a bool, an object, an array — is a hard error, and one
//! bad element takes the entire response's `Vec` down with it. That is the precise cascade the
//! tolerant arm exists to prevent, so the derive would have delivered the opposite of the design.
//!
//! The wire DTO therefore stays flat, and this type is built from it in ordinary code.

use std::str::FromStr;

use pos_api::PolicyType;
use rust_decimal::Decimal;
use serde_json::Value;

/// The inclusive bounds of a range policy.
///
/// # Why `Decimal` and not `f64`
///
/// `OFFLINE_MAX_AMOUNT` is the largest sale a till may complete while offline — money, and this
/// codebase's first rule is that money is never `f64`. The old `RangeValue` reached for
/// `serde_json::Value::as_f64` at the moment of the check, which is the obvious thing to write when
/// a value arrives untyped and there is nowhere to say what it means. Declaring the bounds here is
/// what removes the reach.
///
/// Note that a bound is *not* always currency — `MIN_PIN_LENGTH` is a count. `Decimal` is correct
/// for both, and a count that needs to become an integer converts explicitly at the accessor rather
/// than by a saturating cast.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeBounds {
    min: Decimal,
    max: Decimal,
}

impl RangeBounds {
    /// Builds bounds, refusing an inverted range.
    ///
    /// `min > max` admits no value at all. That is a policy the platform has got wrong, not a range
    /// that happens to be empty, and the caller records it as [`PolicyValue::Malformed`] — the same
    /// standing as a range whose value was not an object. There is deliberately no constructor that
    /// accepts one, so no later code has to re-check.
    pub fn new(min: Decimal, max: Decimal) -> Option<Self> {
        (min <= max).then_some(Self { min, max })
    }

    /// The lower bound.
    pub fn min(&self) -> Decimal {
        self.min
    }

    /// The upper bound.
    pub fn max(&self) -> Decimal {
        self.max
    }

    /// Whether a value falls within the bounds, inclusive.
    pub fn contains(&self, value: Decimal) -> bool {
        self.min <= value && value <= self.max
    }
}

/// A policy value that has been read against the type the platform declared for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyValue {
    /// A feature switch.
    Boolean(bool),
    /// The single value the policy permits.
    Enum(String),
    /// Inclusive numeric bounds.
    Range(RangeBounds),
    /// The values the policy permits. An **empty list is a real, deliberate rule** and is
    /// represented as an empty `Vec` — it is no longer the same value as "I could not read this",
    /// which is [`PolicyValue::Malformed`].
    List(Vec<String>),
    /// A pattern the policy requires.
    Regex(String),
    /// The platform declared a policy type this till has never heard of.
    ///
    /// Carries the value untouched, so a later till that understands it can, and so a diagnostic
    /// can quote what actually arrived.
    UnknownType {
        /// The value exactly as the platform sent it.
        raw: Value,
    },
    /// The platform declared a type this till knows, and the value does not match it.
    ///
    /// This is the shape today's defects actually take — a policy declared `RANGE` whose value is a
    /// string — and it is distinct from [`PolicyValue::UnknownType`] because it says something
    /// different about the platform: not *"you are newer than me"* but *"you contradicted
    /// yourself"*.
    Malformed {
        /// What the platform said the value would be.
        declared: PolicyType,
        /// The value exactly as the platform sent it.
        raw: Value,
    },
}

impl PolicyValue {
    /// Reads a value against its declared type.
    ///
    /// **Total by contract.** Every input produces a variant; there is no error path and no panic.
    /// A policy this till cannot read must not be able to fail a refresh, because a till holding no
    /// policies is a till that permits everything.
    pub fn from_declared(policy_type: &PolicyType, value: &Value) -> Self {
        let malformed = || Self::Malformed {
            declared: policy_type.clone(),
            raw: value.clone(),
        };

        match policy_type {
            PolicyType::Boolean => value.as_bool().map_or_else(malformed, Self::Boolean),
            PolicyType::Enum => value
                .as_str()
                .map_or_else(malformed, |s| Self::Enum(s.to_string())),
            PolicyType::Regex => value
                .as_str()
                .map_or_else(malformed, |s| Self::Regex(s.to_string())),
            PolicyType::Range => parse_bounds(value).map_or_else(malformed, Self::Range),
            PolicyType::List => parse_list(value).map_or_else(malformed, Self::List),
            PolicyType::Unknown => Self::UnknownType { raw: value.clone() },
        }
    }

    /// Whether this value was read successfully against its declaration.
    ///
    /// The complement of the two not-understood arms. Exists so a caller can ask the question once
    /// rather than matching two variants and forgetting the second when a third is added.
    pub fn is_understood(&self) -> bool {
        !matches!(self, Self::UnknownType { .. } | Self::Malformed { .. })
    }
}

/// Reads `{ "min": …, "max": … }` into bounds, or `None` if it is not that.
fn parse_bounds(value: &Value) -> Option<RangeBounds> {
    let object = value.as_object()?;
    let min = decimal_from(object.get("min")?)?;
    let max = decimal_from(object.get("max")?)?;
    RangeBounds::new(min, max)
}

/// Reads an array of strings, refusing one that contains anything else.
///
/// # Why a non-string element poisons the whole list
///
/// The predecessor used `filter_map(|v| v.as_str())`, which **silently dropped** what it could not
/// read. Two consequences, both bad and neither visible:
///
/// - `["CASH", 42, "CARD"]` became a well-formed-looking two-element allow-list, so a partly-wrong
///   rule was enforced as if it were a right one — the hardest case to notice, because the result
///   looks correct.
/// - `[1, 2, 3]` became `[]`, and the caller read an empty allow-list as **allow everything**.
///
/// Composed with the outer `unwrap_or_default`, that gave a security control three separate roads
/// from *malformed* to *permit*. An array with a non-string element is one policy the till cannot
/// read, and it says so.
fn parse_list(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|element| element.as_str().map(str::to_string))
        .collect()
}

/// Converts a JSON number to a `Decimal` through its decimal text.
///
/// # What this does and does not buy, measured rather than assumed
///
/// It does **not** recover the platform's exact decimal text, and the first draft of this module
/// claimed it did. `serde_json` is built here without `arbitrary_precision`, so a JSON number is
/// already an `f64` by the time `SecurityPolicy` finishes deserialising — inside `pos-api`,
/// upstream of everything in this crate. Measured: `123456789012345.678901` arrives as
/// `123456789012345.67`, and no conversion here can undo that. **The precision boundary is the wire
/// DTO, not this function.**
///
/// What it does buy is real and is the reason money is `Decimal` everywhere in this codebase:
/// once past this point the value is exact decimal, so comparisons and arithmetic behave the way
/// the person reading a policy expects. A bound of `0.3` genuinely contains `0.1 + 0.2`, which in
/// binary floating point it does not — that is
/// [`a_bound_compares_in_decimal_not_in_binary_floating_point`] and it fails if anyone swaps this
/// for `as_f64` or for `serde_json::from_value::<Decimal>(v)`. The second is worth naming because
/// it is *well-typed and wrong*: the workspace enables `rust_decimal`'s `serde-with-float`, so it
/// compiles and round-trips through a float with no visible `f64` anywhere.
///
/// [`a_bound_compares_in_decimal_not_in_binary_floating_point`]: tests::a_bound_compares_in_decimal_not_in_binary_floating_point
fn decimal_from(value: &Value) -> Option<Decimal> {
    Decimal::from_str(&value.as_number()?.to_string()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(policy_type: PolicyType, value: Value) -> PolicyValue {
        PolicyValue::from_declared(&policy_type, &value)
    }

    // ---------------------------------------------------------------------
    // The recognised shapes — the positive control for everything below.
    //
    // Without these, a `from_declared` broken open would return `Malformed` for every input,
    // refuse every check, and every negative assertion in this file would still pass. That reads
    // as a finding rather than as a fault, which is the failure this control exists to separate.
    // ---------------------------------------------------------------------

    #[test]
    fn each_declared_type_reads_into_its_own_variant() {
        assert_eq!(
            read(PolicyType::Boolean, json!(true)),
            PolicyValue::Boolean(true)
        );
        assert_eq!(
            read(PolicyType::Enum, json!("STRICT")),
            PolicyValue::Enum("STRICT".to_string())
        );
        assert_eq!(
            read(PolicyType::Regex, json!("^[0-9]{4}$")),
            PolicyValue::Regex("^[0-9]{4}$".to_string())
        );
        assert_eq!(
            read(PolicyType::List, json!(["CASH", "CARD"])),
            PolicyValue::List(vec!["CASH".to_string(), "CARD".to_string()])
        );

        let PolicyValue::Range(bounds) = read(PolicyType::Range, json!({"min": 4, "max": 8}))
        else {
            panic!("a well-formed range reads as a range");
        };
        assert_eq!(bounds.min(), Decimal::from(4));
        assert_eq!(bounds.max(), Decimal::from(8));

        // Every one of the above is understood, which is what the malformed tests contrast with.
        for value in [
            read(PolicyType::Boolean, json!(false)),
            read(PolicyType::Enum, json!("X")),
            read(PolicyType::List, json!([])),
        ] {
            assert!(value.is_understood(), "{value:?}");
        }
    }

    /// An empty allow-list is a **rule**, not an absence.
    ///
    /// This is the distinction the predecessor destroyed: `unwrap_or_default` turned anything it
    /// could not read into `vec![]`, and the caller read `vec![]` as "allow everything". Here the
    /// empty list is `List(vec![])` and unreadable input is `Malformed`, so no later code can
    /// confuse a deliberate rule with a failure to read one.
    #[test]
    fn a_deliberate_empty_list_is_not_the_same_value_as_an_unreadable_one() {
        assert_eq!(read(PolicyType::List, json!([])), PolicyValue::List(vec![]));
        assert!(matches!(
            read(PolicyType::List, json!("CASH")),
            PolicyValue::Malformed { .. }
        ));
    }

    // ---------------------------------------------------------------------
    // The two not-understood arms, and that they stay apart.
    // ---------------------------------------------------------------------

    /// A declared type the till knows, whose value contradicts it.
    #[test]
    fn a_value_that_contradicts_its_declaration_is_malformed() {
        let value = read(PolicyType::Range, json!("not a range"));

        let PolicyValue::Malformed { declared, raw } = value else {
            panic!("a RANGE whose value is a string is malformed, got {value:?}");
        };
        assert_eq!(declared, PolicyType::Range);
        assert_eq!(raw, json!("not a range"), "the original is kept verbatim");
    }

    /// A declared type the till has never heard of, which is a different fact.
    #[test]
    fn an_unrecognised_declared_type_is_not_malformed() {
        let value = read(PolicyType::Unknown, json!({"anything": [1, 2]}));

        let PolicyValue::UnknownType { raw } = value else {
            panic!("an unrecognised type is UnknownType, not Malformed, got {value:?}");
        };
        assert_eq!(raw, json!({"anything": [1, 2]}));
    }

    /// The two arms must not be one arm.
    ///
    /// Asserted directly rather than left to the two tests above, because a refactor that collapsed
    /// them would leave both of those passing if it kept the raw value.
    #[test]
    fn the_two_not_understood_arms_are_distinguishable() {
        let unknown = read(PolicyType::Unknown, json!(1));
        let malformed = read(PolicyType::Boolean, json!(1));

        assert_ne!(unknown, malformed);
        assert!(!unknown.is_understood() && !malformed.is_understood());
    }

    // ---------------------------------------------------------------------
    // Lists: the arm that had three roads from malformed to "allow everything".
    // ---------------------------------------------------------------------

    /// A mixed array is one unreadable policy, not a shorter readable one.
    ///
    /// The predecessor returned `["CASH", "CARD"]` here — a rule that looks entirely well-formed
    /// while silently permitting less than the platform wrote, or more.
    #[test]
    fn a_list_with_a_non_string_element_does_not_become_a_shorter_list() {
        let value = read(PolicyType::List, json!(["CASH", 42, "CARD"]));

        assert!(
            matches!(value, PolicyValue::Malformed { .. }),
            "a partly-unreadable list is malformed, not a two-element allow-list: {value:?}"
        );
    }

    /// An array of no strings at all is malformed — not the empty list that means "allow all".
    ///
    /// This one is the composition of two separate compressions: `as_array` succeeds, then the old
    /// `filter_map` emptied it, and empty was read as permit. Neither step looks wrong alone.
    #[test]
    fn a_list_of_non_strings_is_not_an_empty_allow_everything_list() {
        let value = read(PolicyType::List, json!([1, 2, 3]));

        assert!(
            matches!(value, PolicyValue::Malformed { .. }),
            "an array with no readable elements must not read as the empty rule: {value:?}"
        );
    }

    // ---------------------------------------------------------------------
    // Ranges.
    // ---------------------------------------------------------------------

    /// A range admitting no value is the platform being wrong, not an empty range.
    #[test]
    fn an_inverted_range_is_malformed() {
        assert!(matches!(
            read(PolicyType::Range, json!({"min": 9, "max": 4})),
            PolicyValue::Malformed { .. }
        ));
        assert!(RangeBounds::new(Decimal::from(9), Decimal::from(4)).is_none());

        // Control: the degenerate single-value range is legal, so the constructor is refusing
        // inversion rather than refusing everything.
        assert!(RangeBounds::new(Decimal::from(4), Decimal::from(4)).is_some());
    }

    #[test]
    fn a_range_missing_a_bound_is_malformed() {
        assert!(matches!(
            read(PolicyType::Range, json!({"min": 4})),
            PolicyValue::Malformed { .. }
        ));
    }

    /// The guard against the float shortcut, and the reason it is not a nicety.
    ///
    /// **This test replaces one that asserted something false, and the correction is worth keeping
    /// visible.** The original claimed a 21-digit bound survived exactly. It does not: `serde_json`
    /// is built without `arbitrary_precision`, so the number is an `f64` before `SecurityPolicy`
    /// finishes deserialising in `pos-api`. Measured — `123456789012345.678901` arrives here as
    /// `123456789012345.67`. The precision boundary is upstream and this crate cannot move it.
    ///
    /// What *is* true, and what actually matters for money, is that everything past this point is
    /// exact decimal. A bound of `0.3` contains `0.1 + 0.2`; in binary floating point it does not,
    /// because `0.1 + 0.2` is `0.30000000000000004`. So this fails if anyone reaches for `as_f64`
    /// or for the well-typed `serde_json::from_value::<Decimal>(v)`.
    #[test]
    fn a_bound_compares_in_decimal_not_in_binary_floating_point() {
        let raw: Value = serde_json::from_str(r#"{"min": 0.3, "max": 0.3}"#).expect("valid JSON");

        let PolicyValue::Range(bounds) = PolicyValue::from_declared(&PolicyType::Range, &raw)
        else {
            panic!("a well-formed range reads as a range");
        };

        let sum = Decimal::from_str("0.1").unwrap() + Decimal::from_str("0.2").unwrap();
        assert!(
            bounds.contains(sum),
            "a bound of 0.3 must contain 0.1 + 0.2; got min={} max={} sum={sum}",
            bounds.min(),
            bounds.max()
        );

        // The control: the identical comparison in binary floating point comes out the other way,
        // so the assertion above could have failed and is not vacuous. `black_box` keeps this a
        // runtime comparison — as literals the compiler folds it, and clippy correctly points out
        // that a constant assertion demonstrates nothing.
        let (tenth, fifth, three_tenths) = (
            std::hint::black_box(0.1_f64),
            std::hint::black_box(0.2_f64),
            std::hint::black_box(0.3_f64),
        );
        assert!(
            tenth + fifth > three_tenths,
            "f64 has stopped being a counter-example, so the assertion above proves nothing"
        );
    }

    /// A realistic monetary bound survives the conversion exactly.
    ///
    /// The everyday case, and the one `OFFLINE_MAX_AMOUNT` actually carries. Sits beside the test
    /// above so that "the platform's precision is bounded by f64 upstream" is not mistaken for
    /// "money is approximate here".
    #[test]
    fn a_realistic_money_bound_survives_exactly() {
        let raw: Value = serde_json::from_str(r#"{"min": 0, "max": 2500.75}"#).expect("valid JSON");

        let PolicyValue::Range(bounds) = PolicyValue::from_declared(&PolicyType::Range, &raw)
        else {
            panic!("a well-formed range reads as a range");
        };
        assert_eq!(bounds.max(), Decimal::from_str("2500.75").unwrap());
    }

    #[test]
    fn bounds_are_inclusive_at_both_ends() {
        let bounds = RangeBounds::new(Decimal::from(4), Decimal::from(8)).expect("valid bounds");

        assert!(bounds.contains(Decimal::from(4)));
        assert!(bounds.contains(Decimal::from(8)));
        assert!(!bounds.contains(Decimal::from(3)));
        assert!(!bounds.contains(Decimal::from(9)));
    }
}
