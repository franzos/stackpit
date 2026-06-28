//! Dialect-agnostic migration-tree equivalence checks.
//!
//! Pure file parsing, no database connection, so it runs under a bare
//! `cargo test` regardless of which backend feature is enabled. Catches the two
//! ways the sqlite and postgres trees drift apart:
//!
//! 1. a migration added to one tree but not the other (filename/number parity), and
//! 2. a table defined in one tree but not the other (final live-table-set parity).
//!
//! Per-column comparison is intentionally out of scope: column types differ by
//! design across dialects (e.g. `BLOB` vs `BYTEA`, `INTEGER` vs `BIGINT`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn migrations_dir(dialect: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("migrations")
        .join(dialect)
}

/// Migration filenames (e.g. `001_initial.sql`) in a tree, sorted.
fn migration_files(dialect: &str) -> BTreeSet<String> {
    std::fs::read_dir(migrations_dir(dialect))
        .expect("read migrations dir")
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".sql"))
        .collect()
}

/// Strip quoting, an attached opening paren, and a trailing semicolon from a
/// table-name token, then lowercase it.
fn clean(token: &str) -> String {
    token
        .trim_matches(|c| c == '"' || c == '`' || c == '(' || c == ';')
        .to_ascii_lowercase()
}

/// Replay `CREATE TABLE` / `DROP TABLE` / `ALTER TABLE ... RENAME TO` across a
/// tree's migrations (in filename order) to get the set of tables that remain
/// live at the end. This collapses the sqlite table-rebuild idiom (create temp,
/// drop original, rename temp back) so transient tables don't show up as drift.
fn live_tables(dialect: &str) -> BTreeSet<String> {
    let mut files: Vec<String> = migration_files(dialect).into_iter().collect();
    files.sort();

    let mut live = BTreeSet::new();
    for file in files {
        let sql = std::fs::read_to_string(migrations_dir(dialect).join(&file))
            .expect("read migration file");
        let toks: Vec<&str> = sql.split_whitespace().collect();
        let up: Vec<String> = toks.iter().map(|t| t.to_ascii_uppercase()).collect();

        let mut i = 0;
        while i < toks.len() {
            if up[i] == "CREATE" && i + 1 < toks.len() && up[i + 1] == "TABLE" {
                let mut j = i + 2;
                if j + 2 < up.len() && up[j] == "IF" && up[j + 1] == "NOT" && up[j + 2] == "EXISTS"
                {
                    j += 3;
                }
                if j < toks.len() {
                    live.insert(clean(toks[j]));
                }
                i = j + 1;
            } else if up[i] == "DROP" && i + 1 < toks.len() && up[i + 1] == "TABLE" {
                let mut j = i + 2;
                if j + 1 < up.len() && up[j] == "IF" && up[j + 1] == "EXISTS" {
                    j += 2;
                }
                if j < toks.len() {
                    live.remove(&clean(toks[j]));
                }
                i = j + 1;
            } else if up[i] == "ALTER"
                && i + 5 < toks.len()
                && up[i + 1] == "TABLE"
                && up[i + 3] == "RENAME"
                && up[i + 4] == "TO"
            {
                live.remove(&clean(toks[i + 2]));
                live.insert(clean(toks[i + 5]));
                i += 6;
            } else {
                i += 1;
            }
        }
    }
    live
}

#[test]
fn migration_filenames_match_across_dialects() {
    let sqlite = migration_files("sqlite");
    let postgres = migration_files("postgres");

    assert!(!sqlite.is_empty(), "no sqlite migrations found");
    assert_eq!(
        sqlite,
        postgres,
        "migration filename sets differ between trees; \
         a migration was added to one dialect but not the other.\n\
         only in sqlite:   {:?}\n\
         only in postgres: {:?}",
        sqlite.difference(&postgres).collect::<Vec<_>>(),
        postgres.difference(&sqlite).collect::<Vec<_>>(),
    );
}

#[test]
fn table_sets_match_across_dialects() {
    let sqlite = live_tables("sqlite");
    let postgres = live_tables("postgres");

    // Sanity floor so a parser regression that returns an empty set can't pass.
    assert!(
        sqlite.len() >= 20,
        "expected a non-trivial table set, parsed only {}: {:?}",
        sqlite.len(),
        sqlite
    );
    for core in ["events", "issues", "projects", "spans"] {
        assert!(
            sqlite.contains(core),
            "sqlite tree is missing core table `{core}`"
        );
    }

    assert_eq!(
        sqlite,
        postgres,
        "final table sets differ between trees.\n\
         only in sqlite:   {:?}\n\
         only in postgres: {:?}",
        sqlite.difference(&postgres).collect::<Vec<_>>(),
        postgres.difference(&sqlite).collect::<Vec<_>>(),
    );
}
