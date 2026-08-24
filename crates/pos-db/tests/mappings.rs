//! Every declared row shape names columns the schema actually has.
//!
//! The macro in `projection.rs` gives the compiler the field set: a field missing from a
//! declaration is E0063, an entry that is not a field is E0560, a field twice is E0062. **The
//! column strings are not policed by anything the compiler can see.** This compiles clean, with
//! zero warnings, and silently swaps two of an operator's attributes:
//!
//! ```ignore
//! department from "position",
//! position,
//! ```
//!
//! This file closes the half of that hole a declaration can be checked against on its own: a
//! column the table does not have, and a column already spoken for by another entry. The other
//! half — a column that exists and is the wrong one — is closed by the per-mapping column-identity
//! tests, which write through the store and read every column back by name.
//!
//! # Why `PRAGMA table_info` is read by name here
//!
//! `row.get::<_, String>("name")` below is a **named** read, in a repository whose whole point is
//! that named access was refused. That is not a contradiction and the reason matters, because the
//! next reader will otherwise take it as one.
//!
//! Named access was refused as the mechanism for reading **domain rows**. `row.get("customer_name")`
//! where `customer_id` was meant resolves, type-checks and swaps attribution exactly as silently as
//! `row.get(4)` does — the name buys nothing there, because a wrong name that resolves is the whole
//! defect. `PRAGMA table_info` is SQLite's own metadata: six fixed columns, no domain row behind
//! them, and no attribution to swap. Reading it positionally would import the defect into the
//! guard against it.
//!
//! That is also why this file needs no exemption from the scan guard that comes after it. An
//! exemption avoided is worth more than one with a well-written expiry.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use pos_db::migrations::run_migrations;
use pos_db::projection::{DeclaredShape, DECLARED_SHAPES};
use pos_db::Database;
use rusqlite::Connection;

/// A database migrated to the current schema version.
fn migrated() -> Database {
    let db = Database::in_memory().expect("an in-memory database");
    {
        let conn = db.connection();
        let conn = conn.lock();
        run_migrations(&conn).expect("the migrations");
    }
    db
}

/// The column names `PRAGMA table_info` reports for `table`, read by name.
///
/// Returns whatever SQLite says, including nothing: an unknown table is not an error to SQLite,
/// it is an empty result. That is precisely why every caller must check for emptiness before
/// concluding anything — see `the_schema_check_can_tell_a_real_table_from_a_typo`.
fn schema_columns(conn: &Connection, table: &str) -> BTreeSet<String> {
    let mut statement = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("PRAGMA table_info prepares for any identifier");
    let names = statement
        .query_map([], |row| row.get::<_, String>("name"))
        .expect("the PRAGMA runs");
    names.map(|name| name.expect("a column name")).collect()
}

// ---------------------------------------------------------------------------------------------
// The positive control, first, because everything below is a subset check
// ---------------------------------------------------------------------------------------------

#[test]
fn the_schema_check_can_tell_a_real_table_from_a_typo() {
    // The load-bearing half. Every assertion below this line has the form "these columns are among
    // those the table has", and an empty column set makes that vacuously true. A mapping pointed
    // at `operatorz` would then pass — a broken reader and a clean tree returning literally the
    // same result. This is the reading that comes out differently.
    let db = migrated();
    let conn = db.connection();
    let conn = conn.lock();

    let real = schema_columns(&conn, "operators");
    assert!(
        real.contains("id") && real.contains("permissions_json"),
        "`operators` did not report its own columns: {real:?}"
    );

    let typo = schema_columns(&conn, "operatorz");
    assert!(
        typo.is_empty(),
        "a table that does not exist reported columns: {typo:?}"
    );
}

#[test]
fn every_table_a_mapping_names_exists_and_reports_columns() {
    let db = migrated();
    let conn = db.connection();
    let conn = conn.lock();

    for shape in DECLARED_SHAPES {
        let Some(table) = (shape.table)() else {
            continue;
        };
        let columns = schema_columns(&conn, table);
        assert!(
            !columns.is_empty(),
            "`{}` names the table `{table}`, which the schema does not have",
            shape.name
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The subset checks
// ---------------------------------------------------------------------------------------------

#[test]
fn every_mapping_names_columns_the_schema_has() {
    let db = migrated();
    let conn = db.connection();
    let conn = conn.lock();

    for shape in DECLARED_SHAPES {
        let Some(table) = (shape.table)() else {
            continue;
        };
        let schema = schema_columns(&conn, table);
        assert!(!schema.is_empty(), "`{table}` reported no columns");

        // Projected and inserted both. They are different lists — a managed column is inserted and
        // never projected — and each is a place a name can be wrong.
        for (role, named) in [
            ("projects", (shape.projected)()),
            ("inserts", (shape.inserted)()),
        ] {
            for column in named {
                assert!(
                    schema.contains(column),
                    "`{}` {role} `{column}`, which `{table}` does not have",
                    shape.name
                );
            }
        }
    }
}

#[test]
fn no_mapping_names_the_same_column_twice() {
    // The schema check cannot see an aliasing mistake: `department from "position"` beside
    // `position` names a column that really exists, twice. This is what catches it, and it is the
    // only check in this file that needs no database.
    for shape in DECLARED_SHAPES {
        for (role, named) in [
            ("projects", (shape.projected)()),
            ("inserts", (shape.inserted)()),
        ] {
            let mut seen = HashSet::new();
            for column in &named {
                assert!(
                    seen.insert(*column),
                    "`{}` {role} `{column}` twice: {named:?}",
                    shape.name
                );
            }
        }
    }
}

#[test]
fn the_only_shape_without_a_table_is_the_one_that_has_no_table() {
    // Not a formality. The first draft of this guard exempted the day-totals aggregate as a matter
    // of course, and that aggregate was the one shape whose column names were wrong. The exemption
    // was granted because the case looked different, and looking different was the symptom. So the
    // table-less set is asserted **exactly**, and adding a shape to it is a deliberate edit here.
    let without: Vec<&str> = DECLARED_SHAPES
        .iter()
        .filter(|shape| (shape.table)().is_none())
        .map(|shape| shape.name)
        .collect();
    // Two, both aggregates over `offline_transactions` with no table to write back to, both
    // `RowReader` rather than `RowMapping` so the compiler refuses to write them. Adding a third
    // is a deliberate edit here.
    let expected = vec!["DAY_TOTALS_ROW", "PAYMENT_BREAKDOWN_ROW"];
    assert_eq!(
        without, expected,
        "a shape declares no table; if that is right, name it here"
    );
}

// ---------------------------------------------------------------------------------------------
// The registry is not allowed to certify itself
// ---------------------------------------------------------------------------------------------

/// Every `.rs` file under the `src/` of **every crate in the workspace**, not just this one.
///
/// # Why this reaches outside its own package
///
/// It walked `crates/pos-db/src` alone until a peer named what that is: a guard written from a
/// survey, certifying the survey. `row_mapping!` and `row_reader!` are `#[macro_export]`, so any
/// crate that depends on `pos-db` can declare a shape — and a shape declared in `pos-services`
/// would have been invisible to `DECLARED_SHAPES`, invisible to the schema check, **and its
/// absence would have read as a pass**. The narrow scan was honest while `pos-db` held every
/// shape and would have become a trap the moment one moved, which task 13 does.
///
/// Anchored on `CARGO_MANIFEST_DIR` rather than the working directory, because an integration
/// test's cwd is its package root and this needs the directory above it.
fn sources() -> Vec<PathBuf> {
    fn walk(directory: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(directory) else {
            return; // a crate without a `src/`; `crates/` holds directories that are not packages
        };
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                found.push(path);
            }
        }
    }

    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("this package sits inside `crates/`")
        .to_path_buf();
    let mut found = Vec::new();
    for entry in fs::read_dir(&crates)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", crates.display()))
    {
        let package = entry.expect("a directory entry").path();
        walk(&package.join("src"), &mut found);
    }
    found.sort();
    found
}

/// The name of every `pub const` shape declared by `row_mapping!` or `row_reader!`.
///
/// Comment lines are dropped before matching, so the worked expansion in `row_mapping!`'s own doc
/// comment — which contains the literal text `pub const OPERATOR_ROW: RowMapping<OperatorRow>` —
/// does not count as a declaration. A guard that read its own documentation as evidence would be
/// the same defect one level up.
fn declared_in_source() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for path in sources() {
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
        let mut inside_declaration = false;
        for line in text.lines() {
            // Named `uncommented`, not `code`: `tests/guards.rs` reserves that word for a
            // server error code and refuses `code.contains(...)` anywhere under `crates/`,
            // because a refusal read out of prose once abandoned a queued sale forever. The
            // guard is right to be blunt about it, and this is the clearer name regardless.
            let uncommented = match line.trim_start().find("//") {
                Some(0) => "",
                _ => line,
            };
            if uncommented.contains("row_mapping! {") || uncommented.contains("row_reader! {") {
                inside_declaration = true;
                continue;
            }
            if !inside_declaration {
                continue;
            }
            if let Some(rest) = uncommented.trim_start().strip_prefix("pub const ") {
                if let Some((name, _)) = rest.split_once(':') {
                    names.insert(name.trim().to_string());
                }
                inside_declaration = false;
            } else if uncommented.trim_start().starts_with("const ") {
                // A test-local shape. Not registrable, and not an omission.
                inside_declaration = false;
            }
        }
    }
    names
}

#[test]
fn the_source_scan_finds_declarations_at_all() {
    // The witness. Everything below compares two sets, and two empty sets are equal — a scan that
    // read the wrong directory, or a `row_mapping! {` that was renamed, would agree with an empty
    // registry and report as a pass.
    let files = sources();
    assert!(
        files.len() > 5,
        "the scan read {} file(s); that is not this workspace",
        files.len()
    );
    assert!(
        !declared_in_source().is_empty(),
        "the scan found no `pub const` row shape anywhere"
    );

    // It must reach past its own package, or the widening above is decoration. `pos-services`
    // declares no shape today and is where task 13 puts the first one outside `pos-db`.
    let crates_seen: BTreeSet<String> = files
        .iter()
        .filter_map(|path| {
            let text = path.to_string_lossy();
            let after = text.split("crates/").nth(1)?;
            Some(after.split('/').next()?.to_string())
        })
        .collect();
    for package in ["pos-db", "pos-services", "pos-models"] {
        assert!(
            crates_seen.contains(package),
            "the scan never reached `{package}`; it saw {crates_seen:?}"
        );
    }

    // The third control, and the one that is easiest to skip: the detector must **not** fire on a
    // constructed negative. `projection.rs`'s own test module declares three shapes with a bare
    // `const`, which are not registrable. A detector broken open passes the two assertions above
    // and flags every one of them — and that reads as a finding rather than as a broken
    // instrument.
    let declared = declared_in_source();
    for test_local in ["SAMPLE_BY_MACRO", "RENAMED", "SAMPLE_TOTALS"] {
        assert!(
            !declared.contains(test_local),
            "`{test_local}` is a test-local `const` and must not be collected as registrable; \
             the scan is matching more than `pub const`"
        );
    }
}

#[test]
fn the_registry_lists_every_declared_shape_and_no_others() {
    // A hand-listed registry that nothing checks is worse than no registry: a mapping left out of
    // it is a mapping no guard in this file covers, and its absence reads as a pass everywhere.
    let registered: BTreeSet<String> = DECLARED_SHAPES
        .iter()
        .map(|shape: &DeclaredShape| shape.name.to_string())
        .collect();
    let declared = declared_in_source();

    let missing: Vec<&String> = declared.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "declared in src/ but absent from `DECLARED_SHAPES`, so unguarded: {missing:?}"
    );
    let stale: Vec<&String> = registered.difference(&declared).collect();
    assert!(
        stale.is_empty(),
        "listed in `DECLARED_SHAPES` but not declared in src/: {stale:?}"
    );
}
