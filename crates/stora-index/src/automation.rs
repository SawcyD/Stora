use rusqlite::{params, OptionalExtension};
use stora_core::cleanup::QuarantineItem;
use stora_core::{Result, StoraError};

use crate::migrations::map_err;
use crate::Index;

/// A stored rule, in the shape `stora-rules` uses.
#[derive(Debug, Clone)]
pub struct StoredRule {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub trigger_kind: String,
    pub action_kind: String,
    pub weekday: u8,
    pub free_space_threshold: u64,
    pub growth_threshold: u64,
    pub watched_path: Option<String>,
    pub categories: Vec<String>,
    pub minimum_age_days: u32,
    pub last_run: Option<i64>,
    pub consecutive_errors: u32,
}

#[derive(Debug, Clone)]
pub struct RuleRun {
    pub id: i64,
    pub rule_id: i64,
    pub ran_at: i64,
    pub outcome: String,
    pub detail: String,
    pub recovered_bytes: u64,
}

impl Index {
    // ------------------------------------------------------- automation

    pub fn create_rule(&self, rule: &StoredRule, now: i64) -> Result<i64> {
        let categories = serde_json::to_string(&rule.categories)
            .map_err(|err| StoraError::Internal(err.to_string()))?;

        self.with(|connection| {
            connection
                .execute(
                    "INSERT INTO automation_rules (name, enabled, trigger_kind, action_kind, \
                     weekday, free_space_threshold, growth_threshold, watched_path, \
                     categories, minimum_age_days, created_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    params![
                        rule.name,
                        rule.enabled as i32,
                        rule.trigger_kind,
                        rule.action_kind,
                        rule.weekday as i64,
                        rule.free_space_threshold as i64,
                        rule.growth_threshold as i64,
                        rule.watched_path,
                        categories,
                        rule.minimum_age_days as i64,
                        now,
                    ],
                )
                .map_err(map_err)?;
            Ok(connection.last_insert_rowid())
        })
    }

    /// Turns a rule on or off.
    ///
    /// Enabling also clears the error counter, so a rule the user has looked
    /// at and re-enabled gets a fresh set of attempts.
    pub fn set_rule_enabled(&self, id: i64, enabled: bool) -> Result<()> {
        self.with(|connection| {
            connection
                .execute(
                    "UPDATE automation_rules SET enabled = ?2, \
                     consecutive_errors = CASE WHEN ?2 = 1 THEN 0 ELSE consecutive_errors END \
                     WHERE id = ?1",
                    params![id, enabled as i32],
                )
                .map_err(map_err)?;
            Ok(())
        })
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        self.with(|connection| {
            connection
                .execute("DELETE FROM automation_rules WHERE id = ?1", params![id])
                .map_err(map_err)?;
            Ok(())
        })
    }

    pub fn rules(&self) -> Result<Vec<StoredRule>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT id, name, enabled, trigger_kind, action_kind, weekday, \
                     free_space_threshold, growth_threshold, watched_path, categories, \
                     minimum_age_days, last_run, consecutive_errors \
                     FROM automation_rules ORDER BY created_at DESC",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map([], |row| {
                    let categories: String = row.get(9)?;
                    Ok(StoredRule {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        enabled: row.get::<_, i32>(2)? != 0,
                        trigger_kind: row.get(3)?,
                        action_kind: row.get(4)?,
                        weekday: row.get::<_, i64>(5)? as u8,
                        free_space_threshold: row.get::<_, i64>(6)? as u64,
                        growth_threshold: row.get::<_, i64>(7)? as u64,
                        watched_path: row.get(8)?,
                        categories: serde_json::from_str(&categories).unwrap_or_default(),
                        minimum_age_days: row.get::<_, i64>(10)? as u32,
                        last_run: row.get(11)?,
                        consecutive_errors: row.get::<_, i64>(12)? as u32,
                    })
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    /// Records a rule execution and updates its error counter.
    pub fn record_rule_run(
        &self,
        rule_id: i64,
        ran_at: i64,
        outcome: &str,
        detail: &str,
        recovered_bytes: u64,
        succeeded: bool,
    ) -> Result<()> {
        self.with(|connection| {
            let tx = connection.transaction().map_err(map_err)?;
            tx.execute(
                "INSERT INTO automation_runs (rule_id, ran_at, outcome, detail, recovered_bytes) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![rule_id, ran_at, outcome, detail, recovered_bytes as i64],
            )
            .map_err(map_err)?;

            // A success resets the counter; a failure advances it toward the
            // limit that disables the rule.
            tx.execute(
                "UPDATE automation_rules SET last_run = ?2, \
                 consecutive_errors = CASE WHEN ?3 = 1 THEN 0 ELSE consecutive_errors + 1 END \
                 WHERE id = ?1",
                params![rule_id, ran_at, succeeded as i32],
            )
            .map_err(map_err)?;

            tx.commit().map_err(map_err)
        })
    }

    pub fn rule_runs(&self, rule_id: i64, limit: usize) -> Result<Vec<RuleRun>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT id, rule_id, ran_at, outcome, detail, recovered_bytes \
                     FROM automation_runs WHERE rule_id = ?1 ORDER BY ran_at DESC LIMIT ?2",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map(params![rule_id, limit as i64], |row| {
                    Ok(RuleRun {
                        id: row.get(0)?,
                        rule_id: row.get(1)?,
                        ran_at: row.get(2)?,
                        outcome: row.get(3)?,
                        detail: row.get(4)?,
                        recovered_bytes: row.get::<_, i64>(5)? as u64,
                    })
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    // --------------------------------------------------------- snapshots

    pub fn record_folder_snapshot(&self, path: &str, taken_at: i64, bytes: u64) -> Result<()> {
        self.with(|connection| {
            connection
                .execute(
                    "INSERT INTO folder_snapshots (path, taken_at, bytes) VALUES (?1, ?2, ?3) \
                     ON CONFLICT (path, taken_at) DO UPDATE SET bytes = excluded.bytes",
                    params![path, taken_at, bytes as i64],
                )
                .map_err(map_err)?;
            Ok(())
        })
    }

    pub fn folder_snapshots(&self, path: &str) -> Result<Vec<(i64, u64)>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT taken_at, bytes FROM folder_snapshots WHERE path = ?1 \
                     ORDER BY taken_at ASC",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map(params![path], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)? as u64))
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    /// Every folder that has at least one recorded snapshot.
    pub fn tracked_folders(&self) -> Result<Vec<String>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached("SELECT DISTINCT path FROM folder_snapshots ORDER BY path")
                .map_err(map_err)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(map_err)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    // -------------------------------------------------------- quarantine

    pub fn record_quarantine(
        &self,
        operation_id: Option<i64>,
        original_path: &str,
        quarantine_path: &str,
        size: u64,
        at: i64,
        expires_at: Option<i64>,
    ) -> Result<i64> {
        self.with(|connection| {
            connection
                .execute(
                    "INSERT INTO quarantine_items (operation_id, original_path, \
                     quarantine_path, size, quarantined_at, expires_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        operation_id,
                        original_path,
                        quarantine_path,
                        size as i64,
                        at,
                        expires_at
                    ],
                )
                .map_err(map_err)?;
            Ok(connection.last_insert_rowid())
        })
    }

    /// Items still held in quarantine and not yet restored.
    pub fn quarantine_items(&self) -> Result<Vec<QuarantineItem>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT id, original_path, quarantine_path, size, quarantined_at, \
                     expires_at FROM quarantine_items WHERE restored = 0 \
                     ORDER BY quarantined_at DESC",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map([], |row| {
                    Ok(QuarantineItem {
                        id: row.get(0)?,
                        original_path: row.get(1)?,
                        quarantine_path: row.get(2)?,
                        size: row.get::<_, i64>(3)? as u64,
                        quarantined_at: row.get(4)?,
                        expires_at: row.get(5)?,
                    })
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    pub fn quarantine_item(&self, id: i64) -> Result<Option<QuarantineItem>> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT id, original_path, quarantine_path, size, quarantined_at, \
                     expires_at FROM quarantine_items WHERE id = ?1 AND restored = 0",
                    params![id],
                    |row| {
                        Ok(QuarantineItem {
                            id: row.get(0)?,
                            original_path: row.get(1)?,
                            quarantine_path: row.get(2)?,
                            size: row.get::<_, i64>(3)? as u64,
                            quarantined_at: row.get(4)?,
                            expires_at: row.get(5)?,
                        })
                    },
                )
                .optional()
                .map_err(map_err)
        })
    }

    pub fn mark_quarantine_restored(&self, id: i64) -> Result<()> {
        self.with(|connection| {
            connection
                .execute(
                    "UPDATE quarantine_items SET restored = 1 WHERE id = ?1",
                    params![id],
                )
                .map_err(map_err)?;
            Ok(())
        })
    }

    pub fn remove_quarantine_record(&self, id: i64) -> Result<()> {
        self.with(|connection| {
            connection
                .execute("DELETE FROM quarantine_items WHERE id = ?1", params![id])
                .map_err(map_err)?;
            Ok(())
        })
    }

    /// Quarantined items whose retention period has passed.
    pub fn expired_quarantine(&self, now: i64) -> Result<Vec<QuarantineItem>> {
        Ok(self
            .quarantine_items()?
            .into_iter()
            .filter(|item| item.expires_at.is_some_and(|expiry| now >= expiry))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Index {
        Index::open_in_memory().unwrap()
    }

    fn rule(name: &str, enabled: bool) -> StoredRule {
        StoredRule {
            id: 0,
            name: name.into(),
            enabled,
            trigger_kind: "weekly".into(),
            action_kind: "notify".into(),
            weekday: 0,
            free_space_threshold: 0,
            growth_threshold: 0,
            watched_path: None,
            categories: vec!["userTemp".into()],
            minimum_age_days: 14,
            last_run: None,
            consecutive_errors: 0,
        }
    }

    #[test]
    fn rules_round_trip_with_their_categories() {
        let index = index();
        let id = index.create_rule(&rule("Weekly temp", false), 100).unwrap();

        let stored = index.rules().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].id, id);
        assert_eq!(stored[0].categories, vec!["userTemp".to_string()]);
        assert!(!stored[0].enabled, "rules are stored disabled");
    }

    #[test]
    fn enabling_a_rule_clears_its_error_count() {
        let index = index();
        let id = index.create_rule(&rule("Rule", false), 100).unwrap();

        for _ in 0..3 {
            index
                .record_rule_run(id, 200, "failed", "could not read", 0, false)
                .unwrap();
        }
        assert_eq!(index.rules().unwrap()[0].consecutive_errors, 3);

        index.set_rule_enabled(id, true).unwrap();
        let stored = &index.rules().unwrap()[0];
        assert!(stored.enabled);
        assert_eq!(
            stored.consecutive_errors, 0,
            "a rule the user re-enabled gets a fresh set of attempts"
        );
    }

    #[test]
    fn a_successful_run_resets_the_error_count() {
        let index = index();
        let id = index.create_rule(&rule("Rule", true), 100).unwrap();

        index
            .record_rule_run(id, 200, "failed", "", 0, false)
            .unwrap();
        index
            .record_rule_run(id, 300, "completed", "", 500, true)
            .unwrap();

        let stored = &index.rules().unwrap()[0];
        assert_eq!(stored.consecutive_errors, 0);
        assert_eq!(stored.last_run, Some(300));
    }

    #[test]
    fn run_history_is_recorded_newest_first() {
        let index = index();
        let id = index.create_rule(&rule("Rule", true), 100).unwrap();

        index
            .record_rule_run(id, 100, "completed", "a", 10, true)
            .unwrap();
        index
            .record_rule_run(id, 200, "completed", "b", 20, true)
            .unwrap();

        let runs = index.rule_runs(id, 10).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].ran_at, 200);
        assert_eq!(runs[0].recovered_bytes, 20);
    }

    #[test]
    fn deleting_a_rule_removes_its_history() {
        let index = index();
        let id = index.create_rule(&rule("Rule", true), 100).unwrap();
        index
            .record_rule_run(id, 100, "completed", "", 0, true)
            .unwrap();

        index.delete_rule(id).unwrap();
        assert!(index.rules().unwrap().is_empty());
        assert!(index.rule_runs(id, 10).unwrap().is_empty());
    }

    #[test]
    fn folder_snapshots_accumulate_and_read_back_in_order() {
        let index = index();
        for (at, bytes) in [(300, 3_000u64), (100, 1_000), (200, 2_000)] {
            index.record_folder_snapshot("C:\\Data", at, bytes).unwrap();
        }

        let snapshots = index.folder_snapshots("C:\\Data").unwrap();
        assert_eq!(snapshots, vec![(100, 1_000), (200, 2_000), (300, 3_000)]);
        assert_eq!(
            index.tracked_folders().unwrap(),
            vec!["C:\\Data".to_string()]
        );
    }

    #[test]
    fn re_recording_the_same_instant_updates_rather_than_duplicates() {
        let index = index();
        index
            .record_folder_snapshot("C:\\Data", 100, 1_000)
            .unwrap();
        index
            .record_folder_snapshot("C:\\Data", 100, 9_000)
            .unwrap();

        assert_eq!(
            index.folder_snapshots("C:\\Data").unwrap(),
            vec![(100, 9_000)]
        );
    }

    #[test]
    fn quarantine_items_round_trip_and_can_be_restored() {
        let index = index();
        let id = index
            .record_quarantine(
                None,
                "C:\\Temp\\a.tmp",
                "C:\\Q\\abc-a.tmp",
                500,
                100,
                Some(800),
            )
            .unwrap();

        let items = index.quarantine_items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].original_path, "C:\\Temp\\a.tmp");
        assert!(index.quarantine_item(id).unwrap().is_some());

        index.mark_quarantine_restored(id).unwrap();
        assert!(
            index.quarantine_items().unwrap().is_empty(),
            "a restored item is no longer held"
        );
        assert!(index.quarantine_item(id).unwrap().is_none());
    }

    #[test]
    fn expiry_only_covers_items_past_their_retention() {
        let index = index();
        index
            .record_quarantine(None, "C:\\a", "C:\\Q\\a", 1, 100, Some(500))
            .unwrap();
        index
            .record_quarantine(None, "C:\\b", "C:\\Q\\b", 1, 100, Some(5_000))
            .unwrap();
        // Kept until manually removed.
        index
            .record_quarantine(None, "C:\\c", "C:\\Q\\c", 1, 100, None)
            .unwrap();

        let expired = index.expired_quarantine(1_000).unwrap();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].original_path, "C:\\a");
    }
}
