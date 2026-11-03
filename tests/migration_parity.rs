//! Dialect-agnostic migration-tree equivalence checks.
//!
//! Pure file parsing, no database connection, so it runs under a bare
//! `cargo test` regardless of which backend feature is enabled. Catches the two
//! ways the sqlite and postgres trees drift apart:
//!
//! 1. a migration added to one tree but not the other (filename/number parity),
//! 2. a table defined in one tree but not the other (final live-table-set parity), and
//! 3. a named index defined in one tree but not the other, or on a different table.
//!
//! Per-column comparison is intentionally out of scope: column types differ by
//! design across dialects (e.g. `BLOB` vs `BYTEA`, `INTEGER` vs `BIGINT`).

use std::collections::{BTreeMap, BTreeSet};
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
    replay(dialect).0
}

/// Live named indexes at the end of a tree, mapped to their table.
fn live_indexes(dialect: &str) -> BTreeMap<String, String> {
    replay(dialect).1
}

/// Strip `--` line comments so commented-out SQL can't register.
fn strip_comments(sql: &str) -> String {
    sql.lines()
        .map(|l| l.split_once("--").map_or(l, |(code, _)| code))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Drop the leading `IF [NOT] EXISTS` at `j`, returning the index past it.
fn skip_if_exists(up: &[String], mut j: usize) -> usize {
    if up.get(j).is_some_and(|t| t == "IF") {
        j += 1;
        if up.get(j).is_some_and(|t| t == "NOT") {
            j += 1;
        }
        if up.get(j).is_some_and(|t| t == "EXISTS") {
            j += 1;
        }
    }
    j
}

/// Replay table and index DDL across a tree. Unnamed indexes (Postgres
/// `CREATE UNIQUE INDEX ON t (...)`) stand in for SQLite inline constraints and
/// are skipped, like the constraint-backed `sqlite_autoindex_*` / `*_pkey`.
fn replay(dialect: &str) -> (BTreeSet<String>, BTreeMap<String, String>) {
    let mut files: Vec<String> = migration_files(dialect).into_iter().collect();
    files.sort();

    let mut live = BTreeSet::new();
    let mut indexes: BTreeMap<String, String> = BTreeMap::new();
    for file in files {
        let sql = strip_comments(
            &std::fs::read_to_string(migrations_dir(dialect).join(&file))
                .expect("read migration file"),
        );
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
                    let table = clean(toks[j]);
                    indexes.retain(|_, t| *t != table);
                    live.remove(&table);
                }
                i = j + 1;
            } else if up[i] == "CREATE"
                && (up.get(i + 1).is_some_and(|t| t == "INDEX")
                    || (up.get(i + 1).is_some_and(|t| t == "UNIQUE")
                        && up.get(i + 2).is_some_and(|t| t == "INDEX")))
            {
                let start = if up[i + 1] == "INDEX" { i + 2 } else { i + 3 };
                let j = skip_if_exists(&up, start);
                if up.get(j).is_some_and(|t| t != "ON") && up.get(j + 1).is_some_and(|t| t == "ON")
                {
                    if let Some(table) = toks.get(j + 2) {
                        let table = clean(table.split('(').next().unwrap_or(table));
                        indexes.insert(clean(toks[j]), table);
                    }
                }
                i = j + 1;
            } else if up[i] == "DROP" && up.get(i + 1).is_some_and(|t| t == "INDEX") {
                let j = skip_if_exists(&up, i + 2);
                if let Some(name) = toks.get(j) {
                    indexes.remove(&clean(name));
                }
                i = j + 1;
            } else if up[i] == "ALTER"
                && i + 5 < toks.len()
                && up[i + 1] == "TABLE"
                && up[i + 3] == "RENAME"
                && up[i + 4] == "TO"
            {
                let (from, to) = (clean(toks[i + 2]), clean(toks[i + 5]));
                for table in indexes.values_mut() {
                    if *table == from {
                        table.clone_from(&to);
                    }
                }
                live.remove(&from);
                live.insert(to);
                i += 6;
            } else {
                i += 1;
            }
        }
    }
    (live, indexes)
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

#[test]
fn index_sets_match_across_dialects() {
    let sqlite = live_indexes("sqlite");
    let postgres = live_indexes("postgres");

    assert!(
        sqlite.len() >= 40,
        "expected a non-trivial index set, parsed only {}: {:?}",
        sqlite.len(),
        sqlite
    );
    for (index, table) in [
        ("idx_events_trace_list", "events"),
        ("idx_spans_root", "spans"),
    ] {
        assert_eq!(
            sqlite.get(index).map(String::as_str),
            Some(table),
            "sqlite tree is missing `{index}` on `{table}`"
        );
    }

    let differ = |a: &BTreeMap<String, String>, b: &BTreeMap<String, String>| {
        a.iter()
            .filter(|(k, v)| b.get(*k) != Some(*v))
            .map(|(k, v)| format!("{v}.{k}"))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        sqlite,
        postgres,
        "final index sets differ between trees.\n\
         only in sqlite:   {:?}\n\
         only in postgres: {:?}",
        differ(&sqlite, &postgres),
        differ(&postgres, &sqlite),
    );
}
