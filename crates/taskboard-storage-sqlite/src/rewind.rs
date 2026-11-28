// SPDX-License-Identifier: MIT OR Apache-2.0
//! #![doc(hidden)] migration-rewind test support (#877).
//!
//! Migration-upgrade fixtures simulate an old database by running the
//! shipped migrations, stripping the effects of every migration above a
//! target version, and deleting those `_sqlx_migrations` rows so the
//! next [`crate::open`] replays the real upgrade path. Before this
//! module every fixture hand-wrote its strip block — each new migration
//! forced edits to every fixture rewinding past it (the #877 failure
//! mode: the collision surfaces only as a full-suite failure, far from
//! the migration that caused it).
//!
//! [`rewind_to`] derives the mechanical strips **from the migration
//! ledger itself** (`MIGRATOR.iter()`): every `CREATE TABLE` becomes a
//! `DROP TABLE IF EXISTS`, every `ALTER TABLE … ADD COLUMN` becomes an
//! `ALTER TABLE … DROP COLUMN`, applied newest-first. Effects the
//! grammar cannot invert (data backfills, the 0006 rename rebuild) are
//! the caller's [`ExtraStrip`]s. Adding a migration therefore needs
//! **zero fixture edits**: its objects land in the derived strips
//! automatically, and the `rewind_table_covers_the_ledger` canary
//! fails loudly if a statement shape falls outside the grammar.
//!
//! Corollary law: a new migration must keep its schema-touching
//! statements in the recognized shapes (`CREATE TABLE [IF NOT EXISTS]
//! <name>`, `ALTER TABLE <t> ADD [COLUMN] <c>`) or teach the grammar in
//! the same commit — the canary makes any violation a `nextest`
//! failure, not a distant fixture collision.

use sqlx::SqlitePool;
use thiserror::Error;

/// The same embedded ledger the runtime opens with; test-only, so the
/// duplicate `migrate!` invocation costs nothing at runtime.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// One non-derivable rewind step: a SQL statement the ledger grammar
/// cannot invert (a data backfill, a table rename rebuild), supplied by
/// the fixture that rewinds past the migration carrying it.
#[derive(Debug)]
pub struct ExtraStrip {
    /// The statement, executed verbatim after the derived strips.
    pub sql: &'static str,
    /// Why the step exists (which migration's effect it inverts).
    pub why: &'static str,
}

/// The 0006 rename rebuild (`CREATE TABLE sync_status_new … RENAME TO
/// sync_status`) leaves a per-board-shaped table under the 0001
/// singleton's name; no derived strip can see the rename. Any rewind to
/// a target below 0006 appends these steps to restore the legacy
/// singleton so the replayed 0006 backfill (`… JOIN sync_status ON
/// s.id = 1`) finds the shape it was written against.
pub const LEGACY_SYNC_STATUS_SINGLETON: &[ExtraStrip] = &[
    ExtraStrip {
        sql: "DROP TABLE IF EXISTS sync_status",
        why: "0006 renamed the rebuilt per-board table over the singleton's name",
    },
    ExtraStrip {
        sql: "CREATE TABLE sync_status (\
              id INTEGER PRIMARY KEY CHECK (id = 1), \
              phase TEXT NOT NULL, \
              last_error TEXT NULL, \
              last_success TEXT NULL\
              )",
        why: "the 0001 legacy singleton shape",
    },
    ExtraStrip {
        sql: "INSERT INTO sync_status (id, phase) VALUES (1, 'idle')",
        why: "the 0001 seed row",
    },
];

/// Everything [`rewind_to`] can fail with. Derived and extra strips and
/// the ledger delete all run on the caller's pool; a failure leaves the
/// fixture database unusable by construction (the caller's `expect` is
/// the intended handling).
#[derive(Debug, Error)]
pub enum RewindError {
    /// A strip or the ledger delete failed against the database.
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
}

/// One classified statement of the ledger grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Parsed {
    /// `CREATE TABLE [IF NOT EXISTS] <name> …` — inverted by a
    /// `DROP TABLE IF EXISTS`.
    CreateTable { table: String },
    /// `ALTER TABLE <t> ADD [COLUMN] <c> …` — inverted by an
    /// `ALTER TABLE <t> DROP COLUMN`.
    AddColumn { table: String, column: String },
    /// A statement the grammar recognizes but cannot invert (data
    /// backfills, renames): skipped by the derivation, reported by the
    /// ledger canary so rewinding fixtures know to supply an
    /// [`ExtraStrip`].
    Unstrippable,
}

/// Strip line comments (no migration string literal contains `--`, the
/// grammar's documented conservatism), split on `;`, drop empties.
fn statements(sql: &str) -> Vec<String> {
    let comment_free: String = sql
        .lines()
        .map(|line| match line.find("--") {
            Some(idx) => &line[..idx],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n");
    comment_free
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Classify one statement. Anything outside the two invertible shapes
/// and the known benign keywords is `None` — the canary's loud failure,
/// never a silent guess.
fn parse_statement(statement: &str) -> Option<Parsed> {
    let tokens: Vec<&str> = statement.split_whitespace().collect();
    let upper: Vec<String> = tokens.iter().map(|t| t.to_ascii_uppercase()).collect();
    let head = |names: &[&str]| {
        upper.len() >= names.len() && upper.iter().zip(names).all(|(token, name)| token == name)
    };
    if head(&["CREATE", "TABLE"]) {
        let name = upper
            .iter()
            .skip(2)
            .position(|t| t != "IF" && t != "NOT" && t != "EXISTS")
            .map(|offset| unquote(tokens[offset + 2]));
        return name.map(|table| Parsed::CreateTable { table });
    }
    if head(&["ALTER", "TABLE"]) && upper.get(3).map(String::as_str) == Some("ADD") {
        let column_index = if upper.get(4).map(String::as_str) == Some("COLUMN") {
            5
        } else {
            4
        };
        return upper.get(column_index).map(|_| Parsed::AddColumn {
            table: unquote(tokens[2]),
            column: unquote(tokens[column_index]),
        });
    }
    if head(&["ALTER", "TABLE"]) && upper.iter().any(|t| t == "RENAME") {
        return Some(Parsed::Unstrippable);
    }
    if head(&["CREATE", "INDEX"])
        || head(&["UPDATE"])
        || head(&["INSERT"])
        || head(&["DELETE"])
        || head(&["DROP"])
    {
        return Some(Parsed::Unstrippable);
    }
    None
}

/// The ledger spells identifiers bare (project convention: no quoted or
/// bracketed identifiers); a quoted name arriving here is a grammar
/// violation the canary surfaces.
fn unquote(name: &str) -> String {
    name.trim_matches(|c| c == '"' || c == '`' || c == '[' || c == ']')
        .to_owned()
}

/// The derived strip statements for every migration above `target`,
/// newest first. Recognized-but-unstrippable statements are skipped —
/// their fixtures carry [`ExtraStrip`]s.
fn mechanical_strips(target: i64) -> Vec<String> {
    let mut strips = Vec::new();
    for migration in MIGRATOR.iter().filter(|m| m.version > target).rev() {
        for statement in statements(migration.sql.as_ref()) {
            match parse_statement(&statement) {
                Some(Parsed::CreateTable { table }) => {
                    strips.push(format!("DROP TABLE IF EXISTS {table}"));
                }
                Some(Parsed::AddColumn { table, column }) => {
                    strips.push(format!("ALTER TABLE {table} DROP COLUMN {column}"));
                }
                Some(Parsed::Unstrippable) | None => {}
            }
        }
    }
    strips
}

/// Rewinds the pool's database to the state a `target`-era database
/// would have been in: every migration above `target` is stripped
/// (derived from the ledger, applied newest-first) and its
/// `_sqlx_migrations` row deleted, so the next `MIGRATOR.run` replays
/// everything above `target` in order against the stripped schema.
/// Non-derivable effects ride `extra`, executed after the derived
/// strips, in slice order.
pub async fn rewind_to(
    pool: &SqlitePool,
    target: i64,
    extra: &[ExtraStrip],
) -> Result<(), RewindError> {
    for statement in mechanical_strips(target) {
        // Audited: every statement is derived from the crate's own
        // embedded migration ledger — a `DROP TABLE IF EXISTS` or an
        // `ALTER TABLE … DROP COLUMN` over a migration-owned object;
        // no user data in reach.
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(pool)
            .await?;
    }
    for step in extra {
        sqlx::query(sqlx::AssertSqlSafe(step.sql))
            .execute(pool)
            .await?;
    }
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version > ?")
        .bind(target)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The grammar classifies the invertible shapes (both spellings of
    /// `ADD COLUMN`, the `IF NOT EXISTS` table form, comment clutter).
    #[test]
    fn parses_the_invertible_shapes() {
        assert_eq!(
            parse_statement("CREATE TABLE boards (id TEXT PRIMARY KEY NOT NULL)"),
            Some(Parsed::CreateTable {
                table: "boards".to_owned()
            })
        );
        assert_eq!(
            parse_statement("CREATE TABLE IF NOT EXISTS habits (habit_id TEXT NOT NULL)"),
            Some(Parsed::CreateTable {
                table: "habits".to_owned()
            })
        );
        assert_eq!(
            parse_statement("ALTER TABLE boards ADD COLUMN ck_title TEXT NOT NULL DEFAULT 'x'"),
            Some(Parsed::AddColumn {
                table: "boards".to_owned(),
                column: "ck_title".to_owned()
            })
        );
        assert_eq!(
            parse_statement("ALTER TABLE task_labels\n    ADD COLUMN ck_membership TEXT NOT NULL"),
            Some(Parsed::AddColumn {
                table: "task_labels".to_owned(),
                column: "ck_membership".to_owned()
            })
        );
    }

    /// The known benign shapes classify unstrippable, never
    /// unrecognized: renames, data backfills, plain drops.
    #[test]
    fn classifies_unstrippable_shapes() {
        assert_eq!(
            parse_statement("ALTER TABLE sync_status_new RENAME TO sync_status"),
            Some(Parsed::Unstrippable)
        );
        assert_eq!(
            parse_statement(
                "UPDATE outbox SET op_kind = 'update_board' WHERE op_kind = 'rename_board'"
            ),
            Some(Parsed::Unstrippable)
        );
        assert_eq!(
            parse_statement("INSERT INTO sync_status_new (board) SELECT id FROM boards"),
            Some(Parsed::Unstrippable)
        );
        assert_eq!(
            parse_statement("DROP TABLE sync_status"),
            Some(Parsed::Unstrippable)
        );
    }

    /// Statements outside the grammar are `None` — the canary's loud
    /// signal, never a silent guess.
    #[test]
    fn foreign_shapes_do_not_parse() {
        assert_eq!(parse_statement("VACUUM"), None);
        assert_eq!(parse_statement("PRAGMA journal_mode = wal"), None);
    }

    /// Comments and statement splits behave: a commented-out statement
    /// is not a statement.
    #[test]
    fn comments_are_stripped_before_splitting() {
        assert_eq!(statements("-- only a comment\n"), Vec::<String>::new());
        assert_eq!(
            statements(
                "ALTER TABLE t ADD COLUMN a TEXT; -- trailing\nALTER TABLE t ADD COLUMN b TEXT;"
            ),
            vec![
                "ALTER TABLE t ADD COLUMN a TEXT".to_owned(),
                "ALTER TABLE t ADD COLUMN b TEXT".to_owned(),
            ]
        );
    }

    /// The standing canary (D-877-d): every statement of every ledger
    /// migration must classify — an unclassifiable shape fails here,
    /// at the migration that introduced it, instead of surfacing as a
    /// distant fixture collision at full-suite time (the #877 failure
    /// mode). A migration carrying unstrippable statements is legal but
    /// must be one a rewinding fixture can name an [`ExtraStrip`] for.
    #[test]
    fn rewind_table_covers_the_ledger() {
        let mut unstrippable: Vec<(i64, usize)> = Vec::new();
        for migration in MIGRATOR.iter() {
            let ledger_statements = statements(migration.sql.as_ref());
            let unclassified: Vec<&str> = ledger_statements
                .iter()
                .map(String::as_str)
                .filter(|s| parse_statement(s).is_none())
                .collect();
            assert!(
                unclassified.is_empty(),
                "migration {:06}_{} carries statements outside the rewind grammar \
                 ({}): teach the grammar in this commit or list the fixture \
                 ExtraStrip it needs",
                migration.version,
                migration.description,
                unclassified.join(" ; "),
            );
            let count = ledger_statements
                .iter()
                .filter(|s| parse_statement(s) == Some(Parsed::Unstrippable))
                .count();
            if count > 0 {
                unstrippable.push((migration.version, count));
            }
        }
        // The known unstrippable carriers, pinned so a grammar widening
        // that silently stops flagging them is visible: 0001's seed
        // row, 0003/0005/0009/0021's data backfills, 0016's head row,
        // 0006's rename dance, 0032's habit-event index (the stage-1
        // plan's D-RW — a planned, cited edit; the CREATE INDEX
        // classifies Unstrippable and rewinding fixtures need no
        // ExtraStrip for it, the derived DROP TABLEs take the index
        // with its table).
        assert_eq!(
            unstrippable,
            vec![
                (1, 2),
                (3, 1),
                (5, 1),
                (6, 3),
                (9, 1),
                (16, 1),
                (21, 1),
                (32, 1)
            ],
            "the ledger's unstrippable-statement carriers changed; every \
             fixture rewinding past these versions needs its ExtraStrip \
             list re-checked"
        );
    }

    /// The derived-strip cascade for a pre-0015 target pin: 0015's
    /// table, 0018's kind column on it, and 0021's board column on the
    /// 0018 table all derive from the ledger, newest first.
    #[test]
    fn derivation_produces_the_0015_0018_0021_cascade() {
        let strips = mechanical_strips(14);
        let find = |needle: &str| {
            strips
                .iter()
                .position(|s| s == needle)
                .unwrap_or_else(|| panic!("{needle} missing from the derived strips"))
        };
        let table = find("DROP TABLE IF EXISTS time_entries");
        let kind = find("ALTER TABLE time_entries DROP COLUMN kind");
        let board = find("ALTER TABLE break_rules DROP COLUMN board_id");
        assert!(
            table > kind && kind > board,
            "newest-first ordering violated: table@{table}, kind@{kind}, board@{board}"
        );
    }
}
