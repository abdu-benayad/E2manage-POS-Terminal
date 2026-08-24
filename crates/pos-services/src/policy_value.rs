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
    default: Option<Decimal>,
}

impl RangeBounds {
    /// Builds bounds, refusing a range that contradicts itself.
    ///
    /// Two contradictions are refused, and both are the platform having got the policy wrong
    /// rather than a range that happens to be unusual. The caller records either as
    /// [`PolicyValue::Malformed`]. There is deliberately no constructor that accepts one, so no
    /// later code has to re-check.
    ///
    /// - `min > max` admits no value at all.
    /// - a `default` outside `min..=max` is a chosen setting the same policy forbids.
    pub fn new(min: Decimal, max: Decimal, default: Option<Decimal>) -> Option<Self> {
        let ordered = min <= max;
        let default_permitted = default.is_none_or(|d| min <= d && d <= max);
        (ordered && default_permitted).then_some(Self { min, max, default })
    }

    /// The lower bound.
    pub fn min(&self) -> Decimal {
        self.min
    }

    /// The upper bound.
    pub fn max(&self) -> Decimal {
        self.max
    }

    /// The setting the platform actually chose, falling back to the lowest it permits.
    ///
    /// # Why this is not `min()`
    ///
    /// A RANGE policy here is *the span of permitted configurations plus the one in force* —
    /// `HEARTBEAT_INTERVAL_SECONDS` is `{"min":30,"max":300,"default":60}`. Reading `min` returns
    /// the lowest value an administrator could have chosen, not the value they did choose.
    ///
    /// That was live: the heartbeat timer at `src/platform.rs:143` ran at **30 seconds against a
    /// configured 60**, twice the intended rate. `get_session_timeout_minutes` returned 5 for a
    /// configured 15, and `get_receipt_retention_days` 7 for a configured 90.
    ///
    /// **`get_min_pin_length` was correct, by coincidence** — that row's `min` and `default` are
    /// both 4. Checking one accessor cleared the whole family, which is why the fixtures below are
    /// the measured rows rather than one example.
    ///
    /// The fallback to `min` is what the predecessor did, kept for the case the platform omits
    /// `default`. Measured 2026-08-24: present on all 9 RANGE rows — but the table gained two rows
    /// during the measurement, is written through a path that casts `policyValue as any` into an
    /// unconstrained `Json` column, and has no write validation at all. Uniformity across nine
    /// rows at one instant is not a promise about the tenth.
    pub fn configured(&self) -> Decimal {
        self.default.unwrap_or(self.min)
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

/// Reads `{ "min": …, "max": …, "default": … }` into bounds, or `None` if it is not that.
///
/// `default` is optional and its absence is not a failure; a `default` present but unreadable is,
/// because a policy naming a setting the till cannot read is not a policy the till should apply
/// half of.
fn parse_bounds(value: &Value) -> Option<RangeBounds> {
    let object = value.as_object()?;
    let min = decimal_from(object.get("min")?)?;
    let max = decimal_from(object.get("max")?)?;
    let default = match object.get("default") {
        None => None,
        Some(declared) => Some(decimal_from(declared)?),
    };
    RangeBounds::new(min, max, default)
}

/// Reads `{ "allowed": [...] }` into the permitted values.
///
/// # Why the wrapper object, and why a bare array is refused
///
/// Because that is what the platform sends. Measured against its live table 2026-08-24, the only
/// LIST policy in existence is
/// `ALLOWED_PAYMENT_METHODS = {"allowed":["CASH","CARD","MOBILE","CREDIT_ACCOUNT"]}`, and its own
/// evaluator reads `(policy.policyValue as { allowed: string[] }).allowed || []`.
///
/// This type was originally written to expect a bare array — from the name `LIST`, not from the
/// data — so the till's only LIST policy read as [`PolicyValue::Malformed`]. **The predecessor was
/// no better and was worse:** `as_array()` on that object returned `None` and `unwrap_or_default()`
/// made it `vec![]`, which `check_list` reads as *allow all*. `ALLOWED_PAYMENT_METHODS` has never
/// been enforced by this till.
///
/// A bare array is refused rather than tolerated, and the platform's own behaviour is the reason:
/// `.allowed` on `["CASH","CARD"]` is `undefined`, so its evaluator reads that as `[]` too.
/// Accepting it here would have the till enforce a rule the platform does not — worse than either
/// side failing alone. The platform's e2e tests do create bare arrays, and one deliberately stores
/// the string `"not-an-array"`; there is no write validation on that column, so both shapes can
/// arrive and both are unreadable.
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
        .as_object()?
        .get("allowed")?
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
            read(PolicyType::List, json!({"allowed": ["CASH", "CARD"]})),
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
            read(PolicyType::List, json!({"allowed": []})),
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
        assert_eq!(
            read(PolicyType::List, json!({"allowed": []})),
            PolicyValue::List(vec![])
        );
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
        let value = read(PolicyType::List, json!({"allowed": ["CASH", 42, "CARD"]}));

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
        let value = read(PolicyType::List, json!({"allowed": [1, 2, 3]}));

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
        assert!(RangeBounds::new(Decimal::from(9), Decimal::from(4), None).is_none());

        // Control: the degenerate single-value range is legal, so the constructor is refusing
        // inversion rather than refusing everything.
        assert!(RangeBounds::new(Decimal::from(4), Decimal::from(4), None).is_some());
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

    /// Every policy the platform's live table actually holds, read by this type.
    ///
    /// **These fixtures are the measured rows, verbatim, not compositions of what the shapes ought
    /// to be.** This type was originally written from the declared type names — `LIST` reads like
    /// a bare array — and every one of its tests passed while the only LIST policy in existence
    /// was unreadable. A test can be right about an invented input indefinitely.
    ///
    /// Measured 2026-08-24 against the platform's dev database. Caveats worth carrying: it is
    /// **dev**, not production; `POS_SecurityPolicy` has no `companyId`, so there is one global row
    /// set; and nothing in the platform repo creates these rows — no seed file, and
    /// `MIN_PIN_LENGTH` appears once in that entire checkout. A repo-only search reports this
    /// vocabulary does not exist, which is wrong in the dangerous direction.
    #[test]
    fn every_policy_the_platform_actually_holds_is_understood() {
        let rows = [
            (PolicyType::Range, r#"{"max":8,"min":4,"default":4}"#),
            (PolicyType::Range, r#"{"max":60,"min":5,"default":15}"#),
            (PolicyType::Range, r#"{"max":10,"min":3,"default":5}"#),
            (PolicyType::Range, r#"{"max":10000,"min":0,"default":1000}"#),
            (PolicyType::Range, r#"{"max":1000,"min":10,"default":100}"#),
            (PolicyType::Range, r#"{"max":300,"min":30,"default":60}"#),
            (PolicyType::Range, r#"{"max":30,"min":1,"default":5}"#),
            (PolicyType::Range, r#"{"max":365,"min":7,"default":90}"#),
            (PolicyType::Range, r#"{"max":90,"min":7,"default":30}"#),
            (
                PolicyType::List,
                r#"{"allowed":["CASH","CARD","MOBILE","CREDIT_ACCOUNT"]}"#,
            ),
            (PolicyType::Boolean, "true"),
            (PolicyType::Boolean, "true"),
        ];

        for (declared, raw) in rows {
            let value: Value = serde_json::from_str(raw).expect("the platform's own JSON");
            let read = PolicyValue::from_declared(&declared, &value);
            assert!(
                read.is_understood(),
                "the platform sends {raw} for a {declared:?} policy and the till cannot read it: {read:?}"
            );
        }
    }

    /// The allow-list the platform actually sends yields exactly its four methods.
    #[test]
    fn the_payment_methods_policy_reads_as_its_four_methods() {
        let raw: Value =
            serde_json::from_str(r#"{"allowed":["CASH","CARD","MOBILE","CREDIT_ACCOUNT"]}"#)
                .expect("the platform's own JSON");

        assert_eq!(
            PolicyValue::from_declared(&PolicyType::List, &raw),
            PolicyValue::List(vec![
                "CASH".to_string(),
                "CARD".to_string(),
                "MOBILE".to_string(),
                "CREDIT_ACCOUNT".to_string(),
            ])
        );
    }

    /// Accepting the wrapper object must not mean accepting anything.
    ///
    /// The negative controls, and not hypothetical ones: the platform's own e2e tests create LIST
    /// policies as bare arrays, and one deliberately stores the string `"not-an-array"`. There is
    /// no write validation on that column. A bare array is refused because `.allowed` on it is
    /// `undefined` in the platform's evaluator too — being lenient here would have the till enforce
    /// a rule the platform does not.
    #[test]
    fn the_shapes_the_platform_can_also_emit_are_still_unreadable() {
        for raw in [r#"["CASH","CARD","QR"]"#, r#""not-an-array""#, "42"] {
            let value: Value = serde_json::from_str(raw).expect("valid JSON");
            let read = PolicyValue::from_declared(&PolicyType::List, &value);
            assert!(
                matches!(read, PolicyValue::Malformed { .. }),
                "{raw} is not a readable allow-list, got {read:?}"
            );
        }
    }

    /// A range names the setting in force, not the lowest one permitted.
    ///
    /// The second case is the control and it is the one that matters: `MIN_PIN_LENGTH` has `min`
    /// and `default` both 4, so an implementation ignoring `default` entirely still passes it.
    /// Checking that accessor alone cleared the whole family, which is how the heartbeat came to
    /// run at half its configured interval.
    #[test]
    fn a_range_carries_the_setting_in_force_and_not_only_its_span() {
        let heartbeat: Value =
            serde_json::from_str(r#"{"max":300,"min":30,"default":60}"#).expect("valid JSON");
        let PolicyValue::Range(bounds) = PolicyValue::from_declared(&PolicyType::Range, &heartbeat)
        else {
            panic!("the platform's own heartbeat row reads as a range");
        };
        assert_eq!(
            bounds.configured(),
            Decimal::from(60),
            "configured, not min"
        );
        assert_eq!(
            bounds.min(),
            Decimal::from(30),
            "the span is still available"
        );

        let pin: Value =
            serde_json::from_str(r#"{"max":8,"min":4,"default":4}"#).expect("valid JSON");
        let PolicyValue::Range(bounds) = PolicyValue::from_declared(&PolicyType::Range, &pin)
        else {
            panic!("the platform's own PIN row reads as a range");
        };
        assert_eq!(bounds.configured(), Decimal::from(4));
    }

    /// A range with no `default` falls back to its minimum, as the predecessor did.
    ///
    /// Measured 2026-08-24: `default` is present on all 9 RANGE rows. Modelled as absent-able
    /// anyway — the column has no write validation, is written through a path casting
    /// `policyValue as any`, and the table gained two rows during the measurement itself.
    /// Uniformity across nine rows at one instant is not a promise about the tenth.
    #[test]
    fn a_range_without_a_chosen_setting_falls_back_to_its_minimum() {
        let raw: Value = serde_json::from_str(r#"{"min":30,"max":300}"#).expect("valid JSON");
        let PolicyValue::Range(bounds) = PolicyValue::from_declared(&PolicyType::Range, &raw)
        else {
            panic!("a range without a default is still a range");
        };
        assert_eq!(bounds.configured(), Decimal::from(30));
    }

    /// A setting the same policy forbids is a contradiction, not a range.
    #[test]
    fn a_default_outside_its_own_bounds_is_malformed() {
        let raw: Value =
            serde_json::from_str(r#"{"min":30,"max":300,"default":9000}"#).expect("valid JSON");

        assert!(matches!(
            PolicyValue::from_declared(&PolicyType::Range, &raw),
            PolicyValue::Malformed { .. }
        ));

        // Control: the same row with its default inside the bounds is fine, so the constructor is
        // refusing the contradiction rather than refusing every default.
        let ok: Value =
            serde_json::from_str(r#"{"min":30,"max":300,"default":90}"#).expect("valid JSON");
        assert!(PolicyValue::from_declared(&PolicyType::Range, &ok).is_understood());
    }

    /// Both not-understood arms are reachable, and well-formed input reaches neither.
    ///
    /// # Why this exists beside the tests that already cover each arm
    ///
    /// Those assert what a *particular* input does. This asserts a property of the whole
    /// conversion, and it is the one that rots silently: if a later change made `from_declared`
    /// stop producing `UnknownType` — folding it into `Malformed`, say, or accepting anything —
    /// every individual arm test could be updated to match and this would still be the thing
    /// nobody noticed had gone.
    ///
    /// **An exemption and a blind check produce identical output.** A tolerated class that is
    /// never populated looks exactly like a tolerated class that is working, so the tolerance has
    /// to be asserted non-empty or it is not evidence of anything.
    ///
    /// Three controls, and the third is the one usually skipped:
    ///
    /// 1. the sweep must find things at all — `understood > 0`;
    /// 2. each detector must **fire** — `unknown > 0` and `malformed > 0`;
    /// 3. each detector must **not** fire on a constructed negative — no well-formed row lands in
    ///    a not-understood arm. Without it, a conversion broken open puts everything in
    ///    `Malformed`, satisfies (2), and reads as a finding rather than as a fault.
    #[test]
    fn each_not_understood_arm_is_reachable_and_well_formed_input_reaches_neither() {
        let well_formed = [
            (PolicyType::Boolean, "true"),
            (PolicyType::Enum, r#""STRICT""#),
            (PolicyType::Regex, r#""^[0-9]{4}$""#),
            (PolicyType::Range, r#"{"min":30,"max":300,"default":60}"#),
            (PolicyType::List, r#"{"allowed":["CASH"]}"#),
        ];
        let contradictory = [
            (PolicyType::Range, r#""not a range""#),
            (PolicyType::List, r#"["CASH"]"#),
            (PolicyType::Boolean, r#""yes""#),
        ];
        let unfamiliar = [(PolicyType::Unknown, r#"{"whatever":1}"#)];

        let read = |(declared, raw): &(PolicyType, &str)| {
            let value: Value = serde_json::from_str(raw).expect("valid JSON");
            PolicyValue::from_declared(declared, &value)
        };

        // (1) and (3): the sweep finds things, and none of them is a not-understood arm.
        let understood = well_formed.iter().map(read).collect::<Vec<_>>();
        assert!(!understood.is_empty());
        for (value, (declared, raw)) in understood.iter().zip(well_formed.iter()) {
            assert!(
                value.is_understood(),
                "{raw} is a well-formed {declared:?} policy and must not land in a \
                 not-understood arm: {value:?}"
            );
        }

        // (2): each detector fires, counted rather than spot-checked.
        let malformed = contradictory
            .iter()
            .map(read)
            .filter(|v| matches!(v, PolicyValue::Malformed { .. }))
            .count();
        let unknown = unfamiliar
            .iter()
            .map(read)
            .filter(|v| matches!(v, PolicyValue::UnknownType { .. }))
            .count();

        assert_eq!(
            malformed,
            contradictory.len(),
            "every contradictory value must be Malformed; a tolerated arm nothing reaches is \
             indistinguishable from one that never fires"
        );
        assert_eq!(
            unknown,
            unfamiliar.len(),
            "an unrecognised declared type must reach UnknownType, not Malformed — they are \
             different facts about the platform"
        );
    }

    #[test]
    fn bounds_are_inclusive_at_both_ends() {
        let bounds =
            RangeBounds::new(Decimal::from(4), Decimal::from(8), None).expect("valid bounds");

        assert!(bounds.contains(Decimal::from(4)));
        assert!(bounds.contains(Decimal::from(8)));
        assert!(!bounds.contains(Decimal::from(3)));
        assert!(!bounds.contains(Decimal::from(9)));
    }
}
