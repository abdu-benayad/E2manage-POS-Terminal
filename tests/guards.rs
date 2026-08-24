//! Guards for the concepts this project has finished.
//!
//! `type-driven-domain-core` established the file and the first seven; later issues add their own,
//! because "every finished concept gets a guard test that runs" is a standing decision rather than
//! one issue's cleanup. A deletion is a concept like any other —
//! `the_till_never_carries_a_tenant_id` guards one.
//!
//! The founding issue's diagnosis of how this codebase drifted is that **a rule written in a
//! document is a rule nobody runs**. Two `OperatorPermissions` structs disagreed about the casing of the same
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
//! premises evaporated — the old view tree was deleted and `src/ui/` was migrated like any other
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

    use e2manage_pos_terminal::models::{Digit, EnteredDigits, Pin};

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
    // The whole-file layer
    // ========================================================================

    /// One scanned file, whole, with comments and string literals blanked in place.
    ///
    /// # Why there is a second reading layer at all
    ///
    /// [`scanned_lines`] hands out one line at a time, so a guard built on it can only express a
    /// predicate that fits on one line — and `rustfmt` does not respect that boundary. The shape
    /// that forced this is a positional read carrying a chained call, which rustfmt splits as
    ///
    /// ```text
    /// self.row
    ///     .get(index)
    /// ```
    ///
    /// where line one has no `.get` and line two has no `row`. A line scan cannot see it from
    /// either side, and reports a clean tree for a reason unrelated to the tree being clean.
    ///
    /// # Blanked, not deleted
    ///
    /// Comments and string literals are overwritten with spaces and newlines are kept, so every
    /// line break of the original survives at its own place. That is what lets a hit report a
    /// `file:line` a human can open. Byte offsets into `code` are *not* offsets into the file —
    /// a multi-byte char inside a blanked region becomes one space — so `code` may be used to
    /// locate a line and never to slice the original.
    struct SourceFile {
        path: String,
        code: String,
    }

    /// Every Rust source under the scan roots, read whole and blanked.
    ///
    /// Shares [`rust_sources`] with [`scanned_lines`] deliberately: two readers disagreeing about
    /// which files are the tree is the defect this file exists to make impossible elsewhere.
    fn scanned_files() -> Vec<SourceFile> {
        rust_sources()
            .iter()
            .map(|path| {
                let text = fs::read_to_string(path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                SourceFile {
                    path: relative(path),
                    code: blank_comments_and_strings(&text),
                }
            })
            .collect()
    }

    /// The 1-based line holding byte `offset` of a blanked file.
    fn line_at(code: &str, offset: usize) -> usize {
        code[..offset.min(code.len())].matches('\n').count() + 1
    }

    /// Blanks comments and string literals, leaving every newline and every other byte alone.
    ///
    /// Three constructs are removed, and the order of the tests below is the whole correctness
    /// argument: whichever opener the cursor reaches *first* wins, so `"http://x"` is a string
    /// (the quote comes first) and `// says "` is a comment (the slashes come first). Getting that
    /// backwards is how a scanner blanks the rest of a file from one apostrophe.
    ///
    /// - `//` to end of line, which is also what [`strip_comment`] does per line.
    /// - `/* … */`, **nesting**, because Rust nests them and a depth-free scan stops at the first
    ///   `*/` and hands the tail of the outer comment back as live code.
    /// - `"…"`, `r"…"`, `r#"…"#`, so a route literal or a SQL string is not read as source.
    fn blank_comments_and_strings(source: &str) -> String {
        let bytes: Vec<char> = source.chars().collect();
        let mut out = bytes.clone();
        let mut i = 0;
        while i < bytes.len() {
            let rest: String = bytes[i..(i + 3).min(bytes.len())].iter().collect();
            if rest.starts_with("//") {
                while i < bytes.len() && bytes[i] != '\n' {
                    out[i] = ' ';
                    i += 1;
                }
                continue;
            }
            if rest.starts_with("/*") {
                let mut depth = 0usize;
                while i < bytes.len() {
                    let here: String = bytes[i..(i + 2).min(bytes.len())].iter().collect();
                    let step = if here == "/*" {
                        depth += 1;
                        2
                    } else if here == "*/" {
                        depth -= 1;
                        2
                    } else {
                        1
                    };
                    for slot in out.iter_mut().take((i + step).min(bytes.len())).skip(i) {
                        if *slot != '\n' {
                            *slot = ' ';
                        }
                    }
                    i += step;
                    if depth == 0 {
                        break;
                    }
                }
                continue;
            }
            // Raw strings: r"…", r#"…"#, r##"…"##
            if bytes[i] == 'r' {
                let mut hashes = 0;
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] == '#' {
                    hashes += 1;
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == '"' {
                    let terminator: String = std::iter::once('"')
                        .chain(std::iter::repeat_n('#', hashes))
                        .collect();
                    let tail: String = bytes[j + 1..].iter().collect();
                    let end = tail.find(&terminator).map_or(bytes.len(), |at| {
                        j + 1 + tail[..at].chars().count() + terminator.chars().count()
                    });
                    for slot in out.iter_mut().take(end).skip(i) {
                        if *slot != '\n' {
                            *slot = ' ';
                        }
                    }
                    i = end;
                    continue;
                }
            }
            if bytes[i] == '"' {
                let mut j = i + 1;
                while j < bytes.len() {
                    if bytes[j] == '\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == '"' {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                for slot in out.iter_mut().take(j.min(bytes.len())).skip(i) {
                    if *slot != '\n' {
                        *slot = ' ';
                    }
                }
                i = j;
                continue;
            }
            i += 1;
        }
        out.into_iter().collect()
    }

    // ========================================================================
    // Reading a row by number
    // ========================================================================

    /// One read of a column by its **position** rather than its name.
    struct IndexedRead {
        path: String,
        /// 1-based, and the line the *receiver* is on — `row`, not `.get`. rustfmt puts those on
        /// different lines, and the receiver is where a reader looks.
        line: usize,
        /// The matched source, newlines included. Carried so a failure can print the shape it
        /// found rather than only a coordinate, and so a test can ask what form a hit took.
        text: String,
    }

    /// The offset of the first non-whitespace byte at or after `from`.
    fn skip_space(code: &str, from: usize) -> usize {
        code[from..]
            .find(|c: char| !c.is_whitespace())
            .map_or(code.len(), |at| from + at)
    }

    /// The offset just past a balanced `::<…>`, or `from` if one does not start there.
    ///
    /// **Depth-counted, and that is the whole point of hand-writing this.** The measurement that
    /// preceded this guard used `::<[^>]*>`, which cannot match `::<_, Option<i32>>` — the
    /// character class ends at the inner `>` — and it therefore reported a tree 13 reads cleaner
    /// than the tree was, three of them shipped. A predicate is a claim about the target's shape
    /// and is wrong exactly where the target is shaped unexpectedly.
    ///
    /// Bails at `(` or `;` so a `<` used as less-than can never run the scan off into the next
    /// statement.
    fn skip_turbofish(code: &str, from: usize) -> usize {
        let rest = &code[from..];
        if !rest.starts_with("::<") {
            return from;
        }
        let mut depth = 0usize;
        for (offset, ch) in rest.char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => {
                    depth -= 1;
                    if depth == 0 {
                        return from + offset + 1;
                    }
                }
                '(' | ';' => return from,
                _ => {}
            }
        }
        from
    }

    /// The offset just past an integer literal at `from`, or `None` if there is not one.
    ///
    /// Digits and nothing else, so `row.get(index)` and `row.get("name")` are not indexed reads.
    /// Both are deliberate in this tree: the cursor reads by a variable, and
    /// `crates/pos-db/tests/mappings.rs` reads `PRAGMA table_info` by column name.
    fn integer_literal(code: &str, from: usize) -> Option<usize> {
        if !code[from..].starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        // Everything a Rust integer literal may continue with: more digits, `_` separators, a
        // `0x`/`0b` radix, a `usize`/`u32` suffix. Deliberately looser than the grammar — it will
        // also accept `0abc`, which is not valid Rust. Where a check must choose an error
        // direction, choose the false positive: an over-match gets investigated on its first run
        // and an under-match gets quoted. Under-matching here would mean `row.get(0usize)` reads
        // as a variable and passes forever.
        Some(
            code[from..]
                .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                .map_or(code.len(), |at| from + at),
        )
    }

    /// `row . get [::<…>] ( <int> )`, from the byte just past the `row` token.
    ///
    /// **All four of rusqlite's indexed readers**, not the two that happened to be in the tree.
    /// `Row` offers `get`, `get_ref`, `get_unwrap` and `get_ref_unwrap` (`vendor/rusqlite/src/
    /// row.rs:264-344`); the first version of this predicate knew the two with call sites here,
    /// which is a survey certifying the survey — and the pair it omitted are the *panicking*
    /// ones, so the guard would have banned `get(0)` while pointing the next author at
    /// `get_unwrap(0)`. There are zero sites for either today; the arm is asserted against
    /// constructed input rather than a tree witness, and that is said out loud rather than left
    /// to look like coverage.
    ///
    /// Longest name first: [`starts_with_word`] refuses `get` in front of `get_ref`'s
    /// underscore, so the order is load-bearing, not stylistic.
    fn method_read_from(code: &str, after_row: usize) -> Option<usize> {
        let dot = skip_space(code, after_row);
        if !code[dot..].starts_with('.') {
            return None;
        }
        let at = skip_space(code, dot + 1);
        let name = ["get_ref_unwrap", "get_ref", "get_unwrap", "get"]
            .into_iter()
            .find(|name| starts_with_word(&code[at..], name))?;
        let at = skip_turbofish(code, at + name.len());
        let at = skip_space(code, at);
        if !code[at..].starts_with('(') {
            return None;
        }
        let at = skip_space(code, at + 1);
        let at = skip_space(code, integer_literal(code, at)?);
        code[at..].starts_with(')').then_some(at + 1)
    }

    /// `…( row , <int> )`, where the row is handed to a helper that does the indexing.
    ///
    /// **Not keyed on a list of helper names.** `column.rs`'s readers were such a list until task
    /// 13 made them private, and a name list's misses have no name, no expiry, and the same green
    /// — the failure mode is *not matching*, which no mutation drawn from the list can reveal.
    /// Anything that takes this row and a literal column number is reading it by position
    /// whatever it is called; `ColumnCodec::read(row, 0)` is the live population.
    fn argument_read_from(code: &str, row_start: usize, after_row: usize) -> Option<usize> {
        // `&row` and `&mut row` are the ordinary spellings — every reader in `column.rs` takes
        // `&Row<'_>` — so requiring a bare `(` immediately before the token was too strict, and
        // silently: the arm matched `read(row, 0)` and missed `read(&row, 0)`, which is the same
        // read written the way a caller with an owned row has to write it. Measured as a
        // surviving mutation, not reasoned about.
        let mut before = code[..row_start].trim_end();
        loop {
            before = before.trim_end();
            before = match before.strip_suffix('&') {
                Some(rest) => rest,
                None => match before.strip_suffix("mut") {
                    Some(rest) if !rest.ends_with(is_word_char) => rest,
                    _ => break,
                },
            };
        }
        if !before.ends_with(['(', ',']) {
            return None;
        }
        let at = skip_space(code, after_row);
        if !code[at..].starts_with(',') {
            return None;
        }
        let at = skip_space(code, at + 1);
        let at = skip_space(code, integer_literal(code, at)?);
        code[at..].starts_with(')').then_some(at + 1)
    }

    /// Every read-by-position in one blanked file.
    ///
    /// Anchored on the `row` receiver, which is the constraint that keeps `parts.get(2)` out —
    /// a slice read at `log_service.rs`, and a real false positive during this issue's review
    /// rather than a hypothetical one.
    fn indexed_reads_in(file: &SourceFile) -> Vec<IndexedRead> {
        let code = &file.code;
        let mut found = Vec::new();
        let mut from = 0;
        while let Some(offset) = code[from..].find("row") {
            let at = from + offset;
            let after = at + "row".len();
            from = after;

            let opens = at == 0 || !is_word_byte(code.as_bytes()[at - 1]);
            let closes = !code.as_bytes().get(after).is_some_and(|b| is_word_byte(*b));
            if !opens || !closes {
                continue;
            }

            if let Some(end) =
                method_read_from(code, after).or_else(|| argument_read_from(code, at, after))
            {
                found.push(IndexedRead {
                    path: file.path.clone(),
                    line: line_at(code, at),
                    text: code[at..end].to_string(),
                });
            }
        }
        found
    }

    /// Every read-by-position under the scan roots.
    fn indexed_reads() -> Vec<IndexedRead> {
        scanned_files().iter().flat_map(indexed_reads_in).collect()
    }

    // ========================================================================
    // Recognising a declaration
    // ========================================================================

    fn is_word_byte(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || byte == b'_'
    }

    /// [`is_word_byte`] over a `char`, for the places that walk a `&str` backwards.
    fn is_word_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || c == '_'
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

    /// The field name a declaration line declares, if it declares one.
    ///
    /// The inverse of [`declared_type`], and it exists because a rule keyed on a *list* of field
    /// names can only ever find the names on the list. Reading the name out lets a check ask the
    /// complement question — *what does this tree call PIN material that my list does not?* — and
    /// that question found `operator_pin` and `supervisor_pin`, which no vocabulary here named.
    fn declared_field_name(code: &str) -> Option<&str> {
        let (before, after) = code.split_once(':')?;
        if after.starts_with(':') {
            return None;
        }
        let name = before
            .trim()
            .strip_prefix("pub")
            .map_or(before.trim(), str::trim_start);
        (!name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))
            .then_some(name)
    }

    /// Whether a field name is one this tree uses for PIN material.
    ///
    /// Component-wise rather than substring, so `pinned` and `spinner` are not PINs, and
    /// suffix-tolerant, so `operator_pin` and `supervisor_pin` are. **Deliberately a predicate
    /// and not a list**: a list can only find its own members, and its misses have no name, no
    /// expiry, and produce a green indistinguishable from coverage.
    fn names_pin_material(field: &str) -> bool {
        field
            .split('_')
            .any(|part| matches!(part, "pin" | "pins" | "digit" | "digits" | "entered"))
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

        // The entry buffer's witness. `a_live_pin_never_reaches_a_derived_debug`'s buffer half
        // scans the bodies of `Debug`-deriving declarations for a field carrying keyed-in digits;
        // if the walker stops reaching `PinEntryStanding` — a `Debug`-deriving enum in the root
        // package that holds an `EnteredDigits` — that half goes quietly vacuous while still
        // reporting clean.
        //
        // A currently-true assertion, not a deferred one: this type exists as of `6120e2b`. It is
        // in the root package rather than a workspace crate, which also exercises the second
        // entry in `SCANNED_ROOTS`.
        assert!(
            declarations
                .iter()
                .any(|d| d.name == "PinEntryStanding" && d.path == "src/ui/sign_in/phase.rs"),
            "the walker did not find `PinEntryStanding` — the PIN buffer scan is reading a tree that does not contain the buffer"
        );

        // `only_the_transport_crates_name_a_route` needs its own positive control, and it is the
        // one guard here whose clean state is *also* its broken state: it passes when it finds no
        // route literal outside the transport crates, and a scan that reads no route literals
        // anywhere passes identically. So prove the corpus still contains the thing it looks for.
        let routes_seen = lines
            .iter()
            .filter(|line| line.code.contains("\"/api/") || line.code.contains("\"{}/api/"))
            .count();
        assert!(
            routes_seen > 20,
            "the scan found only {routes_seen} route literals in the whole tree; it is not reading \
             the strings `only_the_transport_crates_name_a_route` exists to locate, so that guard \
             is green against nothing"
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

        // `the_till_never_carries_a_tenant_id` is a scan for an absence, and an absence is what a
        // broken reader reports too. So both of its spellings need a witness that the same matcher
        // does find, or the guard passes on a tree it never read.
        let finds = |word: &str| lines.iter().any(|line| contains_word(&line.code, word));
        assert!(
            finds("company_id"),
            "no scanned line contains the word `company_id`; the tenant-id guard is scanning a tree it cannot read, and its snake_case arm is vacuous"
        );
        assert!(
            finds("companyId"),
            "no scanned line contains the word `companyId`. Measured 2026-08-23 there is exactly one in the shipped tree — the `\"companyId\"` key in `test_login_response_deserialization` (`crates/pos-api/src/auth.rs`), kept deliberately when its `tenantId` sibling was deleted. If that fixture is gone, the camelCase arm of the tenant-id guard now has no witness: give it another one rather than deleting this assertion"
        );

        // ------------------------------------------------------------------------------------
        // The whole-file layer
        // ------------------------------------------------------------------------------------
        //
        // The newest reader here, and the only one that can express a predicate spanning a
        // newline. Three separate things have to hold, and a green on any two of them is
        // consistent with the layer being useless:
        //
        //   1. it reads the same tree the line layer reads,
        //   2. it blanks what it promises to blank — otherwise every scan built on it matches
        //      prose about the pattern instead of the pattern,
        //   3. it recovers a construct the line layer structurally cannot see, which is the only
        //      reason to have paid for it.
        //
        // (3) is the one that would be skipped. A reader that reads every file and blanks every
        // comment and still cannot cross a newline passes (1) and (2) and buys nothing.

        let files = scanned_files();
        assert_eq!(
            files.len(),
            sources.len(),
            "the whole-file reader returned {} files where the line reader walked {}; the two \
             layers disagree about what the tree is, and a guard built on either is scanning a \
             different codebase than the one beside it",
            files.len(),
            sources.len()
        );

        let cursor_module = "crates/pos-db/src/projection.rs";
        let cursor = files
            .iter()
            .find(|file| file.path == cursor_module)
            .unwrap_or_else(|| panic!("{cursor_module} is not in the scanned set"));

        // (2), and it needs a positive control rather than an absence: assert the raw file holds
        // the text, then that the blanked one does not. A reader that returned the empty string
        // would satisfy the second half alone.
        let raw = fs::read_to_string(repo_root().join(cursor_module)).expect("the cursor reads");
        const IN_A_LINE_COMMENT: &str = "DELIBERATE and load-bearing";
        assert!(
            raw.contains(IN_A_LINE_COMMENT),
            "the witness text for `//` blanking is gone from {cursor_module}; pick another phrase \
             from a comment there rather than dropping the control"
        );
        assert!(
            !cursor.code.contains(IN_A_LINE_COMMENT),
            "the whole-file reader is not blanking `//` comments — every scan built on it will \
             match documentation of a banned shape as though it were the shape"
        );

        // The same control for the block-comment arm, which the line layer has never had and
        // therefore has never needed. One block comment exists in the shipped tree.
        const BLOCK_COMMENT_HOST: &str = "crates/pos-services/src/sync_service.rs";
        const IN_A_BLOCK_COMMENT: &str = "proceed with sync";
        let block_host_raw =
            fs::read_to_string(repo_root().join(BLOCK_COMMENT_HOST)).expect("the host reads");
        assert!(
            block_host_raw.contains(IN_A_BLOCK_COMMENT),
            "the tree's only `/* … */` comment is gone, so the block-comment arm of the reader now \
             has no witness. Give it another one — a scan that tolerates a construct it never \
             sees is indistinguishable from one that is blind to it"
        );
        let block_host = files
            .iter()
            .find(|file| file.path == BLOCK_COMMENT_HOST)
            .unwrap_or_else(|| panic!("{BLOCK_COMMENT_HOST} is not in the scanned set"));
        assert!(
            !block_host.code.contains(IN_A_BLOCK_COMMENT),
            "the whole-file reader is not blanking `/* … */` comments"
        );

        // (3). `RowCursor::take` in `projection.rs` carries `#[rustfmt::skip]` so that
        // `self.row` and `.get(index)` stay on two lines; task 02 planted it because that is the
        // shape rustfmt produces for a positional read with a chained call, and the form under
        // which every earlier measurement of this repo's positional reads came out wrong.
        //
        // The control is a comparison, not a hit: the same predicate is applied to the file whole
        // and to its lines one at a time. A line can never contain a newline, so the split read is
        // invisible to the second and the totals must differ. Join those two lines and this fails
        // — which is the point, and is what the note above `take` promises.
        fn reads_a_column(code: &str) -> usize {
            let mut found = 0;
            let mut from = 0;
            while let Some(offset) = code[from..].find(".get") {
                let at = from + offset;
                if code[..at].trim_end().ends_with("row") {
                    found += 1;
                }
                from = at + ".get".len();
            }
            found
        }

        let whole = reads_a_column(&cursor.code);
        let line_at_a_time: usize = cursor.code.lines().map(reads_a_column).sum();
        assert!(
            whole > line_at_a_time,
            "the whole-file reader found {whole} `row … .get` reads in {cursor_module} and a \
             line-at-a-time pass over the same text found {line_at_a_time}. They should differ: \
             `RowCursor::take` splits one across two lines under `#[rustfmt::skip]` precisely so \
             this comparison has something to be about. Equal totals mean either the reader has \
             stopped crossing newlines, or somebody joined those two lines and removed the only \
             witness in the tree for the thing this layer exists to do"
        );
    }

    /// The read-by-position scan sees every shape this tree has, and only those.
    ///
    /// The ban itself is task 15c's. This pins the *instrument*, because the instrument is the
    /// part that has been wrong every time: every count of positional reads taken in this
    /// repository before this predicate existed was low, and each was low in a way its author
    /// could not see from its output.
    ///
    /// Four of the five assertions below are about what the scan must **not** report. That ratio
    /// is deliberate — a scan that over-reports gets investigated on its first run, and a scan
    /// that under-reports gets quoted.
    #[test]
    fn the_read_by_position_scan_sees_the_forms_this_tree_has_and_no_others() {
        // Constructed inputs first, because after this issue most arms have no tree witness left
        // — and that is the *goal*, not a gap. An arm certified by neither a witness nor a
        // constructed case is asserted and never demonstrated, which is how a scan comes to
        // report a clean tree for a reason unrelated to the tree being clean.
        //
        // Each case runs through `blank_comments_and_strings` first, so this exercises the real
        // pipeline rather than the matcher in isolation.
        const CASES: [(&str, usize); 12] = [
            ("fn f(row: &Row) { let _ = row.get(3); }", 1),
            (
                "fn f(row: &Row) { let _ = row.get::<_, Option<i32>>(3); }",
                1,
            ),
            ("fn f(row: &Row) { let _ = row.get_unwrap(2); }", 1),
            ("fn f(row: &Row) { let _ = row.get_ref_unwrap(2); }", 1),
            ("fn f(row: &Row) { let _ = row.get(0usize); }", 1),
            ("fn f(row: &Row) { let _ = row.get(0x1f); }", 1),
            ("fn f(row: &Row) { let _ = row\n            .get(4); }", 1),
            ("fn f(row: &Row) { let _ = read(row, 0); }", 1),
            ("fn f(row: &Row) { let _ = read(&row, 3); }", 1),
            ("fn f(row: &Row) { let _ = row.get(index); }", 0),
            ("fn f(row: &Row) { let _ = row.get(\"name\"); }", 0),
            ("fn f(parts: &[u8]) { let _ = parts.get(2); }", 0),
        ];
        for (source, expected) in CASES {
            let file = SourceFile {
                path: "constructed".to_string(),
                code: blank_comments_and_strings(source),
            };
            assert_eq!(
                indexed_reads_in(&file).len(),
                expected,
                "the predicate is wrong about `{}`",
                source.replace('\n', " ⏎ ")
            );
        }

        let reads = indexed_reads();

        // The form that matters, and the only one no line-based scan can see. rustfmt splits a
        // positional read carrying a chained call, and that is how every one in this tree arose.
        let split: Vec<&IndexedRead> = reads.iter().filter(|r| r.text.contains('\n')).collect();
        assert!(
            !split.is_empty(),
            "the scan found no read split across two lines. There is exactly one in the shipped \
             tree — `row\\n    .get::<_, String>(0)` in the v13 migration test that proves \
             `SELECT pin_hash FROM operators` now errors (`crates/pos-db/src/migrations.rs`). If \
             that read is gone or has been joined onto one line, the repair is **another \
             deliberately-split read**, not a weaker assertion here: without one, this predicate's \
             newline tolerance is asserted and never demonstrated, and the whole reason it is \
             hand-rolled rather than line-based goes untested"
        );
        assert!(
            split
                .iter()
                .any(|r| r.path == "crates/pos-db/src/migrations.rs"),
            "the split read is no longer in `migrations.rs` but in {:?}; check it is still the \
             `pin_hash` guard read before accepting the new home",
            split.iter().map(|r| &r.path).collect::<Vec<_>>()
        );

        // The second arm — a row handed to something else that does the indexing — had exactly
        // one live population, `ColumnCodec::read(row, 0)` in `column.rs`'s codec tests, and this
        // assertion used to require it non-empty. **That requirement expired when the conversion
        // reached it**, and the assertion fired, correctly: those two reads now go through
        // `RowCursor::take_via`, which is the point of the whole issue. An arm whose population
        // has legitimately gone to zero cannot be certified from the tree, so it is certified
        // against constructed input above and that is said here rather than left as a green.
        //
        // This is the second time in this file a non-emptiness control has fired because the
        // migration removed what it was calibrated against, and both times the repair was to
        // re-point it rather than lower it.
        assert!(
            reads.iter().all(|r| r.text.contains(".get")),
            "the scan reported an `f(row, <int>)` read: {:?}. That arm has no live population — \
             if one has come back, give it a tree witness here",
            reads
                .iter()
                .filter(|r| !r.text.contains(".get"))
                .map(|r| (&r.path, &r.line))
                .collect::<Vec<_>>()
        );

        // `parts.get(2)` at `log_service.rs` is a slice read. A receiverless predicate sweeps it
        // in — measured, during this issue's review — so the `row` receiver is required, and this
        // file is the standing proof plus the host every mutation of this guard is inserted into.
        assert!(
            !reads.iter().any(|r| r.path.ends_with("log_service.rs")),
            "the scan reported a read in `log_service.rs`, which contains none: {:?}. Its \
             `parts.get(2)` is a `Vec` read, and matching it means the receiver check is gone",
            reads
                .iter()
                .filter(|r| r.path.ends_with("log_service.rs"))
                .map(|r| (&r.line, &r.text))
                .collect::<Vec<_>>()
        );

        // Named reads are deliberate and must survive. `crates/pos-db/tests/mappings.rs` reads
        // `PRAGMA table_info` by column name — a wrong name there resolves to nothing and fails,
        // which is the property that test is about.
        assert!(
            !reads.iter().any(|r| r.text.contains('"')),
            "the scan reported a read whose argument is a string: {:?}. `row.get(\"name\")` is a \
             named read and is not what this guard bans",
            reads
                .iter()
                .filter(|r| r.text.contains('"'))
                .map(|r| (&r.path, &r.line))
                .collect::<Vec<_>>()
        );

        // And a read by variable is the cursor doing its job, not a defect. `RowCursor::take`
        // holds the only index in the codebase that is *supposed* to be an index.
        assert!(
            !reads.iter().any(|r| r.text.contains("index")),
            "the scan reported `row.get(index)`, an identifier rather than a literal: {:?}. That \
             is `RowCursor::take`, the one place indexing is the point",
            reads
                .iter()
                .filter(|r| r.text.contains("index"))
                .map(|r| (&r.path, &r.line))
                .collect::<Vec<_>>()
        );
    }

    /// No code reads a row by column **number**.
    ///
    /// This is the concept `positional-row-access-in-pos-db` finished. A positional read matched a
    /// column list by hand — three unlinked lists per table, in the worst case — and nothing
    /// checked that the hand was steady. Inserting a column into a `SELECT` shifted every index
    /// after it; the types still lined up, because the columns either side were all TEXT; and the
    /// swapped read compiled, ran, and attributed one terminal's details to another. A declared
    /// mapping makes the SELECT list and the field bindings one artifact, so the wrong column is
    /// unrepresentable rather than merely currently-correct.
    ///
    /// # The rule needs no context, and that is why it is this rule
    ///
    /// **No row is read by position. Use a declared mapping, or [`scalar`] for a one-column
    /// query.** The rejected alternative was to permit index `0` — cheaper, since 54 of the 60
    /// reads were `row.get(0)` on `SELECT COUNT(*)`-shaped queries where a one-column projection
    /// has no ordinal to get wrong. It was rejected because **deciding whether a given `row.get(0)`
    /// is safe requires reading the `SELECT` list, which this scan cannot see.** `SELECT a,
    /// COUNT(*)` breaks it silently. That is the original defect — a positional read checked by
    /// hand against a column list no checker sees — reinstated one level up, inside the guard
    /// meant to abolish it, and it would have been a check that cannot observe what it claims.
    ///
    /// The safety argument for a one-column read still exists; it just lives where it can be
    /// enforced, in [`scalar`]'s own contract, rather than in a pattern scan's tolerance.
    ///
    /// # Two exemptions, neither a line number
    ///
    /// A line-number exemption in a file every migration appends to is a tripwire that fires on
    /// the correct change and teaches whoever hits it to bump a number — `migrations.rs` records
    /// that lesson three lines from the read exempted here.
    ///
    /// Both exemptions assert they are **non-empty**, because an exemption and a blind scan
    /// produce identical output: if the guard stops seeing what it tolerates, it fails rather than
    /// reporting a clean tree.
    #[test]
    fn every_row_is_read_through_its_mapping() {
        /// The cursor's own module. `RowCursor` is where the index arithmetic is *supposed* to
        /// live — one place, exercised by every generated mapping — and `scalar`/`optional_scalar`
        /// /`scalars` read the single column of a one-column query beside it.
        const CURSOR_MODULE: &str = "crates/pos-db/src/projection.rs";

        /// The one read no mapping can express, keyed on the statement it belongs to. The v13
        /// migration test asserts `pin_hash` is **gone**, so the read's success condition is that
        /// it *fails*. A mapping describes a column that exists.
        const REFUSAL_PROBE: &str = "SELECT pin_hash FROM operators";

        let reads = indexed_reads();

        // Exemption one, and its expiry. Renaming or emptying the cursor's module must fail here
        // rather than silently permitting everything under a path that no longer exists — the
        // construction `only_the_transport_crates_name_a_route` uses for its directory prefixes.
        assert!(
            repo_root().join(CURSOR_MODULE).is_file(),
            "{CURSOR_MODULE} does not exist. It is an exemption in this guard, so its \
             disappearance must be noticed here: move the exemption to wherever `RowCursor` now \
             lives, and do not delete it silently"
        );
        assert!(
            reads.iter().any(|read| read.path == CURSOR_MODULE),
            "{CURSOR_MODULE} is exempted from this guard and contains no read by position. An \
             exemption that covers nothing and a scan that sees nothing produce the same green, so \
             this is a failure: either the cursor has stopped indexing — in which case delete the \
             exemption — or the scan has stopped reading that file"
        );

        // Exemption two. The statement is located in the *raw* file, because `scanned_files()`
        // blanks string literals and the key to this exemption is a string literal.
        let spans = raw_statement_spans(REFUSAL_PROBE);
        assert_eq!(
            spans.len(),
            1,
            "{} statements in the tree contain `{REFUSAL_PROBE}`; this exemption is keyed on there \
             being exactly one. Zero means the v13 test proving the PIN hash column is gone has \
             moved or been deleted — re-key the exemption, or delete it with the test, but do not \
             let it lapse silently. More than one means the key no longer identifies a single \
             read, and the exemption has quietly widened to cover reads nobody approved. Found: \
             {:?}",
            spans.len(),
            spans
        );
        let (refusal_file, refusal_lines) = spans.into_iter().next().expect("exactly one");

        let offenders: Vec<&IndexedRead> = reads
            .iter()
            .filter(|read| read.path != CURSOR_MODULE)
            .filter(|read| !(read.path == refusal_file && refusal_lines.contains(&read.line)))
            .collect();

        assert!(
            offenders.is_empty(),
            "{} reads by column number outside the two exemptions:\n{}\n\nRead the row through a \
             declared mapping (`row_mapping!` / `row_reader!` with `read_one`/`read_all`), or \
             through `scalar`/`optional_scalar`/`scalars` when the query projects one column. An \
             index is only correct against a `SELECT` list nothing links it to.",
            offenders.len(),
            offenders
                .iter()
                .map(|read| format!(
                    "  {}:{}  {}",
                    read.path,
                    read.line,
                    read.text.replace('\n', " ⏎ ")
                ))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// The file and line range of every statement containing `needle`, searched in the **raw**
    /// source rather than the blanked source — the key to this exemption is a string literal, and
    /// `scanned_files()` blanks those.
    ///
    /// A span runs from the line holding `needle` to the line holding the `;` that ends the
    /// statement, so a read anywhere inside that statement is covered however rustfmt lays it out
    /// and an unrelated read on a neighbouring line is not. Every occurrence is returned rather
    /// than the first, so the caller can require that there is exactly one: an exemption keyed on
    /// a string that appears twice covers a read nobody approved.
    fn raw_statement_spans(needle: &str) -> Vec<(String, std::ops::RangeInclusive<usize>)> {
        let mut spans = Vec::new();
        for path in rust_sources() {
            let text = fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
            let lines: Vec<&str> = text.lines().collect();
            for (index, line) in lines.iter().enumerate() {
                if !line.contains(needle) {
                    continue;
                }
                let start = index + 1;
                let end = (start..=lines.len())
                    .find(|number| lines[number - 1].contains(';'))
                    .unwrap_or(start);
                spans.push((relative(&path), start..=end));
            }
        }
        spans
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
    /// `code`, `employee_id`, `employee_number`, `department` and `position` are deliberately not
    /// here. They are all bare strings and all mutually swappable, and they are not identity —
    /// they belong to a later tier, and naming them as out of scope is the point.
    ///
    /// `pin_hash` was on that list until schema v13 deleted it. It is named here rather than
    /// silently dropped, because "not identity" was the wrong reason to have excluded it: it was
    /// a bcrypt hash of every operator's PIN, sent to every enrolled terminal, and the fix was
    /// deletion rather than a newtype.
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
        // `Pin::parse` takes no policy: a tenant's required length governs minting, and the till
        // has no minting door. Four digits are a shape the platform accepts, which is all this
        // function judges.
        let pin = Pin::parse("1234").expect("four ASCII digits are a platform-legal PIN");
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

        // ------------------------------------------------------------------
        // The entry buffer, which the phase machine created and no name-based
        // rule above would have seen.
        // ------------------------------------------------------------------

        // Same shape as the `Pin` assertion: the structural scan below is only worth anything
        // while this holds. A length-tracking redaction would leak how much has been typed, one
        // frame at a time, to anything logging the phase.
        let mut entered = EnteredDigits::empty();
        for value in [1, 2, 3, 4] {
            entered.push(Digit::new(value).expect("a single decimal digit"));
        }
        assert_eq!(format!("{entered:?}"), "EnteredDigits(****)");
        assert_eq!(format!("{entered:#?}"), "EnteredDigits(****)");

        // A field carrying keyed-in PIN digits must be declared as `Pin` or `EnteredDigits`,
        // whatever it is called. `pin` alone was the old vocabulary and it was enough while the
        // only PIN in the tree was a finished one; an entry buffer is called other things.
        const REDACTED_CARRIERS: [&str; 2] = ["Pin", "EnteredDigits"];

        /// Whether a declared type could hold keyed-in PIN digits, as opposed to a count of them.
        ///
        /// `digits: u8` is a *length* — `PinPolicyError` and `DetailsBreach` each carry one — and
        /// flagging those would be a rule that cannot tell a count from a buffer, needing an
        /// exemption that never expires.
        ///
        /// **The boundary is `u8`-sized, not "scalar", and the difference is a real hole.** An
        /// earlier version of this comment said *a scalar cannot carry a PIN*. That is false as a
        /// general predicate: the shortest PIN this domain admits is four digits
        /// (`PinLength::SHORTEST`), whose largest value is 9999 — which fits a `u16` and every
        /// wider integer. Only the 8-bit widths genuinely cannot, and that is precisely why the
        /// two `u8` sites are safe. An integer PIN buffer is a bad choice somebody nonetheless
        /// makes, and it must not fall through a rule that reads like a type check while behaving
        /// like a whitelist.
        ///
        /// Found by lane 21 reviewing the narrowing, not by this guard — a whitelist's misses are
        /// invisible to the whitelist.
        fn can_hold_digits(declared: &str) -> bool {
            const SEQUENCES: [&str; 6] =
                ["String", "str", "Vec<u8>", "[u8", "Vec<Digit>", "Box<str>"];
            // Checked before the widths below, so `Vec<u8>` and `[u8; 4]` are carriers rather
            // than being read as the `u8` they contain.
            if SEQUENCES.iter().any(|carrier| declared.contains(carrier)) {
                return true;
            }

            const TOO_NARROW_FOR_A_PIN: [&str; 3] = ["u8", "i8", "bool"];
            if TOO_NARROW_FOR_A_PIN
                .iter()
                .any(|narrow| contains_word(declared, narrow))
            {
                return false;
            }

            const WIDE_ENOUGH: [&str; 10] = [
                "u16", "u32", "u64", "u128", "usize", "i16", "i32", "i64", "i128", "isize",
            ];
            WIDE_ENOUGH.iter().any(|wide| contains_word(declared, wide))
        }

        let buffer_offences: Vec<String> = type_declarations()
            .iter()
            .filter(|declaration| derives(&declaration.attributes, "Debug"))
            .flat_map(|declaration| {
                declaration.body.iter().filter_map(move |line| {
                    let field = declared_field_name(&line.code)?;
                    if !names_pin_material(field) {
                        return None;
                    }
                    let declared = declared_type(&line.code, field)?;
                    let redacted = REDACTED_CARRIERS
                        .iter()
                        .any(|carrier| contains_word(declared, carrier));
                    (!redacted && can_hold_digits(declared)).then(|| {
                        format!(
                            "{}:{} — `{}` derives `Debug` over `{}: {}`",
                            line.path,
                            line.number,
                            declaration.name,
                            field,
                            declared.trim_end_matches(',')
                        )
                    })
                })
            })
            .collect();

        assert!(
            buffer_offences.is_empty(),
            "keyed-in PIN digits are exposed to a derived `Debug` in {} place(s); hold them as \
             `pos_models::EnteredDigits`, or drop the derive until you can:\n  {}",
            buffer_offences.len(),
            buffer_offences.join("\n  ")
        );

        // The type-directed half, and the one the task named. Every type that holds the buffer
        // holds it *as* `EnteredDigits`, whose `Debug` is the redaction asserted above — so this
        // set is the safe population, and its job is to be non-empty. An exemption and a blind
        // scan produce identical output; only a positive separates them.
        let holders: Vec<String> = type_declarations()
            .into_iter()
            .filter(|declaration| {
                declaration
                    .body
                    .iter()
                    .any(|line| contains_word(&line.code, "EnteredDigits"))
            })
            .map(|declaration| declaration.name)
            .collect();

        assert!(
            holders.iter().any(|name| name == "PinEntryStanding"),
            "the scan cannot see `PinEntryStanding`, which holds the entry buffer — the two \
             assertions above are then passing over a tree they are not reading. Found: {holders:?}"
        );

        // ------------------------------------------------------------------
        // The complement check: is the matcher blind to a name this tree uses?
        // ------------------------------------------------------------------
        //
        // Everything above asks "does any field the matcher recognises hold digits unsafely?" —
        // a question whose misses are silent, because a name the matcher does not recognise is
        // indistinguishable from a name that is not there. This asks the inverse, using the
        // population that is already *safe* as the oracle: every field declared as `Pin` or
        // `EnteredDigits` is, by construction, a name this codebase uses for PIN material. If the
        // matcher does not recognise one of those, it would equally not recognise the same name
        // holding a `String`.
        //
        // This is not hypothetical. `operator_pin` and `supervisor_pin` are real fields here, and
        // the original rule — which matched the bare word `pin` — missed both, because
        // `declared_type` requires a word boundary and `_` is a word byte. The list could not
        // find them and could not report that it had not.
        let unrecognised: Vec<String> = type_declarations()
            .iter()
            .flat_map(|declaration| {
                declaration.body.iter().filter_map(move |line| {
                    let field = declared_field_name(&line.code)?;
                    let declared = declared_type(&line.code, field)?;
                    let carries_pin_material = REDACTED_CARRIERS
                        .iter()
                        .any(|carrier| contains_word(declared, carrier));
                    (carries_pin_material && !names_pin_material(field))
                        .then(|| format!("{}:{} — `{}`", line.path, line.number, field))
                })
            })
            .collect();

        assert!(
            unrecognised.is_empty(),
            "{} field(s) hold PIN material under a name `names_pin_material` does not recognise. \
             The same name holding a `String` would pass unnoticed — extend the predicate:\n  {}",
            unrecognised.len(),
            unrecognised.join("\n  ")
        );

        // Positive control for the sweep above. An empty `unrecognised` means either "the
        // matcher covers every name in use" or "the walk found no PIN fields at all", and those
        // read identically. This separates them.
        let recognised: Vec<String> = type_declarations()
            .iter()
            .flat_map(|declaration| {
                declaration.body.iter().filter_map(move |line| {
                    let field = declared_field_name(&line.code)?;
                    let declared = declared_type(&line.code, field)?;
                    let carries = REDACTED_CARRIERS
                        .iter()
                        .any(|carrier| contains_word(declared, carrier));
                    (carries && names_pin_material(field)).then(|| field.to_string())
                })
            })
            .collect();

        assert!(
            recognised.len() >= 3,
            "the complement sweep recognised only {} PIN-material field(s); it is walking an \
             empty population and its clean result means nothing. Found: {recognised:?}",
            recognised.len()
        );

        // KNOWN GAP, stated rather than left for someone to discover.
        //
        // Both scans are line-based, so two shapes are invisible to them: a field declaration
        // rustfmt has split across lines, and a *tuple* variant, which carries a type with no
        // field name to match on. `PinEntryStanding::Entering(EnteredDigits)` is the second kind
        // — it is confirmed present by the `holders` assertion above, but a change to
        // `Entering(String)` would pass the vocabulary scan, because there is no `name:` to read.
        //
        // The whole-file reader that closes the first shape is lane 21's tasks 15a-c, and
        // `scanned_lines()` is `text.lines()` until it lands. This guard is deliberately the
        // narrowest thing that works rather than a second scanner beside theirs; rebuild both
        // halves on the shared reader when it exists, and cover tuple variants then.
    }

    /// The till carries no tenant id.
    ///
    /// `LoginTerminalResponse` required `tenantId` and the platform stopped sending it, so
    /// `POST /api/pos/terminals/login` was undeserialisable across four production call sites for
    /// an unknown length of time. `PairedTerminalInfo` failed the same way and worse, breaking
    /// pairing at the moment it succeeded rather than while it was pending.
    ///
    /// The fix was a deletion rather than a design question, and the measurement is what decided
    /// it: of 25 occurrences across the tree, **not one read the value for anything** — no request
    /// header, no query parameter, no scoping decision, no view-model bridge. There is no role here
    /// for a `TenantId` newtype to name.
    ///
    /// So this guard is not "the spelling is banned". It is: **a field the till never consumes does
    /// not get carried.** If a genuine tenancy need appears it arrives *with* its consumer, and the
    /// type is designed against that consumer — at which point deleting this test is the deliberate
    /// act that reopens the question, which is the whole point of it existing.
    ///
    /// Both spellings, because the concept has two vocabularies here as everywhere else: the wire
    /// sends `tenantId` and the store and the structs say `tenant_id`. A scan for one is blind to
    /// the other, and the wire spelling is the one that appears in a JSON fixture — which is
    /// precisely where this defect hid.
    ///
    /// `tests/` stays out of scope, as it does for every scan here, and that is load-bearing rather
    /// than inherited: `scripts/setup-e2e-test-data.sql` and `scripts/setup-test-env.sh` name
    /// `pos_tenant_configurations.tenant_id`, which is the **platform's** own column — a different
    /// concept that happens to share a spelling.
    #[test]
    fn the_till_never_carries_a_tenant_id() {
        const SPELLINGS: [&str; 2] = ["tenant_id", "tenantId"];

        let offences: Vec<String> = scanned_lines()
            .iter()
            .filter(|line| {
                SPELLINGS
                    .iter()
                    .any(|spelling| contains_word(&line.code, spelling))
            })
            .map(|line| format!("{}:{} {}", line.path, line.number, line.code.trim()))
            .collect();

        assert!(
            offences.is_empty(),
            "a tenant id is back in the shipped tree in {} place(s). The till is write-only about \
             one: it was deserialised, carried, persisted to two SQLite columns and read back, and \
             consumed by nothing, which is why it and its columns were deleted rather than \
             defaulted. If this reintroduction has a real consumer, that consumer is the argument \
             for a `TenantId` newtype and for deleting this guard in the same increment — do both \
             deliberately, not by adding a field:\n  {}",
            offences.len(),
            offences.join("\n  ")
        );
    }

    /// A lockout expiry is rendered and never stored.
    ///
    /// The server sends `lockedUntil` so a till can say *"locked until 14:32"* to the person at the
    /// drawer. It is the one field in the refusal catalogue that will look like something it must
    /// not be, and the mistake it invites is a single line: keep it, and unlock when the local
    /// clock passes it.
    ///
    /// PCI DSS v4.0 §8.3.4 permits a lockout to end after thirty minutes **or** when the user's
    /// identity is confirmed. The till takes the second branch, and
    /// [`pos_models::LockState`](../crates/pos-models/src/verification.rs) has no expiry field on
    /// purpose: the first branch is a timer read from the clock of whoever is holding the device,
    /// so anyone who can set the date ends their own lockout. `CredentialExpiry` may hold a time
    /// because expiry fails **closed**; a lockout that expires fails **open**.
    ///
    /// # What the type already refuses, and why this scan still earns its place
    ///
    /// `LockoutNotice` derives no `PartialOrd`, so `notice < Utc::now()` does not compile. That
    /// closes the direct comparison and nothing else — `instant_to_render()` hands out a
    /// `DateTime<Utc>` that compares fine, because rendering it in the operator's own zone is what
    /// it is for. So the two shapes a type cannot refuse are checked here: **writing it down**, and
    /// **reading it beside a clock**.
    ///
    /// This is the half of `auth-outcome-and-offline-lockout`'s acceptance row 16 that is a claim
    /// about the tree rather than about a value. The other half — that the instant reaches the
    /// caller at all — is `crates/pos-api/tests/transport_failures.rs`.
    #[test]
    fn a_lockout_notice_is_never_stored() {
        /// The wire spelling, the Rust spelling, and the type. A concept has as many vocabularies
        /// as it has boundaries, and a scan keyed on one of them is blind to the others.
        const SPELLINGS: [&str; 3] = ["lockedUntil", "locked_until", "LockoutNotice"];
        /// Statements that write or read a row. A lockout expiry belongs in none of them.
        const SQL: [&str; 6] = [
            "INSERT",
            "UPDATE ",
            "CREATE TABLE",
            "SELECT",
            "ALTER",
            "REPLACE",
        ];
        /// The crate that owns the till's SQLite store.
        const STORE: &str = "crates/pos-db/";

        // The positive control. Without it this test passes forever the day the type is renamed,
        // reporting a clean tree because it is scanning for a word nobody writes.
        let declared: Vec<String> = scanned_lines()
            .iter()
            .filter(|line| line.code.contains("pub struct LockoutNotice"))
            .map(|line| format!("{}:{}", line.path, line.number))
            .collect();
        assert_eq!(
            declared.len(),
            1,
            "`LockoutNotice` must be declared exactly once for this guard to mean anything; \
             found {declared:?}"
        );

        let persisted: Vec<String> = scanned_lines()
            .iter()
            .filter(|line| {
                let names_it = SPELLINGS
                    .iter()
                    .any(|spelling| contains_word(&line.code, spelling));
                let in_the_store = line.path.replace('\\', "/").contains(STORE);
                let in_a_statement = SQL.iter().any(|keyword| line.code.contains(keyword));
                names_it && (in_the_store || in_a_statement)
            })
            .map(|line| format!("{}:{} {}", line.path, line.number, line.code.trim()))
            .collect();

        assert!(
            persisted.is_empty(),
            "a lockout expiry is being written down in {} place(s). It is a sentence to draw on \
             a screen, not a condition: an unlock the till decides from a stored instant is an \
             unlock an attacker performs by changing the clock. A lock ends when someone confirms \
             the operator identity behind it, and `LockState` has no expiry field for exactly \
             this reason:\n  {}",
            persisted.len(),
            persisted.join("\n  ")
        );

        let compared: Vec<String> = scanned_lines()
            .iter()
            .filter(|line| line.code.contains("instant_to_render") && line.code.contains("now("))
            .map(|line| format!("{}:{} {}", line.path, line.number, line.code.trim()))
            .collect();

        assert!(
            compared.is_empty(),
            "a lockout notice is being read beside a clock in {} place(s). `instant_to_render` \
             exists to format the instant for a person; comparing it against the local time is \
             the timer this whole design refuses:\n  {}",
            compared.len(),
            compared.join("\n  ")
        );
    }

    /// Only the crates that own transport may name a route.
    ///
    /// `doc/architecture` states it as an invariant — *"`pos-api` is the only thing that knows the
    /// network exists"* — and until 2026-08-23 it was false in twelve places. `pos-services`
    /// assembled `/api/pos/...` strings inline, which is not a style problem: it put the choice of
    /// **how to read the reply** in a file that does not contain the DTO, decided by someone not
    /// looking at the route. Sixteen call sites read the wrong wire shape as a direct result, in
    /// both directions — a raw read of an enveloped route, and an enveloped read of a route that
    /// sends no `data`.
    ///
    /// So this guard is not about tidiness. A route literal outside these crates is the return of
    /// the condition that produced the defect class.
    ///
    /// # The crate list is a statement, not an exemption
    ///
    /// The module docs say there are no exemptions here, and this is not one: it names who owns
    /// the network, which is a permanent fact about the architecture rather than a temporary
    /// allowance waiting to expire. It is still held to the same standard — each entry must name a
    /// directory that **exists**, so a crate that is renamed or deleted fails this test instead of
    /// silently widening it into a pattern that matches nothing.
    ///
    /// `pos-updater` is on the list for a reason worth knowing rather than assuming: it is
    /// `[workspace] exclude`d, carries its own `reqwest::Client` and its own envelope type, and
    /// never touches `ApiClient`. Nothing in `pos-api` can protect it, and the pact cannot see it
    /// either — a platform change to `/api/pos/version/check` breaks it silently. That is a real
    /// gap, recorded in `till/doc/till-consumer-surface-audit`, not something this guard closes.
    #[test]
    fn only_the_transport_crates_name_a_route() {
        /// Directories permitted to contain a route literal. See the doc comment: this names who
        /// owns the network.
        const TRANSPORT_CRATES: [&str; 3] = [
            "crates/pos-api/",
            "crates/pos-contract/",
            "crates/pos-updater/",
        ];

        for crate_dir in TRANSPORT_CRATES {
            let path = repo_root().join(crate_dir);
            assert!(
                path.is_dir(),
                "{} is on the transport allowlist and does not exist. A stale entry turns this \
                 guard into a pattern that matches nothing, which is the failure mode the module \
                 docs refuse: fix the list deliberately rather than leaving it to rot",
                path.display()
            );
        }

        let offences: Vec<String> = scanned_lines()
            .iter()
            .filter(|line| !TRANSPORT_CRATES.iter().any(|c| line.path.starts_with(c)))
            .filter(|line| line.code.contains("\"/api/") || line.code.contains("\"{}/api/"))
            .map(|line| format!("{}:{} {}", line.path, line.number, line.code.trim()))
            .collect();

        assert!(
            offences.is_empty(),
            "a route is named outside the crates that own transport, in {} place(s). This is how \
             the wire-shape defects got in: the helper choice ends up in a file that does not hold \
             the DTO. Add a typed method on `ApiClient` beside the response type and call that \
             instead — `crates/pos-api/src/transactions.rs` is the pattern:\n  {}",
            offences.len(),
            offences.join("\n  ")
        );
    }

    /// The till's writes go through the till's own door, never the back office's.
    ///
    /// The platform did not solve "this till holds no credential the POS write routes accept" by
    /// loosening `/api/pos/{transactions,shifts,returns}` — those are genuine user-session routes
    /// and loosening them would have been a CSRF hole. It gave the till **six audience-filtered
    /// routes of its own** at `/api/pos/till/*`, behind `[terminalAuth, attendedOperatorAuth]`
    /// (`pos.routes.ts:163-165`). The back-office mounts are deliberately untouched and still
    /// answer a cookieless caller.
    ///
    /// # Why this needs a guard rather than a code review
    ///
    /// The failure it prevents is **silent in the worst way**. When the till's paths drifted
    /// before, the symptom was a 404. Here the old routes still exist and still refuse, so a
    /// regression produces no new error at all — the write simply keeps failing exactly as it did
    /// when nobody had noticed. That is why this issue existed: the platform moved *toward* the
    /// till and nothing told the till.
    ///
    /// # Both halves, and the second is the one that matters
    ///
    /// A guard that only forbids the old literals passes trivially against a till that has deleted
    /// all six methods, or repointed them somewhere else again. So it also asserts the six are
    /// **present**. A rule matched only against the population it was written from certifies that
    /// population; the positive half is what makes this a ratchet instead.
    #[test]
    fn the_till_writes_through_the_till_mounts() {
        /// The six routes the platform opened, exactly as `ApiClient` must spell them.
        const TILL_WRITE_ROUTES: [&str; 6] = [
            "\"/api/pos/till/transactions\"",
            "\"/api/pos/till/transactions/{}/void\"",
            "\"/api/pos/till/transactions/by-receipt/{}\"",
            "\"/api/pos/till/shifts/start\"",
            "\"/api/pos/till/shifts/{}/end\"",
            "\"/api/pos/till/returns\"",
        ];

        /// The back-office mounts. Reachable with a user JWT, which this till does not hold.
        const BACK_OFFICE_MOUNTS: [&str; 3] = [
            "\"/api/pos/transactions",
            "\"/api/pos/shifts",
            "\"/api/pos/returns",
        ];

        let scanned = scanned_lines();
        let client_lines: Vec<&SourceLine> = scanned
            .iter()
            .filter(|line| line.path.starts_with("crates/pos-api/src/"))
            .collect();

        assert!(
            !client_lines.is_empty(),
            "the scan found no lines under crates/pos-api/src/, so both halves below would pass \
             against an empty corpus. That is the no-answer-wearing-an-answer's-clothes failure, \
             not a green"
        );

        let regressed: Vec<String> = client_lines
            .iter()
            .filter(|line| BACK_OFFICE_MOUNTS.iter().any(|m| line.code.contains(m)))
            .map(|line| format!("{}:{} {}", line.path, line.number, line.code.trim()))
            .collect();

        assert!(
            regressed.is_empty(),
            "the till names a back-office write mount in {} place(s). Those routes want a user \
             JWT this till has never held; the till's own mounts are `/api/pos/till/*`. This does \
             not fail loudly at runtime — the old route exists and refuses, so the symptom is \
             unchanged and there is no new error to notice:\n  {}",
            regressed.len(),
            regressed.join("\n  ")
        );

        let missing: Vec<&str> = TILL_WRITE_ROUTES
            .iter()
            .filter(|route| !client_lines.iter().any(|line| line.code.contains(*route)))
            .copied()
            .collect();

        assert!(
            missing.is_empty(),
            "{} of the six till write routes is/are named nowhere in `pos-api`. Either a write was \
             deleted, or one was repointed away from the door the platform opened for it — and \
             the negative half of this guard would pass either way:\n  {}",
            missing.len(),
            missing.join("\n  ")
        );
    }

    /// The till never asks the platform to hand back its own secret.
    ///
    /// `POST /api/pos/terminals/pairing/recover` took a `hardwareId` and returned that terminal's
    /// **raw secret**, with no authentication and no rate limit. A `hardwareId` is client-chosen,
    /// validated only for length, unique across every tenant, and was written to the platform's
    /// application log — so the secret was retrievable by anyone who could guess a device name.
    /// The till was not the victim of that hole; it was the caller, and its whole re-enrolment
    /// story was the attack performed by the legitimate client.
    ///
    /// The platform deleted the route in `566e5c62` and asserts its absence in
    /// `presentation/__tests__/pairing-recover-is-gone.test.ts`. This is the consumer-side mirror.
    ///
    /// **There is no safe version of this endpoint, which is why the guard bans the name rather
    /// than policing its use.** A till that has lost its secret is indistinguishable from an
    /// attacker claiming that hardware id: whatever the device could prove is the thing it lost.
    /// Re-enrolment goes through `pairing/request` and an administrator, and nothing hands a
    /// credential to an unauthenticated caller.
    ///
    /// The scan reads comment-stripped lines, so the explanatory comment standing where
    /// `recover_registration` used to be in `crates/pos-api/src/auth.rs` names the route freely.
    /// That is deliberate and the module docs require it: documenting why a shape is forbidden
    /// must never break the build.
    #[test]
    fn the_till_never_asks_for_its_own_secret_back() {
        const ROUTE: &str = "pairing/recover";
        const METHOD: &str = "fn recover_registration";

        let lines = scanned_lines();

        // The positive control, and the reason this guard is not vacuous. A scan for an absence
        // reports a clean tree in exactly the same words as a tree it could not read, so prove the
        // same matcher still finds the sibling routes it is *not* banning.
        for witness in ["pairing/request", "pairing/status"] {
            assert!(
                lines.iter().any(|line| line.code.contains(witness)),
                "no scanned line contains `{witness}`. The sibling pairing routes are this \
                 guard's only witness that it is reading the tree at all — without one, its \
                 green means nothing. If `pos-api` genuinely stopped naming them, give this \
                 guard another witness rather than deleting the assertion"
            );
        }

        let offences: Vec<String> = lines
            .iter()
            .filter(|line| line.code.contains(ROUTE) || line.code.contains(METHOD))
            .map(|line| format!("{}:{} {}", line.path, line.number, line.code.trim()))
            .collect();

        assert!(
            offences.is_empty(),
            "the till names the deleted secret-recovery route in {} place(s). That endpoint \
             returned a terminal's raw secret for a client-chosen hardware id and the platform \
             removed it; there is no replacement and none is possible, because a till that has \
             lost its secret cannot be told apart from an attacker claiming its hardware id. \
             Re-enrol through `pairing/request` and an administrator:\n  {}",
            offences.len(),
            offences.join("\n  ")
        );
    }

    /// A refusal is read from a machine code, never out of prose — and the list of places that
    /// still get this wrong only shrinks.
    ///
    /// Messages are translated, product names contain digits, and none of it is a contract.
    /// `ServerErrorCode` is. The worst instance matched the word `"invalid"`, which classified
    /// `POS_OPERATOR_SESSION_INVALID` as a permanent data fault and **abandoned a queued sale
    /// forever**; the pairing path matched `"409"` and `"already registered"` to decide whether to
    /// ask the platform for its own secret back.
    ///
    /// # Why this keys on the SHAPE of the line and not on which words it looks for
    ///
    /// The first version of this guard matched `contains("4xx")` / `contains("5xx")` — the
    /// spellings found by surveying the tree. Measured against the real population, that was blind
    /// to **seven of the nine** matchers in `SyncFailureType::classify` alone: `"invalid"`,
    /// `"conflict"`, `"duplicate"`, `"already exists"`, `"not found"`, `"validation failed"`,
    /// `"malformed"` — including the exact word whose damage is named above. A guard keyed on the
    /// vocabulary you happened to find silently defines its target as *"a thing shaped like the
    /// ones I saw"*, and its green then reads as coverage it does not have.
    ///
    /// **The vocabulary is the survey; the shape is the rule.** So this matches a *branch* —
    /// a line that begins `if` / `} else if` / `||` / `&&` / `return` and tests an error-shaped
    /// value with `.contains("` — and says nothing about which string. That distinguishes
    /// *deciding* from *describing*: the assertions in `pos-models/src/operator.rs` and
    /// `pos-api/src/client.rs` that check a rendered message begin with the receiver instead, and
    /// are correctly ignored. One of them asserts a rejected **discount percent** reaches an error
    /// message via `contains("500")` — nothing to do with HTTP, and a false positive the first
    /// version of this guard would have turned red on day one.
    ///
    /// # The allowance list is a ratchet, not an exemption
    ///
    /// These files carry branches this issue did not own. Listing them is what lets the guard
    /// exist at all — a flat ban cannot pass today — and every property below exists to stop the
    /// list becoming permanent furniture. It is keyed on **paths, not counts**, deliberately:
    /// other sessions edit these files concurrently, and a pinned count would collide with work
    /// this guard has no opinion about. It still turns red the moment the pattern reaches a file
    /// that is not listed.
    #[test]
    fn the_till_never_reads_a_refusal_out_of_prose() {
        /// Files that still branch on prose, and the issues that own them. A path leaves this list
        /// when its branches are gone — never because the list was tidied.
        const ALLOWED: [&str; 3] = [
            "crates/pos-services/src/offline_service.rs",
            "crates/pos-services/src/sync_service.rs",
            "crates/pos-services/src/shared_draft_service.rs",
        ];

        /// A line that *decides* something, as opposed to one that describes or asserts.
        const BRANCH: [&str; 5] = ["if ", "} else if ", "|| ", "&& ", "return "];

        /// Reading a rendered error. `.to_string()` / `.to_lowercase()` catch the chained forms,
        /// including `!e.to_string().contains(…)`.
        const READS_AN_ERROR: [&str; 10] = [
            "msg.contains(\"",
            "message.contains(\"",
            "err.contains(\"",
            "err_str.contains(\"",
            "error.contains(\"",
            "error_msg.contains(\"",
            "error_str.contains(\"",
            "e.contains(\"",
            ".to_string().contains(\"",
            ".to_lowercase().contains(\"",
        ];

        fn branches_on_prose(code: &str) -> bool {
            let trimmed = code.trim_start();
            BRANCH.iter().any(|start| trimmed.starts_with(start))
                && READS_AN_ERROR.iter().any(|read| trimmed.contains(read))
        }

        let lines = scanned_lines();
        let matches: Vec<&SourceLine> = lines
            .iter()
            .filter(|line| branches_on_prose(&line.code))
            .collect();

        // The positive control. Without it, a matcher broken in any way reports a clean tree —
        // and so do BOTH assertions below, which would then fail open together.
        assert!(
            !matches.is_empty(),
            "the matcher found no prose branch anywhere in the tree. Measured 2026-08-24 there \
             are eleven, nine of them in `SyncFailureType::classify`. Finding none means this \
             guard is scanning something it cannot read, and its green is about nothing"
        );

        let offences: Vec<String> = matches
            .iter()
            .filter(|line| {
                let path = line.path.replace('\\', "/");
                !ALLOWED.iter().any(|allowed| path == *allowed)
            })
            .map(|line| format!("{}:{} {}", line.path, line.number, line.code.trim()))
            .collect();

        assert!(
            offences.is_empty(),
            "a refusal is being read out of a rendered message in {} new place(s). Branch on \
             `ServerErrorCode`, never on prose: messages are translated, product names contain \
             digits, and none of it is a contract. The worst instance of this matched the word \
             \"invalid\", classified `POS_OPERATOR_SESSION_INVALID` as a permanent data fault, \
             and abandoned a queued sale forever:\n  {}",
            offences.len(),
            offences.join("\n  ")
        );

        // An allowance that has stopped matching is an exemption nobody noticed expiring, and the
        // module docs refuse those. A path leaves this list deliberately, not by rotting.
        for allowed in ALLOWED {
            let path = repo_root().join(allowed);
            assert!(
                path.is_file(),
                "{} is on the prose-branching allowance list and does not exist. Remove the entry \
                 in the same commit that moved the file, rather than leaving a pattern that \
                 matches nothing",
                path.display()
            );
            assert!(
                matches
                    .iter()
                    .any(|line| line.path.replace('\\', "/") == *allowed),
                "{allowed} is on the prose-branching allowance list and no longer branches on \
                 prose. That is good news and the list must shrink to record it: delete the entry \
                 so the next file to acquire one cannot hide behind a stale allowance"
            );
        }
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

    /// No crate under `crates/` may acquire a view dependency.
    ///
    /// The rule is stated in the architecture doc and was enforced by nothing. `src/ui/`'s own
    /// module doc says the bridges "hold no dependency on any UI toolkit, and must not acquire
    /// one" — a sentence, in a file, that no build step reads.
    ///
    /// # Why a manifest rule and not an import rule
    ///
    /// A `use egui::…` in a workspace crate is a symptom; the dependency is the defect, and it
    /// arrives first. More concretely, it arrives *workspace-wide*: `[source.crates-io]
    /// replace-with` resolves the whole workspace before feature selection, so an unvendored
    /// view dependency on **any** member breaks `cargo build --offline` for every crate here,
    /// including ones that never mention it. That is why this reads manifests rather than
    /// imports, and why one crate's mistake is everyone's.
    ///
    /// # The exemption is the root package, and it asserts itself
    ///
    /// Task 11 put the binary on the root package, so the root manifest is the one place a view
    /// dependency belongs. An exemption that merely skips a path is indistinguishable from a
    /// scanner that cannot see it, so this one asserts the exempt manifest **does** hold the
    /// banned keys. If the binary moves, or the arrangement is reverted, this fails loudly
    /// rather than permitting everything under a path that no longer means anything.
    ///
    /// # No witness in the meta-guard, for the reason its neighbour records
    ///
    /// `doc/guard-tests` exempts a guard that reads TOML rather than Rust from the witness rule
    /// and asks for a positive control instead. This carries two, both inside the test where its
    /// reader will look: the walk must find at least six manifests, and the section reader must
    /// find `serde` in `pos-models` before any conclusion is drawn. A meta-guard witness here
    /// would restate the first of those and fail on exactly the mutations the guard already
    /// catches — which is what
    /// `every_excluded_crate_is_named_in_the_verification_script` measured before deleting its
    /// own. A pair that fails on the same mutations is one assertion wearing two hats.
    #[test]
    fn no_workspace_crate_may_acquire_a_view_dependency() {
        const BANNED: [&str; 8] = [
            "abdu-egui-ui",
            "egui",
            "eframe",
            "egui_extras",
            "egui_kittest",
            "winit",
            "wgpu",
            "glow",
        ];

        let manifests = crate_manifests();

        // Guard the guard, part one: the walk found the tree. Every way this can break returns
        // fewer manifests, which reads as a cleaner workspace.
        assert!(
            manifests.len() >= 6,
            "found only {} manifest(s) under `crates/`; the walk is broken and this guard is \
             passing on an empty corpus. Found: {:?}",
            manifests.len(),
            manifests.iter().map(|(p, _)| p).collect::<Vec<_>>()
        );

        // Guard the guard, part two: the *reader* works. A section walker that returns nothing
        // also reports a clean tree, and does so from a full corpus, which the count above
        // cannot detect.
        let models = manifests
            .iter()
            .find(|(path, _)| path.ends_with("pos-models/Cargo.toml"))
            .expect("`crates/pos-models/Cargo.toml` must be in the walk");
        let models_keys: Vec<String> = declared_dependencies(&models.1)
            .into_iter()
            .map(|(_, key)| key)
            .collect();
        assert!(
            models_keys.iter().any(|key| key == "serde"),
            "the section reader did not find `serde` in `pos-models`' dependencies — it is \
             returning nothing and every check below is vacuous. Found: {models_keys:?}"
        );

        let offences: Vec<String> = manifests
            .iter()
            .flat_map(|(path, manifest)| {
                declared_dependencies(manifest)
                    .into_iter()
                    .filter(|(_, key)| BANNED.contains(&key.as_str()))
                    .map(move |(section, key)| format!("{path} — `{key}` in [{section}]"))
            })
            .collect();

        assert!(
            offences.is_empty(),
            "{} view dependenc(ies) inside `crates/`. The view layer lives on the root package; \
             a workspace crate that names a toolkit puts the domain underneath its own renderer, \
             and breaks `--offline` for every other crate here as a side effect:\n  {}",
            offences.len(),
            offences.join("\n  ")
        );

        // The exemption, asserted rather than assumed. This is the positive control for the
        // sweep above: it proves `declared_dependencies` finds a *banned* key when one is really
        // there, which an empty `offences` alone cannot.
        let root = fs::read_to_string(repo_root().join("Cargo.toml"))
            .expect("the root Cargo.toml must be readable");
        let root_view_deps: Vec<String> = declared_dependencies(&root)
            .into_iter()
            .filter(|(_, key)| BANNED.contains(&key.as_str()))
            .map(|(_, key)| key)
            .collect();

        assert!(
            root_view_deps.len() >= 3,
            "the root manifest holds only {} view dependenc(ies) — the exempt entry is supposed \
             to be where they live, so either the binary has moved or this scanner cannot see a \
             banned key at all, and the sweep above proves nothing. Found: {root_view_deps:?}",
            root_view_deps.len()
        );
    }

    /// Every `Cargo.toml` directly under `crates/`, as `(relative path, contents)`.
    ///
    /// Excluded crates are included deliberately: `[workspace] exclude` removes a crate from
    /// `--workspace` commands, not from this repository, and `pos-updater` acquiring a view
    /// dependency would be exactly as wrong and exactly as invisible.
    fn crate_manifests() -> Vec<(String, String)> {
        let crates = repo_root().join("crates");
        assert!(
            crates.is_dir(),
            "{} is not a directory — the scan root has moved and this guard is vacuous",
            crates.display()
        );

        let mut found: Vec<(String, String)> = fs::read_dir(&crates)
            .expect("`crates/` must be readable")
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("Cargo.toml"))
            .filter(|path| path.is_file())
            .map(|path| {
                let text = fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
                (relative(&path), text)
            })
            .collect();
        found.sort();
        found
    }

    /// Every `(section, key)` pair in every dependency table of a manifest.
    ///
    /// Covers `[dependencies]`, `[dev-dependencies]`, `[build-dependencies]`, the
    /// `[target.'cfg(…)'.dependencies]` variants, and the `[dependencies.name]` sub-table form
    /// where the key is in the header rather than on a line. A rule that reads only
    /// `[dependencies]` is one `[target.'cfg(unix)'.dependencies]` away from blind.
    ///
    /// Comments are stripped with [`strip_hash_comment`] — TOML's `#`, not Rust's `//` — so a
    /// dependency named only in prose is not a dependency.
    fn declared_dependencies(manifest: &str) -> Vec<(String, String)> {
        let mut pairs = Vec::new();
        let mut section = String::new();

        for raw in manifest.lines() {
            let line = strip_hash_comment(raw).trim();
            if line.is_empty() {
                continue;
            }

            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = header.trim().to_string();
                // `[dependencies.egui]` — the key is the header's last segment.
                if let Some((table, name)) = section.rsplit_once('.') {
                    if table.ends_with("dependencies") {
                        pairs.push((table.to_string(), name.trim_matches('"').to_string()));
                    }
                }
                continue;
            }

            if !section.ends_with("dependencies") || !line.contains('=') {
                continue;
            }
            let key = line.split('=').next().unwrap_or(line);
            let key = key.split('.').next().unwrap_or(key).trim();
            if !key.is_empty() {
                pairs.push((section.clone(), key.to_string()));
            }
        }

        pairs
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

    /// The retired view toolkit is not named anywhere in the tree.
    ///
    /// The first view layer was a mistake that cost this project two stalled attempts, and Abdu's
    /// standing instruction is that no further effort goes into it — including reading it. It was
    /// deleted as code long before it was gone as a word: 199 mentions survived across `docs/`,
    /// the plan tree, `CLAUDE.md`, `README.md` and this file, because the sweep that removed it
    /// only ever grepped `*.rs` and `*.toml`. Two of those documents were not documents *about* a
    /// toolkit, they were 1,687 lines of documentation *for* it, describing an application that no
    /// longer exists.
    ///
    /// The banned word is assembled at runtime rather than written out. A guard that spells a name
    /// in order to forbid it is the one file that always matches, and the rule here is that the
    /// name does not appear at all.
    #[test]
    fn the_retired_view_toolkit_is_not_named_anywhere_in_the_tree() {
        let banned = ["sl", "int"].concat();

        let files = text_files();
        assert!(
            files.len() > 40,
            "the text walker found only {} files; it is broken and this guard is vacuous",
            files.len()
        );
        assert!(
            files
                .iter()
                .any(|p| p.extension().is_some_and(|e| e == "md")),
            "the walker is not reaching Markdown, which is where every one of these hid last time"
        );

        let offences: Vec<String> = files
            .iter()
            .filter_map(|path| {
                let text = fs::read_to_string(path).ok()?;
                let hits = text
                    .lines()
                    .enumerate()
                    .filter(|(_, line)| line.to_ascii_lowercase().contains(&banned))
                    .map(|(index, _)| (index + 1).to_string())
                    .collect::<Vec<_>>();
                (!hits.is_empty()).then(|| format!("{} (line {})", relative(path), hits.join(", ")))
            })
            .collect();

        assert!(
            offences.is_empty(),
            "the retired view toolkit is named in {} file(s); it is not coming back, so neither should the word:\n  {}",
            offences.len(),
            offences.join("\n  ")
        );
    }

    /// Every text file in the working tree, skipping generated and machine-local trees.
    ///
    /// Broader than [`rust_sources`] on purpose: the mentions this exists to catch were all in
    /// Markdown, and a scan restricted to code is what let them survive a deletion.
    fn text_files() -> Vec<PathBuf> {
        const SKIP: [&str; 9] = [
            ".git",
            ".claude",
            ".superpowers",
            ".worktrees",
            "target",
            "vendor",
            "vendor.new",
            "data",
            "node_modules",
        ];
        const TEXT: [&str; 10] = [
            "rs", "toml", "md", "sh", "py", "sql", "ts", "json", "yml", "yaml",
        ];

        fn walk(dir: &Path, skip: &[&str], text: &[&str], found: &mut Vec<PathBuf>) {
            let Ok(entries) = fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if skip.contains(&name) {
                    continue;
                }
                if path.is_dir() {
                    walk(&path, skip, text, found);
                } else if path
                    .extension()
                    .is_some_and(|e| e.to_str().is_some_and(|e| text.contains(&e)))
                {
                    found.push(path);
                }
            }
        }

        let mut found = Vec::new();
        walk(&repo_root(), &SKIP, &TEXT, &mut found);
        found.sort();
        found
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

    // ========================================================================
    // Every excluded crate is verified by something
    // ========================================================================

    /// The path of this repository's verification entry point, relative to the root.
    const VERIFICATION_SCRIPT: &str = "scripts/verify.sh";

    /// The `exclude` array from the root manifest, and only that array.
    ///
    /// Naive on purpose, like [`dependency_keys`] and [`toml_section_headers`]: it reads this
    /// repository's own manifest, not arbitrary TOML. The array is collected across lines so a
    /// multi-line spelling reads the same as the single-line one it has today — the guard must not
    /// start passing vacuously the day somebody reformats the manifest.
    fn excluded_crates() -> Vec<String> {
        let manifest = fs::read_to_string(repo_root().join("Cargo.toml"))
            .expect("the root Cargo.toml must be readable");

        let mut collected = String::new();
        let mut collecting = false;
        for line in manifest.lines() {
            let code = strip_hash_comment(line);
            if code.trim_start().starts_with("exclude") && code.contains('[') {
                collecting = true;
            }
            if collecting {
                collected.push_str(code);
                if code.contains(']') {
                    break;
                }
            }
        }

        collected
            .split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    }

    /// One line with any `#` comment removed.
    ///
    /// The shell and TOML counterpart of [`strip_comment`], and it carries the same residue: a `#`
    /// inside a quoted string truncates the line early. That can only ever *hide* a mention and so
    /// cause a false failure — it can never invent one, which is the safe direction for a guard
    /// whose success condition is a presence.
    fn strip_hash_comment(line: &str) -> &str {
        match line.find('#') {
            Some(at) => &line[..at],
            None => line,
        }
    }

    /// # Why this carries its own positive control instead of a witness in the meta-guard
    ///
    /// `doc/guard-tests` exempts a guard that reads TOML rather than Rust from the witness rule,
    /// and requires a positive control in its place — the way the build guard proves `[build]` and
    /// `[env]` parse out of the file before concluding no `[source.*]` does. This is that shape:
    /// it reads the manifest and a shell script, never the Rust walker the meta-guard proves.
    ///
    /// A witness *was* written and then deleted, because measuring it showed it was decoration:
    /// emptying the `exclude` array failed the guard and the witness together, and every other
    /// mutation that failed the witness failed the guard too. A pair that fails on the same
    /// mutations is one assertion wearing two hats. The corpus checks below are strictly stronger
    /// and live where the reader of this guard will actually look.
    /// Every crate the workspace excludes is run by the verification script.
    ///
    /// # The failure this exists to prevent, which has already been paid for once
    ///
    /// `[workspace] exclude` means **no workspace command can see the crate** — not
    /// `cargo test --workspace`, not `cargo clippy --workspace`, not `cargo check --workspace`.
    /// `crates/pos-contract` was red from `040d0c1` through **five consecutive task
    /// verifications**, every one of them reporting green, because the command that was run could
    /// not observe the thing being claimed. `CLAUDE.md` carried a written warning about it the
    /// whole time, and prose is measured not to work.
    ///
    /// # Both sides read the tree, so neither can drift
    ///
    /// The list of excluded crates is parsed from the manifest's own `exclude` array rather than
    /// restated here, and the script is read from disk. There is no allowlist to justify per entry
    /// and nothing to keep in sync: add a fourth excluded crate and this fails until somebody
    /// wires it into the script. That is the whole design — it converts "invisible to
    /// `--workspace`" from a standing hazard into a one-time wiring cost.
    ///
    /// A `#`-commented mention does not count. The script's own header explains the pos-contract
    /// history in prose and names the crate while doing so; if a comment satisfied this guard,
    /// deleting the lane while keeping the paragraph about it would pass.
    #[test]
    fn every_excluded_crate_is_named_in_the_verification_script() {
        let excluded = excluded_crates();

        // Guard the guard. A parser that reads nothing reports "every excluded crate is verified"
        // in exactly the words a correctly-wired tree uses, and every way this parser can break
        // returns *fewer* entries.
        assert!(
            excluded.len() >= 2,
            "parsed {} entries from the root manifest's `exclude` array; expected at least the two \
             known ones. The parser is broken and this guard is now vacuous — it would pass an \
             unwired excluded crate",
            excluded.len()
        );
        for known in ["crates/pos-updater", "crates/pos-contract"] {
            assert!(
                excluded.iter().any(|crate_path| crate_path == known),
                "`{known}` is not in the parsed `exclude` array. Either it stopped being excluded \
                 — in which case delete it from this witness — or the parser is reading the wrong \
                 thing"
            );
        }

        let script_path = repo_root().join(VERIFICATION_SCRIPT);
        let script = fs::read_to_string(&script_path).unwrap_or_else(|error| {
            panic!(
                "{VERIFICATION_SCRIPT} must exist and be readable: {error}. It is this repository's \
                 only verification entry point; without it nothing runs the excluded crates at all"
            )
        });

        let code: String = script
            .lines()
            .map(strip_hash_comment)
            .collect::<Vec<_>>()
            .join("\n");

        // The second half of guarding the guard: prove the comment stripper left something
        // executable behind. If it ate the whole file, every assertion below would report a
        // missing lane in the same words a genuinely missing lane produces.
        assert!(
            code.contains("cargo"),
            "no un-commented line of {VERIFICATION_SCRIPT} mentions `cargo`. The comment stripper \
             has eaten the script, so the assertions below are vacuous"
        );

        for crate_path in &excluded {
            assert!(
                code.contains(crate_path.as_str()),
                "`{crate_path}` is excluded from the workspace but is not named in \
                 {VERIFICATION_SCRIPT}, so NO command in this repository runs it. That is how \
                 `crates/pos-contract` stayed red through five consecutive green verifications. \
                 Add a lane for it rather than deleting this assertion"
            );
        }
    }

    // ========================================================================
    // The pact artifact's matcher paths
    // ========================================================================

    /// The artifact the platform replays. Read here, not in `crates/pos-contract`, on purpose:
    /// that crate is excluded from the workspace (`Cargo.toml:35`), so `cargo test --workspace`
    /// cannot see its suite — and the failure this guards is a **hand-edited or merge-polluted
    /// artifact**, whose author is exactly the person who did not run the generator.
    const PACT_ARTIFACT: &str = "crates/pos-contract/pacts/e2manage-pos-terminal-wadi-dms-api.json";

    /// How a matcher path relates to the body declared beside it.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum PathStanding {
        /// Every segment resolved against the declared body.
        Resolves,
        /// The unresolvable tail `[*].*` over an array of **scalars**, which
        /// `EachLike::extract_matching_rules` emits unconditionally — see the test's doc comment.
        LibraryEachLikeOverScalars,
        /// Unresolvable for any other reason. A rule that can never fire.
        Drift,
    }

    /// One `.`-separated step of a matcher path: a key, then any number of index suffixes.
    struct Step<'a> {
        key: &'a str,
        indexes: usize,
    }

    fn parse_path(path: &str) -> Vec<Step<'_>> {
        path.trim_start_matches('$')
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(|segment| {
                let key = segment.split('[').next().unwrap_or(segment);
                Step {
                    key,
                    indexes: segment.matches('[').count(),
                }
            })
            .collect()
    }

    /// Walk a matcher path against a declared body **literally**, without collapsing `[*]` or `*`.
    ///
    /// The collapsing is the whole hazard: normalising `heldBy[*].*` to `heldBy` makes it resolve,
    /// and a checker that does so reports a clean artifact while looking straight at the case it
    /// exists to find.
    fn stand_path(path: &str, body: Option<&serde_json::Value>) -> PathStanding {
        let Some(body) = body else {
            return PathStanding::Drift;
        };
        let steps = parse_path(path);
        let mut node = body;

        for (position, step) in steps.iter().enumerate() {
            // A bare `*` means "any key of this object". It resolves only over an object.
            let descended = if step.key == "*" {
                match node.as_object().and_then(|map| map.values().next()) {
                    Some(child) => child,
                    None => return tail_standing(&steps, position),
                }
            } else {
                match node.get(step.key) {
                    Some(child) => child,
                    None => return tail_standing(&steps, position),
                }
            };

            node = descended;
            for _ in 0..step.indexes {
                match node.as_array().and_then(|items| items.first()) {
                    Some(element) => node = element,
                    None => return tail_standing(&steps, position),
                }
            }
        }

        PathStanding::Resolves
    }

    /// Classify a path that stopped resolving at `position`.
    ///
    /// The one tolerated shape is a final `*` step, reached because the step before it carried an
    /// index into an array whose elements are not objects. Anything else is drift.
    fn tail_standing(steps: &[Step<'_>], position: usize) -> PathStanding {
        let is_final_wildcard = position + 1 == steps.len() && steps[position].key == "*";
        let previous_indexed = position
            .checked_sub(1)
            .is_some_and(|before| steps[before].indexes > 0);

        if is_final_wildcard && previous_indexed {
            PathStanding::LibraryEachLikeOverScalars
        } else {
            PathStanding::Drift
        }
    }

    /// Every `(path, body)` pair the artifact declares, flattened across interactions and parts.
    fn artifact_rule_paths(
        artifact: &serde_json::Value,
    ) -> Vec<(String, String, Option<&serde_json::Value>)> {
        let mut found = Vec::new();
        for interaction in artifact["interactions"].as_array().into_iter().flatten() {
            let description = interaction["description"].as_str().unwrap_or("<unnamed>");
            for part in ["request", "response"] {
                let body = interaction[part].get("body");
                let rules = interaction[part]["matchingRules"].get("body");
                for path in rules.and_then(|r| r.as_object()).into_iter().flatten() {
                    found.push((description.to_string(), path.0.clone(), body));
                }
            }
        }
        found
    }

    /// Every matching rule in the pact points at a key the interaction's own body declares.
    ///
    /// # The failure this exists for
    ///
    /// A matching rule attached to a path the body does not contain **is silence, not an error**.
    /// Measured 2026-08-24 against the real verifier: a rule at `$.error.details.notAField` passes,
    /// exit 0, unmentioned in the output, while the same unsatisfiable rule at a valid path fails
    /// loudly and names the path. So a rule that can never fire is indistinguishable from coverage,
    /// and the pact would report green while pinning nothing.
    ///
    /// Regeneration **merges** on `(description, providerState)`, so an interaction whose body
    /// changes while an old entry survives can leave a rule pointing at a key the new body lost.
    /// That is the mechanism; a hand-edited artifact is the other.
    ///
    /// # Why this is not "no absent paths"
    ///
    /// `EachLike::extract_matching_rules` (`pact_consumer-1.4.10/src/patterns/special_rules.rs:152-158`)
    /// pushes `[*]` then `*` and adds a `Type` rule there **unconditionally** — whether or not the
    /// elements are objects. So every `each_like!` over an array of strings emits a path with a
    /// field wildcard under a scalar, which cannot resolve and never will. pact_consumer's own
    /// documented `each_like!("tag")` example produces it. It is library output, not drift, and a
    /// guard that refused it would fail on a correct artifact.
    ///
    /// # The trap, and why the tolerated case is asserted as a POSITIVE
    ///
    /// The obvious implementation of this check normalises `[*]` and `*` away, which collapses
    /// `$.error.details.heldBy[*].*` to `heldBy` — and `heldBy` resolves. Written that way, the
    /// scan looks straight at the one path in this artifact that does not resolve and reports it
    /// clean. That version was written while reviewing this guard and did exactly that.
    ///
    /// So it is not enough to assert that nothing drifts. This asserts that the
    /// `LibraryEachLikeOverScalars` class is **non-empty** — a normalising walker would classify
    /// that path as `Resolves`, the count would fall to zero, and this test would fail. The
    /// tolerated case is the control for the walker that tolerates it.
    ///
    /// # Guarding the guard
    ///
    /// A corpus assertion, both classes asserted non-empty, and two mutations checked in-process:
    /// a fabricated absent key must read as drift, and a deeper path under the tolerated tail must
    /// **also** read as drift, so the tolerance is exactly that two-step tail and not a prefix rule
    /// anything can hide under.
    #[test]
    fn every_matcher_in_the_pact_points_at_a_key_its_body_declares() {
        let path = repo_root().join(PACT_ARTIFACT);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{PACT_ARTIFACT} could not be read: {e}"));
        let artifact: serde_json::Value =
            serde_json::from_str(&text).expect("the pact artifact is not valid JSON");

        let rules = artifact_rule_paths(&artifact);

        // Guarding the guard: a scan over an empty corpus reports no drift for the same reason a
        // correct artifact does.
        assert!(
            artifact["interactions"]
                .as_array()
                .is_some_and(|i| !i.is_empty()),
            "{PACT_ARTIFACT} declares no interactions, so every assertion below is vacuous"
        );
        assert!(
            rules.len() >= 10,
            "only {} matching rules found in {PACT_ARTIFACT}; the reader has stopped seeing them \
             and this guard is passing over an empty set",
            rules.len()
        );

        let mut resolving = 0usize;
        let mut library = 0usize;
        let mut drifted = Vec::new();
        for (description, rule_path, body) in &rules {
            match stand_path(rule_path, *body) {
                PathStanding::Resolves => resolving += 1,
                PathStanding::LibraryEachLikeOverScalars => library += 1,
                PathStanding::Drift => drifted.push(format!("  {rule_path}  <- {description}")),
            }
        }

        assert!(
            drifted.is_empty(),
            "{} matching rule(s) in {PACT_ARTIFACT} point at a path their own declared body does \
             not contain. Such a rule NEVER FIRES and the verifier reports nothing about it, so \
             the interaction pins less than it appears to:\n{}\n\nIf an interaction was edited: \
             regeneration merges on `(description, providerState)`, so `rm` the artifact and \
             re-run `cargo test` in crates/pos-contract rather than editing the JSON.",
            drifted.len(),
            drifted.join("\n")
        );

        assert!(
            resolving > 0,
            "no matcher path resolved at all, which means the walker is broken rather than the \
             artifact clean"
        );

        // The anti-normalisation control. See this test's doc comment: a walker that collapses
        // `[*]` and `*` classifies the library's own output as `Resolves`, and this count falls to
        // zero while `drifted` stays empty — a clean-looking pass over a blind scan.
        assert!(
            library > 0,
            "no path classified as the library's `[*].*`-over-scalars shape. Either the artifact \
             genuinely contains no `each_like!` over an array of scalars — in which case delete \
             this assertion deliberately — or the walker has started normalising `[*]`/`*` away, \
             which makes the drift check above blind in exactly the case it exists to catch"
        );

        // Mutation 1: a fabricated absent key must read as drift.
        let sample = &rules
            .iter()
            .find(|(_, _, body)| body.is_some())
            .expect("no interaction declares a body")
            .2;
        assert_eq!(
            stand_path("$.error.details.notAField", *sample),
            PathStanding::Drift,
            "a fabricated absent path did not read as drift, so the check above cannot fail"
        );

        // Mutation 2: the tolerance is the two-step tail exactly, not a prefix anything hides under.
        assert_eq!(
            stand_path("$.error.details.heldBy[*].*.deeper", *sample),
            PathStanding::Drift,
            "a path deeper than the tolerated `[*].*` tail was tolerated, so the exemption is a \
             prefix rule rather than the one library shape it is meant to admit"
        );
    }

    // ========================================================================
    // A READ THAT FAILS IS NOT A ROW THAT IS ABSENT
    // ========================================================================

    /// Blanks `//` comments and string literals, preserving every byte offset and newline.
    ///
    /// Length preservation is what lets the caller report a real line number from the blanked
    /// text. Both classes must go: this repo's prose quotes the very combinators banned below, and
    /// SQL literals contain the parentheses the balancer counts.
    /// The text immediately following a balanced `query_row( … )`, or `None` if it never closes.
    fn tail_after_balanced_call(code: &str, open_paren: usize) -> Option<&str> {
        let mut depth = 0usize;
        for (offset, ch) in code[open_paren..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&code[open_paren + offset + 1..]);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// Whether a call's tail hands the error straight to a default.
    ///
    /// `.ok()` counts: it discards the error into an `Option`, which is the same loss wearing a
    /// different type. `.map(…)` is followed through, because `.map(…).unwrap_or(…)` is the same
    /// defect with a transformation in the middle.
    fn discards_its_error(tail: &str) -> bool {
        let trimmed = tail.trim_start();
        for combinator in [".unwrap_or_default", ".unwrap_or_else", ".unwrap_or", ".ok"] {
            if let Some(rest) = trimmed.strip_prefix(combinator) {
                if rest.trim_start().starts_with(['(', '<']) {
                    return true;
                }
            }
        }
        if let Some(rest) = trimmed.strip_prefix(".map") {
            let opened = rest.find('(').map(|at| rest.len() - rest[at..].len());
            if let Some(open) = opened {
                if let Some(after) = tail_after_balanced_call(rest, open) {
                    return discards_its_error(after);
                }
            }
        }
        false
    }

    struct ReadSite {
        path: String,
        line: usize,
        discards: bool,
    }

    /// Every call shape by which `pos-services` gets a row out of the store.
    ///
    /// `query_row` alone was the whole list until `positional-row-access-in-pos-db` moved the
    /// reads onto the projection helpers, at which point the corpus fell to one call and the
    /// non-emptiness control below fired — correctly. **The floor is what caught it.** Lowering
    /// that floor to match would have left a guard that still passed, still read as coverage, and
    /// no longer looked at anything: the population had not become clean, it had moved.
    ///
    /// So the names travel with it. A read is a read whichever helper spells it, and the failure
    /// this guard exists to catch — an error flattened into an answer by `.unwrap_or_default()`,
    /// `.ok()`, `.unwrap_or(false)` — is available at every one of them.
    const READ_CALLS: [&str; 6] = [
        "query_row",
        "read_one",
        "read_all",
        "select_one",
        "optional_scalar",
        "select_optional_scalar",
    ];

    fn query_row_sites_under(relative_dir: &str) -> Vec<ReadSite> {
        let mut sites = Vec::new();
        for file in scanned_files() {
            if !file.path.starts_with(relative_dir) {
                continue;
            }
            let code = file.code;
            let shown = file.path;
            for call in READ_CALLS {
                let mut from = 0;
                while let Some(at) = code[from..].find(call) {
                    let start = from + at;
                    // A whole identifier, not a substring: `read_one` must not also match
                    // `spread_one`, and `select_one` must not match `deselect_one`.
                    let preceded_by_ident = start > 0
                        && code[..start]
                            .chars()
                            .next_back()
                            .is_some_and(|c| c.is_alphanumeric() || c == '_');
                    let after = &code[start + call.len()..];
                    let opened = after.find('(').filter(|at| after[..*at].trim().is_empty());
                    if !preceded_by_ident {
                        if let Some(open) = opened {
                            let absolute = start + call.len() + open;
                            if let Some(tail) = tail_after_balanced_call(&code, absolute) {
                                sites.push(ReadSite {
                                    path: shown.clone(),
                                    line: line_at(&code, start),
                                    discards: discards_its_error(tail),
                                });
                            }
                        }
                    }
                    from = start + call.len();
                }
            }
        }
        sites
    }

    /// No read in `pos-services` turns a store failure into an answer.
    ///
    /// `is_registered` reported every error as "not registered" and `get_hardware_id` reported
    /// every error as "no hardware id yet". Neither could tell `QueryReturnedNoRows` — the
    /// fresh-install case those defaults existed to serve — from a store that could not answer,
    /// and the second one then wrote a freshly generated identity over the one the platform had
    /// enrolled. The rule is a predicate rather than a list of sites, because
    /// `positional-row-access-in-pos-db` is rewriting this population and any list would be stale
    /// within the hour.
    ///
    /// # This guard expects to find nothing, so it proves it can still see
    ///
    /// A scan whose passing result is an empty set cannot distinguish *clean* from *blind*. Both
    /// controls below fire before the assertion that matters: the walker must still find reads,
    /// and the detector must still recognise one.
    #[test]
    fn a_read_in_pos_services_never_flattens_its_error_into_a_default() {
        let sites = query_row_sites_under("crates/pos-services/src");

        assert!(
            sites.len() >= 4,
            "found only {} `query_row` calls in pos-services; the walker is broken and this guard \
             is passing on an empty corpus",
            sites.len()
        );

        assert!(
            discards_its_error(".unwrap_or_default()"),
            "the detector no longer recognises a discarded error, so its silence means nothing"
        );
        assert!(
            discards_its_error(" .map(|row| row.get(0)).unwrap_or(0)"),
            "the detector stopped following `.map(…)`, so the defect hides behind a transformation"
        );
        assert!(
            !discards_its_error("?;"),
            "the detector flags a propagating read, so every site would look like a violation"
        );

        let offenders: Vec<String> = sites
            .iter()
            .filter(|site| site.discards)
            .map(|site| format!("{}:{}", site.path, site.line))
            .collect();

        assert!(
            offenders.is_empty(),
            "these reads hand their error to a default, so a store that cannot answer is \
             indistinguishable from a row that is absent:\n  {}\nMatch on the error instead: \
             `Err(QueryReturnedNoRows)` is the absent case, and everything else belongs to the \
             caller.",
            offenders.join("\n  ")
        );
    }

    /// Every column of `terminal_registration` is either cleared by `clear_registration` or
    /// exempt on the record.
    ///
    /// # Why this exists
    ///
    /// `clear_registration` is the operation that severs this terminal from a company. It named
    /// five of the seven columns that describe an enrolment and left two standing: `company_name`
    /// since schema V3 and `license_key` since V8. **Five schema versions apart, one mechanism** —
    /// a hand-maintained SQL column list that nobody revisits when a column is added — and nothing
    /// caught either, because `clear_tenant_data` empties nineteen tables first and the appearance
    /// was a thorough wipe. `license_key` was a live cross-tenant credential leak.
    ///
    /// Fixing both instances does not stop the third. This does.
    ///
    /// # The column set is read from the schema, never restated here
    ///
    /// A guard that carries its own copy of a column list has the defect it is guarding against:
    /// it would go stale the same way and by the same mechanism. Both the `CREATE TABLE` and every
    /// `ALTER TABLE … ADD COLUMN` are parsed, because `license_key` arrived by the second route and
    /// a guard reading only the first would have been blind to the very column that motivated it.
    ///
    /// # The contract with the sum-type work, stated here rather than in a plan
    ///
    /// If `clear_registration`'s SQL is ever relocated — into a row-lifecycle type, or into
    /// `pos-db` — **this guard turns red rather than silent**, because control 1 below asserts the
    /// extraction still finds a `SET` list. That is deliberate. A guard whose passing result is an
    /// empty set cannot tell *clean* from *blind*, and this one would otherwise pass forever the
    /// moment the statement it inspects moves. Whoever relocates that SQL owns updating this.
    ///
    /// # What the extractor matches, learned from a mutation that did not fire
    ///
    /// It locates the function by the **prefix** `pub fn clear_registration`. The first attempt to
    /// mutate toward the blind spot renamed it to `clear_registration_RENAMED` — which still
    /// contains that prefix, so the guard passed and the probe proved nothing. **A probe that does
    /// not fire is a claim about the probe before it is a claim about the guard.** Renaming to a
    /// name sharing no prefix turns it red, as intended.
    ///
    /// The residual property, stated so nobody rediscovers it: a rename that *extends* the name is
    /// followed silently, and a second function whose name starts with the same prefix, declared
    /// earlier in the file, would be read instead. Neither is true today.
    ///
    /// `tests/guards.rs` lives in the **root package**, so `cargo test -p pos-services` does not
    /// build it. Run `cargo test -p e2manage-pos-terminal --test guards`.
    #[test]
    fn clear_registration_accounts_for_every_column_terminal_registration_has() {
        /// Columns deliberately NOT cleared, each with the reason it is exempt.
        ///
        /// Adding a column here is a decision on the record. Adding one to satisfy a red guard
        /// without a reason is the defect wearing the guard's clothes.
        const EXEMPT: [(&str, &str); 2] = [
            (
                "id",
                "the singleton discriminator — it appears in the WHERE, and clearing it would \
                 destroy the row rather than the enrolment",
            ),
            (
                "hardware_id",
                "identifies the DEVICE, not the enrolment; it is NOT NULL and must survive so the \
                 platform sees a known device re-enrolling rather than a new one",
            ),
        ];

        let schema = fs::read_to_string(repo_root().join("crates/pos-db/src/schema.rs"))
            .expect("cannot read crates/pos-db/src/schema.rs");

        // Columns from `CREATE TABLE … terminal_registration ( … )`.
        let mut columns: Vec<String> = Vec::new();
        if let Some(start) = schema.find("CREATE TABLE IF NOT EXISTS terminal_registration (") {
            let body = &schema[start..];
            let end = body.find(");").expect("the CREATE TABLE is not terminated");
            for line in body[..end].lines().skip(1) {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with("--") {
                    continue;
                }
                if let Some(name) = trimmed.split_whitespace().next() {
                    columns.push(name.trim_matches(',').to_string());
                }
            }
        }

        // …plus every column added later by migration. `license_key` arrives this way.
        for line in schema.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("ALTER TABLE terminal_registration ADD COLUMN")
            {
                if let Some(name) = rest.split_whitespace().next() {
                    columns.push(name.to_string());
                }
            }
        }

        // The `SET` list of `clear_registration`, read from the function itself.
        let service =
            fs::read_to_string(repo_root().join("crates/pos-services/src/pairing_service.rs"))
                .expect("cannot read crates/pos-services/src/pairing_service.rs");
        let cleared: Vec<String> = match service.find("pub fn clear_registration") {
            Some(at) => {
                let body = &service[at..];
                let update = body
                    .find("UPDATE terminal_registration")
                    .expect("clear_registration issues no UPDATE on terminal_registration");
                let where_at = body[update..]
                    .find("WHERE")
                    .expect("the UPDATE in clear_registration has no WHERE clause");
                body[update..update + where_at]
                    .lines()
                    .filter_map(|line| {
                        let trimmed = line.trim().trim_start_matches("SET ").trim();
                        trimmed
                            .split_once(" = ")
                            .map(|(name, _)| name.trim().to_string())
                    })
                    .collect()
            }
            None => Vec::new(),
        };

        // --- Control 1: the extraction can still see. -----------------------------------------
        // An empty result here is indistinguishable from a clean tree, so the guard proves it
        // found something before it asserts it found nothing wrong.
        assert!(
            !columns.is_empty(),
            "no columns were extracted for terminal_registration — the schema moved or its shape \
             changed, and this guard is now blind rather than clean"
        );
        assert!(
            !cleared.is_empty(),
            "no SET list was extracted from clear_registration — the statement was renamed or \
             relocated, and this guard would otherwise pass forever while checking nothing. \
             Whoever moved it owns updating this guard"
        );

        // --- Control 2: the detector fires on a known positive. -------------------------------
        // Two halves, and the canary is deliberately NOT `license_key`. That column is the one a
        // regression is most likely to drop, and using it here would make a real omission report
        // "the extractor is broken" instead of "you dropped a column" — a control that mislabels
        // the defect it was built to expose.
        //
        // (a) everything extracted is a real column of this table. A parser grabbing text from the
        //     wrong region returns names the schema does not have, and this catches that without
        //     depending on any single column surviving.
        let strays: Vec<&String> = cleared.iter().filter(|c| !columns.contains(c)).collect();
        assert!(
            strays.is_empty(),
            "the SET-list extractor produced {strays:?}, which are not columns of \
             terminal_registration — it is reading the wrong region, not the SET list"
        );

        // (b) the column whose clearing IS the definition of de-registration is present. If this
        //     is ever absent the till has a far worse problem than a stale guard.
        assert!(
            cleared.iter().any(|c| c == "secret"),
            "the extractor did not find `secret` in clear_registration's SET list; either the \
             extractor is broken or de-registration has stopped clearing the terminal secret"
        );

        // --- Control 3: the detector does NOT fire on a known negative. ------------------------
        // Without this, a detector broken OPEN passes controls 1 and 2 and then reports every
        // column as cleared — which reads as a clean run rather than as a broken instrument.
        assert!(
            !cleared.iter().any(|c| c == "hardware_id"),
            "the extractor claims clear_registration clears `hardware_id`, which it must not — the \
             extractor is matching more than the SET list"
        );

        // --- The assertion this guard is named for. -------------------------------------------
        let unaccounted: Vec<&String> = columns
            .iter()
            .filter(|c| {
                !cleared.iter().any(|x| &x == c)
                    && !EXEMPT.iter().any(|(name, _)| *name == c.as_str())
            })
            .collect();

        assert!(
            unaccounted.is_empty(),
            "terminal_registration has {} column(s) that clear_registration neither clears nor \
             exempts: {:?}.\n\nThis is how `company_name` (schema V3) and `license_key` (V8) each \
             survived a de-registration whose whole purpose is severing the terminal from a \
             company. Either add the column to the SET list, or add it to EXEMPT with the reason \
             it must survive.",
            unaccounted.len(),
            unaccounted
        );
    }
}
