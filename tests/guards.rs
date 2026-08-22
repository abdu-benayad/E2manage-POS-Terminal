//! Guards for the concepts `type-driven-domain-core` finished.
//!
//! The issue's diagnosis of how this codebase drifted is that **a rule written in a document is a
//! rule nobody runs**. Two `OperatorPermissions` structs disagreed about the casing of the same
//! JSON for as long as they both existed, because nothing checked. Every guard here is a rule that
//! used to live only in a doc, expressed so that violating it turns the build red.
//!
//! # What these guards are, and are not
//!
//! They are pattern scans over the source tree. That makes them cheap, total (they see every line,
//! not a sample), and structurally blind — they cannot tell a declaration from a discussion of one.
//! So **every scan strips `//` comments before matching**: twice during this issue a guard failed
//! against finished work because a doc comment quoted the very pattern it banned, and the fix
//! belongs in the guard, not in the comments. Documenting *why* a shape is forbidden must never
//! break the build.
//!
//! The blindness has a residue worth naming: stripping at the first `//` also truncates a line at a
//! `//` inside a string literal. That can only hide a violation, never invent one, and no line in
//! the scanned tree has that shape today.
//!
//! # Scope
//!
//! `crates/` and `src/` — the shipped tree. **There are no exemptions**, and that is deliberate:
//! the earlier plan for this file carried two (`src/main.rs`, `src/ui/conflict_bridge.rs`) and both
//! premises evaporated — the Slint tree was deleted and `src/ui/` was migrated like any other
//! module. If a later task ever does need a temporary allowance, spell it as an explicit path list
//! that **fails when a listed path stops existing**, never as a pattern that quietly keeps matching
//! nothing. An exemption with no expiry is how a temporary allowance becomes permanent, which is
//! the shape of every defect this issue unwound.
//!
//! One guard does not scan Rust at all. `the_config_cargo_reads_by_default_needs_nothing_a_clone_lacks`
//! reads `.cargo/` and `.gitignore`, because the defect it holds shut is not in the source: it is a
//! build that cannot start. It lives here because it is the same kind of rule — one that used to be
//! true only in somebody's head — and this is the file that runs those.
//!
//! `tests/` is out of scope. Its fixtures were migrated alongside the source, but a test may
//! legitimately model a foreign shape — `tests/api_tests.rs` deserializes the ERP's `UserInfo`,
//! whose `role` is a free-text string belonging to a different domain than the operator's.
//!
//! # Two scans, because a concept has two vocabularies
//!
//! A financial record names somebody else — `operator_id`, `operator_name` — and the operator's
//! own record names itself: `id`, `name`. A scan keyed on the first spelling is blind to the
//! second, which is exactly how `OperatorRow.id` and `OperatorDto.id` stayed `String` after the
//! concept was declared finished. So identity is guarded twice: once by field name across the
//! whole tree, and once across every type whose own name says it is about an operator.

mod guards {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    use e2manage_pos_terminal::models::{Pin, PinLength};

    /// The shipped tree. See the module docs on why there is nothing else and no exemption list.
    const SCANNED_ROOTS: [&str; 2] = ["crates", "src"];

    /// Said the same way wherever the `OperatorPermissions` default is refused, because the two
    /// spellings of it are one defect.
    const DEFAULT_IS_THE_DEFECT: &str = "a default permission set is what let an unreadable mapping look like a legitimately unprivileged operator; construct `OperatorPermissions::none()` at a site that means it";

    // ========================================================================
    // Reading the tree
    // ========================================================================

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn relative(path: &Path) -> String {
        path.strip_prefix(repo_root())
            .unwrap_or(path)
            .display()
            .to_string()
    }

    fn collect_rust_sources(dir: &Path, found: &mut Vec<PathBuf>) {
        let entries = fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {} while scanning: {e}", dir.display()));
        for entry in entries {
            let path = entry
                .expect("cannot read a directory entry while scanning")
                .path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if path.is_dir() {
                collect_rust_sources(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }

    /// Every `.rs` file under [`SCANNED_ROOTS`], sorted so failures are reported deterministically.
    fn rust_sources() -> Vec<PathBuf> {
        let root = repo_root();
        let mut found = Vec::new();
        for scanned in SCANNED_ROOTS {
            collect_rust_sources(&root.join(scanned), &mut found);
        }
        found.sort();
        found
    }

    /// One source line with any `//` comment removed. See the module docs.
    struct SourceLine {
        path: String,
        number: usize,
        code: String,
    }

    fn strip_comment(line: &str) -> &str {
        match line.find("//") {
            Some(at) => &line[..at],
            None => line,
        }
    }

    fn scanned_lines() -> Vec<SourceLine> {
        rust_sources()
            .iter()
            .flat_map(|path| {
                let text = fs::read_to_string(path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                let relative = relative(path);
                text.lines()
                    .enumerate()
                    .map(|(index, line)| SourceLine {
                        path: relative.clone(),
                        number: index + 1,
                        code: strip_comment(line).to_string(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    // ========================================================================
    // Recognising a declaration
    // ========================================================================

    fn is_word_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    /// `text` begins with `word` and does not continue it — so `String` matches `String` and
    /// `String,` but not `StringBuilder`.
    fn starts_with_word(text: &str, word: &str) -> bool {
        text.strip_prefix(word)
            .is_some_and(|rest| !rest.as_bytes().first().is_some_and(|b| is_word_byte(*b)))
    }

    fn contains_word(text: &str, word: &str) -> bool {
        let mut from = 0;
        while let Some(offset) = text[from..].find(word) {
            let at = from + offset;
            let opens = at == 0 || !is_word_byte(text.as_bytes()[at - 1]);
            if opens && starts_with_word(&text[at..], word) {
                return true;
            }
            from = at + word.len();
        }
        false
    }

    /// The type expression declared for `name` on this line, if any.
    ///
    /// Word-bounded on both sides, so `pin_hash:` is not a `pin` declaration and `operator_ids:` is
    /// not an `operator_id` one. A following `::` is a path expression, not a declaration.
    fn declared_type<'a>(code: &'a str, name: &str) -> Option<&'a str> {
        let mut from = 0;
        while let Some(offset) = code[from..].find(name) {
            let at = from + offset;
            let opens = at == 0 || !is_word_byte(code.as_bytes()[at - 1]);
            let after = &code[at + name.len()..];
            let closes = !after.as_bytes().first().is_some_and(|b| is_word_byte(*b));
            if opens && closes {
                if let Some(rest) = after.trim_start().strip_prefix(':') {
                    if !rest.starts_with(':') {
                        return Some(rest.trim());
                    }
                }
            }
            from = at + name.len();
        }
        None
    }

    /// The primitive spelling of a type, if this declaration uses one.
    ///
    /// A leading `String::` is `String::new()` — an expression in a struct literal, not a type. The
    /// literal's own field declaration is what this guard is for, and that is caught where it is
    /// written.
    fn primitive_string_spelling(declared: &str) -> Option<&'static str> {
        if declared.starts_with("String::") {
            return None;
        }
        if starts_with_word(declared, "String") {
            return Some("String");
        }
        if declared.starts_with("Option<String>") {
            return Some("Option<String>");
        }
        if declared.starts_with("Option<&str>") {
            return Some("Option<&str>");
        }
        let borrowed = declared.strip_prefix('&')?;
        let after_lifetime = match borrowed.strip_prefix('\'') {
            Some(rest) => rest
                .trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_')
                .trim_start(),
            None => borrowed,
        };
        starts_with_word(after_lifetime, "str").then_some("&str")
    }

    // ========================================================================
    // Recognising a type declaration
    // ========================================================================

    /// A `struct` or `enum` declaration: where it is, the attributes written immediately above it,
    /// and the lines of its body.
    ///
    /// Two guards need this rather than a line scan, and for the same reason: the property they
    /// check spans lines. A derive sits above the header, a field sits below it, and neither is
    /// visible from the other's line.
    struct TypeDeclaration {
        path: String,
        number: usize,
        name: String,
        attributes: String,
        body: Vec<SourceLine>,
    }

    /// The name a declaration header introduces.
    ///
    /// Anchored at the start of the trimmed line after an optional visibility, so `impl Debug for
    /// Foo` and a `where` clause naming a type are not declarations. Attributes have already been
    /// consumed by the caller, and comments by [`strip_comment`].
    fn declared_type_name(code: &str) -> Option<String> {
        let after_visibility = match code.strip_prefix("pub") {
            Some(rest) => match rest.strip_prefix('(') {
                Some(scoped) => scoped.split_once(')')?.1,
                None => rest,
            },
            None => code,
        }
        .trim_start();

        let after_keyword = ["struct", "enum"].into_iter().find_map(|keyword| {
            starts_with_word(after_visibility, keyword).then(|| &after_visibility[keyword.len()..])
        })?;

        let name: String = after_keyword
            .trim_start()
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        (!name.is_empty()).then_some(name)
    }

    fn brace_delta(code: &str) -> i32 {
        let opens = i32::try_from(code.matches('{').count()).unwrap_or(i32::MAX);
        let closes = i32::try_from(code.matches('}').count()).unwrap_or(i32::MAX);
        opens - closes
    }

    fn type_declarations() -> Vec<TypeDeclaration> {
        let mut declarations = Vec::new();
        for path in rust_sources() {
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let where_ = relative(&path);
            let lines: Vec<&str> = text.lines().collect();
            let mut attributes = String::new();
            let mut index = 0;

            while index < lines.len() {
                let code = strip_comment(lines[index]).trim().to_string();
                if code.is_empty() {
                    index += 1;
                    continue;
                }
                if code.starts_with('#') {
                    attributes.push_str(&code);
                    index += 1;
                    continue;
                }

                let Some(name) = declared_type_name(&code) else {
                    attributes.clear();
                    index += 1;
                    continue;
                };

                let header = index + 1;
                let mut depth = brace_delta(&code);
                let mut body = Vec::new();
                index += 1;
                while index < lines.len() && depth > 0 {
                    let line = strip_comment(lines[index]);
                    body.push(SourceLine {
                        path: where_.clone(),
                        number: index + 1,
                        code: line.to_string(),
                    });
                    depth += brace_delta(line);
                    index += 1;
                }

                declarations.push(TypeDeclaration {
                    path: where_.clone(),
                    number: header,
                    name,
                    attributes: std::mem::take(&mut attributes),
                    body,
                });
            }
        }
        declarations
    }

    /// Whether the attributes above a declaration derive `trait_name`.
    ///
    /// A derive expands to an `impl`, so a guard that greps only for the written-out `impl` misses
    /// the shorter and likelier spelling of the same thing.
    fn derives(attributes: &str, trait_name: &str) -> bool {
        let mut from = 0;
        while let Some(offset) = attributes[from..].find("derive(") {
            let opens = from + offset + "derive(".len();
            let closes = attributes[opens..]
                .find(')')
                .map_or(attributes.len(), |end| opens + end);
            if contains_word(&attributes[opens..closes], trait_name) {
                return true;
            }
            from = closes;
        }
        false
    }

    // ========================================================================
    // The guards
    // ========================================================================

    /// A guard that reads nothing passes everything.
    ///
    /// This is the one failure the other four cannot detect about themselves: a walker that returns
    /// an empty list reports a clean tree in exactly the same words as a clean tree does. Every
    /// scan below therefore runs over a corpus this test has already proved is real.
    #[test]
    fn the_guards_actually_read_the_shipped_tree() {
        for scanned in SCANNED_ROOTS {
            let root = repo_root().join(scanned);
            assert!(root.is_dir(), "{} is not a directory — the scan roots have moved and every guard below is now vacuous", root.display());
        }

        let sources = rust_sources();
        assert!(
            sources.len() > 50,
            "scanned only {} Rust files under {SCANNED_ROOTS:?}; the walker is broken and the guards are passing on an empty corpus",
            sources.len()
        );

        let landmark = repo_root().join("crates/pos-models/src/operator.rs");
        assert!(
            sources.contains(&landmark),
            "the operator concept's own module is not in the scanned set — the walker is missing part of the tree"
        );

        let lines = scanned_lines();
        assert!(
            lines.len() > sources.len(),
            "read {} lines from {} files; the reader is not returning file contents",
            lines.len(),
            sources.len()
        );

        // The declaration walker needs its own positive control. It is strictly more fragile than
        // the line scan — it tracks an attribute block, a header and a brace depth — and every way
        // it can break returns fewer declarations, which reads as a cleaner tree.
        let declarations = type_declarations();
        assert!(
            declarations.len() > 100,
            "the declaration walker found only {} types; it is broken, and the two guards built on it are passing on nothing",
            declarations.len()
        );
        assert!(
            declarations
                .iter()
                .any(|d| d.name == "VerifyPinRequest" && d.path == "crates/pos-api/src/auth.rs"),
            "the walker did not find `VerifyPinRequest` — it is not reaching a struct that sits behind a doc comment and two attributes"
        );

        // The exact shape `a_live_pin_never_reaches_a_derived_debug` inspects: a `Debug`-deriving
        // type with a `pin` field. Without one in the tree that guard has nothing to be right
        // about. The witness is the `VerifyPin` mock in `pos-models`' own tests, which also proves
        // the walker descends into `mod tests`.
        let witnesses: Vec<&str> = declarations
            .iter()
            .filter(|d| derives(&d.attributes, "Debug"))
            .filter(|d| {
                d.body
                    .iter()
                    .any(|line| declared_type(&line.code, "pin").is_some())
            })
            .map(|d| d.name.as_str())
            .collect();
        assert!(
            witnesses.contains(&"VerifyPin"),
            "no `Debug`-deriving type in the tree declares a `pin` field (found {witnesses:?}); the PIN guard is vacuous"
        );
    }

    /// An operator's identity is never a bare string.
    ///
    /// `OperatorId`, `OperatorName`/`RecordedOperatorName` and `OperatorRole` exist so that the
    /// compiler distinguishes an operator's id from their name from an arbitrary string. A single
    /// `String` field re-opens all three: it accepts a name where an id belongs, it accepts
    /// `"cashier"` where the server sends `"CASHIER"`, and it accepts `""`.
    #[test]
    fn operator_identity_never_survives_as_a_bare_string() {
        const IDENTITY_FIELDS: [&str; 4] =
            ["operator_id", "operator_name", "operator_role", "role"];

        let offences: Vec<String> = scanned_lines()
            .iter()
            .filter_map(|line| {
                IDENTITY_FIELDS.iter().find_map(|field| {
                    let declared = declared_type(&line.code, field)?;
                    let spelling = primitive_string_spelling(declared)?;
                    Some(format!(
                        "{}:{} — `{field}: {spelling}`",
                        line.path, line.number
                    ))
                })
            })
            .collect();

        assert!(
            offences.is_empty(),
            "an operator's identity is spelled as a bare string in {} place(s); use `OperatorId`, `RecordedOperatorName` or `OperatorRole` from `pos-models`:\n  {}",
            offences.len(),
            offences.join("\n  ")
        );
    }

    /// An operator's own record does not get to spell identity differently.
    ///
    /// The scan above keys on the field *names* `operator_id` and `operator_name`, which is how a
    /// financial record refers to somebody else. The operator's own row and the wire's own DTO
    /// call the same two concepts `id` and `name`, so neither that scan nor the grep that closed
    /// task 09 could see them, and they stayed `String` after the concept was declared finished.
    /// A boundary defined by a spelling is not a boundary.
    ///
    /// `code`, `pin_hash`, `employee_id`, `employee_number`, `department` and `position` are
    /// deliberately not here. They are all bare strings and all mutually swappable, and they are
    /// not identity — they belong to a later tier, and naming them as out of scope is the point.
    #[test]
    fn an_operator_type_never_spells_its_own_identity_as_a_string() {
        const IDENTITY_FIELDS: [&str; 4] = ["id", "name", "name_ar", "role"];

        // One allowance, two instances of it, load-bearing rather than historical: a type that
        // *is* the wire's shape carries the wire's two name fields. The server sends `name` and
        // `nameAr` side by side and `OperatorName` has no serde, precisely so nothing gives the
        // wire a nested shape it does not have. Both types convert the pair into one
        // `OperatorName` at their boundary, and the loop below retires either entry automatically
        // if it stops needing the allowance.
        const WIRE_SHAPED: [&str; 2] = [
            // `pos-api`'s sync DTO; converts in `to_operator_row`.
            "OperatorDto",
            // `pos-models`' captured-payload fixture; converts in the test that reads it.
            "SyncedOperator",
        ];
        const WIRE_SHAPED_FIELDS: [&str; 2] = ["name", "name_ar"];

        let declarations = type_declarations();
        let operator_types: Vec<&TypeDeclaration> = declarations
            .iter()
            .filter(|declaration| declaration.name.contains("Operator"))
            .collect();

        let offences: Vec<String> = operator_types
            .iter()
            .flat_map(|declaration| {
                declaration.body.iter().flat_map(move |line| {
                    IDENTITY_FIELDS.iter().filter_map(move |field| {
                        let allowed = WIRE_SHAPED.contains(&declaration.name.as_str())
                            && WIRE_SHAPED_FIELDS.contains(field);
                        if allowed {
                            return None;
                        }
                        let spelling =
                            primitive_string_spelling(declared_type(&line.code, field)?)?;
                        Some(format!(
                            "{}:{} — `{}.{field}: {spelling}`",
                            line.path, line.number, declaration.name
                        ))
                    })
                })
            })
            .collect();

        assert!(
            offences.is_empty(),
            "an operator type spells its own identity as a bare string in {} place(s):\n  {}",
            offences.len(),
            offences.join("\n  ")
        );

        // Each entry, checked on both halves, so no allowance outlives its reason. An exemption
        // that keeps matching nothing is how a temporary one becomes permanent — the shape of
        // every defect this issue unwound.
        for allowed in WIRE_SHAPED {
            let wire = operator_types
                .iter()
                .find(|declaration| declaration.name == allowed)
                .unwrap_or_else(|| {
                    panic!("`{allowed}` no longer exists; delete its allowance above")
                });
            assert!(
                wire.body.iter().any(|line| declared_type(&line.code, "name")
                    .and_then(primitive_string_spelling)
                    .is_some()),
                "`{allowed}` no longer carries the wire's `name` as a string; delete its allowance above"
            );
        }
    }

    /// One concept, one mapping.
    ///
    /// There were two `OperatorPermissions` structs: `pos-api` wrote the wire's `camelCase` into
    /// `permissions_json` and `pos-db` read the same JSON back as `snake_case`, joined by an
    /// `.ok().unwrap_or_default()` that turned a manager holding every privilege into an operator
    /// holding none. Neither half was wrong on its own; the defect was that there were two of them
    /// and a `Default` to absorb the disagreement. This guard catches the third copy, not the
    /// second — by the time a second exists the drift has already happened.
    #[test]
    fn operator_permissions_has_exactly_one_definition_and_no_default() {
        let declarations = type_declarations();
        let definitions: Vec<&TypeDeclaration> = declarations
            .iter()
            .filter(|declaration| declaration.name == "OperatorPermissions")
            .collect();

        let sites: Vec<String> = definitions
            .iter()
            .map(|d| format!("{}:{}", d.path, d.number))
            .collect();
        assert_eq!(
            definitions.len(),
            1,
            "expected exactly one `OperatorPermissions`, found {}:\n  {}",
            definitions.len(),
            sites.join("\n  ")
        );

        let only = definitions[0];
        assert!(
            only.path.starts_with("crates/pos-models/"),
            "`OperatorPermissions` is defined at {}:{} — it belongs to `pos-models`, the one crate both the wire and the store depend on",
            only.path,
            only.number
        );

        // Two spellings of one impl. The orphan rule confines both to `pos-models` — which is also
        // where the convenience is most tempting — and `#[derive(Default)]` is the likelier of the
        // two precisely because it looks like nothing.
        //
        // The derive arm is belt-and-braces today: `DiscountAuthority` has no `Default`, so the
        // derive does not compile, and the type model is refusing the defect before this guard
        // gets to. Keep the arm — the day someone gives `DiscountAuthority` a `Default` for an
        // unrelated reason, this is the only thing still saying no.
        assert!(
            !derives(&only.attributes, "Default"),
            "`OperatorPermissions` derives `Default` at {}:{} — {DEFAULT_IS_THE_DEFECT}",
            only.path,
            only.number
        );

        let written_out: Vec<String> = scanned_lines()
            .iter()
            .filter(|line| line.code.contains("impl Default for OperatorPermissions"))
            .map(|line| format!("{}:{}", line.path, line.number))
            .collect();
        assert!(
            written_out.is_empty(),
            "`impl Default for OperatorPermissions` at {} — {DEFAULT_IS_THE_DEFECT}",
            written_out.join(", ")
        );
    }

    /// A live PIN cannot reach a log.
    #[test]
    fn a_live_pin_never_reaches_a_derived_debug() {
        // The behaviour the structural scan below depends on. If this ever changed, a `pin: Pin`
        // field would start leaking and the scan would still call the tree clean.
        let pin =
            Pin::parse("1234", PinLength::Four).expect("four digits are a valid four-digit PIN");
        assert_eq!(format!("{pin:?}"), "Pin(****)");
        assert_eq!(format!("{pin:#?}"), "Pin(****)");

        // And the structural half: a type that derives `Debug` may hold a PIN only as a `Pin`,
        // whose `Debug` is the redaction asserted above. `pin: String` behind a derived `Debug`
        // puts the digits one `tracing` call away from the till's disk.
        let offences: Vec<String> = type_declarations()
            .iter()
            .filter(|declaration| derives(&declaration.attributes, "Debug"))
            .flat_map(|declaration| {
                declaration.body.iter().filter_map(|line| {
                    let declared = declared_type(&line.code, "pin")?;
                    (!contains_word(declared, "Pin")).then(|| {
                        format!(
                            "{}:{} — `{}` derives `Debug` over `pin: {}`",
                            line.path,
                            line.number,
                            declaration.name,
                            declared.trim_end_matches(',')
                        )
                    })
                })
            })
            .collect();

        assert!(
            offences.is_empty(),
            "a PIN is exposed to a derived `Debug` in {} place(s); hold it as `pos_models::Pin`, or drop the derive until you can:\n  {}",
            offences.len(),
            offences.join("\n  ")
        );
    }

    /// `pos-models` knows nothing about storage or the network.
    ///
    /// It is the apex of the dependency diamond — `pos-db`, `pos-api`, `pos-printing` and
    /// `pos-services` all reach it, and nothing reaches back. That is what makes it the one place a
    /// concept can live without either the wire or the store owning it. A single `rusqlite` or
    /// `reqwest` dependency here inverts that, and the inversion is invisible in a diff of one line.
    #[test]
    fn pos_models_knows_nothing_about_storage_or_the_network() {
        const PERMITTED: [&str; 7] = [
            "chrono",
            "rust_decimal",
            "serde",
            "thiserror",
            "tracing",
            "uuid",
            "zeroize",
        ];

        let path = repo_root().join("crates/pos-models/Cargo.toml");
        let manifest = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));

        let declared = dependency_keys(&manifest, "dependencies");
        let permitted: BTreeSet<String> = PERMITTED.iter().map(|d| (*d).to_string()).collect();

        let added: Vec<&String> = declared.difference(&permitted).collect();
        assert!(
            added.is_empty(),
            "`pos-models` gained {added:?}. It sits above every other crate, so a dependency here is a dependency for the whole workspace, and one that knows about a transport or a store puts the domain underneath its own consumers. If the addition is right, widen this list in the same commit and say why."
        );

        let removed: Vec<&String> = permitted.difference(&declared).collect();
        assert!(
            removed.is_empty(),
            "this guard still permits {removed:?}, which `pos-models` no longer uses — an allowlist nobody prunes stops being an assertion"
        );

        // Held separately from the allowlist: the allowlist says what may be added, this says what
        // may never be, including in the dev-dependencies the allowlist does not cover.
        let forbidden = ["rusqlite", "reqwest", "sqlx", "tokio", "hyper", "ureq"];
        for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
            for key in dependency_keys(&manifest, section) {
                assert!(
                    !key.starts_with("pos-") && !key.starts_with("pos_"),
                    "`pos-models` depends on `{key}` in [{section}] — the domain crate cannot depend on a crate that depends on it"
                );
                assert!(
                    !forbidden.contains(&key.as_str()),
                    "`pos-models` depends on `{key}` in [{section}] — that is a store or a transport, and the domain must not know about either"
                );
            }
        }
    }

    /// The keys of one dependency section of a `Cargo.toml`.
    ///
    /// Handles both `serde.workspace = true` and `serde = { … }`; the key is what precedes the
    /// first `.` or `=`.
    fn dependency_keys(manifest: &str, section: &str) -> BTreeSet<String> {
        let header = format!("[{section}]");
        manifest
            .lines()
            .map(str::trim)
            .skip_while(|line| *line != header)
            .skip(1)
            .take_while(|line| !line.starts_with('['))
            .filter(|line| !line.is_empty() && !line.starts_with('#') && line.contains('='))
            .map(|line| {
                let key = line.split('=').next().unwrap_or(line);
                key.split('.').next().unwrap_or(key).trim().to_string()
            })
            .filter(|key| !key.is_empty())
            .collect()
    }

    // ========================================================================
    // The build configuration
    // ========================================================================

    /// A clean clone can start building.
    ///
    /// `.cargo/config.toml` is the one file in the repository cargo reads before it does anything
    /// else, and it replaced the `crates-io` registry with a directory source at `vendor/` — a
    /// 1.1 GB tree that is gitignored and carried by no ref. Every clone therefore failed during
    /// dependency resolution, before compiling a line, and failed naming a missing directory rather
    /// than the config line that asked for it. It stayed invisible for as long as it did because
    /// the machine that wrote the setting is the one machine that has `vendor/`.
    ///
    /// The offline build did not go away; it moved to `.cargo/vendor.toml`, which cargo reads only
    /// when asked (`--config .cargo/vendor.toml`). So this guard holds both halves. Checking only
    /// the first would let the offline path be deleted as dead weight, and checking only the second
    /// is what the previous arrangement already did.
    #[test]
    fn the_config_cargo_reads_by_default_needs_nothing_a_clone_lacks() {
        let read = |relative: &str| {
            let path = repo_root().join(relative);
            fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {relative}: {e}"))
        };

        // ---- the file cargo reads automatically ----------------------------
        let config = read(".cargo/config.toml");
        let sections = toml_section_headers(&config);

        // A parser that returns nothing reports a clean file in the same words as a clean file.
        for expected in ["build", "env"] {
            assert!(
                sections.iter().any(|s| s == expected),
                "`.cargo/config.toml` has no [{expected}] section (parsed {sections:?}); either the build config moved or this guard's reader is broken and passing on nothing"
            );
        }

        let replacements: Vec<&String> = sections
            .iter()
            .filter(|section| section.starts_with("source."))
            .collect();
        assert!(
            replacements.is_empty(),
            "`.cargo/config.toml` declares {replacements:?}. Cargo reads this file for every invocation including a fresh clone's, so a source replacement here makes the build depend on a directory the clone does not have, and it fails before compiling anything. Put the replacement in `.cargo/vendor.toml` and opt into it with `--config .cargo/vendor.toml`."
        );

        // ---- the file that carries the replacement -------------------------
        let sidecar = read(".cargo/vendor.toml");
        let sidecar_sections = toml_section_headers(&sidecar);
        for expected in ["source.crates-io", "source.vendored-sources"] {
            assert!(
                sidecar_sections.iter().any(|s| s == expected),
                "`.cargo/vendor.toml` no longer declares [{expected}] (parsed {sidecar_sections:?}) — the offline build is the reason the replacement was moved rather than deleted, so if it is genuinely gone, delete the file and this half of the guard together"
            );
        }
        assert!(
            sidecar.contains("directory = \"vendor\""),
            "`.cargo/vendor.toml` no longer points at `vendor/`; the offline build and the `.gitignore` rule below disagree about where the vendored tree lives"
        );

        // ---- and the reason the two must stay apart ------------------------
        let ignored = read(".gitignore");
        let rules: Vec<&str> = ignored
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        assert!(
            rules.contains(&"/vendor/"),
            "`.gitignore` no longer ignores `/vendor/`. If the vendored tree is now committed, the split this guard enforces has no reason to exist — fold `.cargo/vendor.toml` back into `.cargo/config.toml` and delete this test."
        );
        assert!(
            !rules.contains(&"config.toml"),
            "`.gitignore` has an unanchored `config.toml` rule, which matches at any depth and so shadows the tracked `.cargo/config.toml` — a cloner who deletes and rewrites it gets a file git silently refuses to see. Anchor it as `/config.toml`."
        );
    }

    /// Every section header in a TOML file, in the order written.
    ///
    /// Naive on purpose, like [`dependency_keys`]: it reads the repository's own two small config
    /// files, not arbitrary TOML, and neither uses a quoted key containing `]`.
    fn toml_section_headers(text: &str) -> Vec<String> {
        text.lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix('[')?.strip_suffix(']'))
            .map(str::to_string)
            .collect()
    }
}
