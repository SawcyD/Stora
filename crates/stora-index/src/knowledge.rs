use rusqlite::params;
use stora_core::Result;

use crate::migrations::map_err;
use crate::Index;

/// One curated entry, in the shape `stora-knowledge` supplies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeRow {
    pub id: String,
    pub pattern: String,
    pub title: String,
    pub written_by: String,
    pub if_removed: String,
    pub removable: bool,
    pub source_title: String,
    pub source_url: String,
}

impl Index {
    /// Replaces the seeded knowledge rows with the current curated set.
    ///
    /// Run at startup. Seeded rows are wholly owned by the shipped file, so
    /// they are cleared first rather than merged — that way an entry removed
    /// from the JSON also disappears here.
    pub fn seed_knowledge(&self, rows: &[KnowledgeRow]) -> Result<usize> {
        self.with(|connection| {
            let tx = connection.transaction().map_err(map_err)?;
            tx.execute("DELETE FROM knowledge_entries WHERE seeded = 1", [])
                .map_err(map_err)?;

            {
                let mut statement = tx
                    .prepare_cached(
                        "INSERT INTO knowledge_entries (id, pattern, title, written_by, \
                         if_removed, removable, source_title, source_url, seeded) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1) \
                         ON CONFLICT (id) DO UPDATE SET \
                           pattern = excluded.pattern, \
                           title = excluded.title, \
                           written_by = excluded.written_by, \
                           if_removed = excluded.if_removed, \
                           removable = excluded.removable, \
                           source_title = excluded.source_title, \
                           source_url = excluded.source_url",
                    )
                    .map_err(map_err)?;

                for row in rows {
                    statement
                        .execute(params![
                            row.id,
                            row.pattern,
                            row.title,
                            row.written_by,
                            row.if_removed,
                            row.removable as i32,
                            row.source_title,
                            row.source_url,
                        ])
                        .map_err(map_err)?;
                }
            }

            tx.commit().map_err(map_err)?;
            Ok(rows.len())
        })
    }

    pub fn knowledge_entry_count(&self) -> Result<u64> {
        self.with(|connection| {
            let count: i64 = connection
                .query_row("SELECT count(*) FROM knowledge_entries", [], |row| {
                    row.get(0)
                })
                .map_err(map_err)?;
            Ok(count as u64)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Index {
        Index::open_in_memory().unwrap()
    }

    fn row(id: &str) -> KnowledgeRow {
        KnowledgeRow {
            id: id.into(),
            pattern: format!("C:\\{id}"),
            title: id.into(),
            written_by: "Windows".into(),
            if_removed: "Nothing happens".into(),
            removable: true,
            source_title: "Docs".into(),
            source_url: "https://example.invalid".into(),
        }
    }

    #[test]
    fn seeding_inserts_every_curated_entry() {
        let index = index();
        let seeded = index.seed_knowledge(&[row("a"), row("b")]).unwrap();

        assert_eq!(seeded, 2);
        assert_eq!(index.knowledge_entry_count().unwrap(), 2);
    }

    #[test]
    fn reseeding_replaces_rather_than_accumulating() {
        let index = index();
        index.seed_knowledge(&[row("a"), row("b")]).unwrap();
        index.seed_knowledge(&[row("a"), row("b")]).unwrap();

        assert_eq!(index.knowledge_entry_count().unwrap(), 2);
    }

    #[test]
    fn an_entry_dropped_from_the_file_disappears_on_reseed() {
        let index = index();
        index.seed_knowledge(&[row("a"), row("removed")]).unwrap();
        index.seed_knowledge(&[row("a")]).unwrap();

        assert_eq!(index.knowledge_entry_count().unwrap(), 1);
    }

    #[test]
    fn seeding_an_empty_set_clears_the_table() {
        let index = index();
        index.seed_knowledge(&[row("a")]).unwrap();
        index.seed_knowledge(&[]).unwrap();

        assert_eq!(index.knowledge_entry_count().unwrap(), 0);
    }
}
