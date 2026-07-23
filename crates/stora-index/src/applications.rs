use rusqlite::{params, OptionalExtension};
use stora_core::Result;

use crate::migrations::map_err;
use crate::Index;

/// A stored activity record. Kept as a plain tuple-like struct so this crate
/// does not need to depend on `stora-apps`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredActivity {
    pub executable_path: String,
    pub executable_name: String,
    pub first_observed: i64,
    pub last_observed: i64,
    pub launch_count: u64,
}

impl Index {
    /// Records launches Stora witnessed.
    ///
    /// Repeated launches of the same executable increment a counter rather
    /// than creating a row each time, so the table stays small however long
    /// tracking runs.
    pub fn record_launches(&self, launches: &[(String, String, i64)]) -> Result<()> {
        if launches.is_empty() {
            return Ok(());
        }
        self.with(|connection| {
            let tx = connection.transaction().map_err(map_err)?;
            {
                let mut statement = tx
                    .prepare_cached(
                        "INSERT INTO application_activity \
                         (executable_path, executable_name, first_observed, last_observed, \
                          launch_count) \
                         VALUES (?1, ?2, ?3, ?3, 1) \
                         ON CONFLICT (executable_path) DO UPDATE SET \
                           last_observed = excluded.last_observed, \
                           launch_count = launch_count + 1",
                    )
                    .map_err(map_err)?;

                for (path, name, at) in launches {
                    statement
                        .execute(params![path, name, at])
                        .map_err(map_err)?;
                }
            }
            tx.commit().map_err(map_err)
        })
    }

    pub fn activity_for_executable(&self, path: &str) -> Result<Option<StoredActivity>> {
        self.with(|connection| {
            connection
                .query_row(
                    "SELECT executable_path, executable_name, first_observed, last_observed, \
                     launch_count FROM application_activity WHERE executable_path = ?1",
                    params![path],
                    |row| {
                        Ok(StoredActivity {
                            executable_path: row.get(0)?,
                            executable_name: row.get(1)?,
                            first_observed: row.get(2)?,
                            last_observed: row.get(3)?,
                            launch_count: row.get::<_, i64>(4)? as u64,
                        })
                    },
                )
                .optional()
                .map_err(map_err)
        })
    }

    /// The most recent observation for any executable inside `install_location`.
    ///
    /// An application's launcher is often a nested executable, so activity is
    /// matched by containment rather than an exact path.
    pub fn activity_within(&self, install_location: &str) -> Result<Option<StoredActivity>> {
        // A LIKE prefix match narrows the scan; containment is then confirmed
        // properly so a sibling like `C:\App2` cannot match `C:\App`.
        //
        // No ESCAPE clause: backslash is the path separator here, and using it
        // as the escape character would turn the trailing `\%` into a literal
        // percent sign. `_` therefore stays a wildcard, which can only widen
        // the candidate set — every row is re-checked below regardless.
        let prefix = format!("{}\\%", install_location.trim_end_matches('\\'));

        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT executable_path, executable_name, first_observed, last_observed, \
                     launch_count FROM application_activity \
                     WHERE executable_path LIKE ?1 OR executable_path = ?2 \
                     ORDER BY last_observed DESC",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map(params![prefix, install_location], |row| {
                    Ok(StoredActivity {
                        executable_path: row.get(0)?,
                        executable_name: row.get(1)?,
                        first_observed: row.get(2)?,
                        last_observed: row.get(3)?,
                        launch_count: row.get::<_, i64>(4)? as u64,
                    })
                })
                .map_err(map_err)?;

            for row in rows {
                let record = row.map_err(map_err)?;
                if stora_security::is_within(&record.executable_path, install_location) {
                    return Ok(Some(record));
                }
            }
            Ok(None)
        })
    }

    pub fn all_activity(&self) -> Result<Vec<StoredActivity>> {
        self.with(|connection| {
            let mut statement = connection
                .prepare_cached(
                    "SELECT executable_path, executable_name, first_observed, last_observed, \
                     launch_count FROM application_activity ORDER BY last_observed DESC",
                )
                .map_err(map_err)?;

            let rows = statement
                .query_map([], |row| {
                    Ok(StoredActivity {
                        executable_path: row.get(0)?,
                        executable_name: row.get(1)?,
                        first_observed: row.get(2)?,
                        last_observed: row.get(3)?,
                        launch_count: row.get::<_, i64>(4)? as u64,
                    })
                })
                .map_err(map_err)?;

            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(map_err)
        })
    }

    pub fn clear_activity(&self) -> Result<()> {
        self.with(|connection| {
            connection
                .execute("DELETE FROM application_activity", [])
                .map_err(map_err)?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> Index {
        Index::open_in_memory().unwrap()
    }

    #[test]
    fn a_first_launch_is_recorded_with_a_count_of_one() {
        let index = index();
        index
            .record_launches(&[("C:\\App\\app.exe".into(), "app.exe".into(), 100)])
            .unwrap();

        let stored = index
            .activity_for_executable("C:\\App\\app.exe")
            .unwrap()
            .expect("recorded");
        assert_eq!(stored.launch_count, 1);
        assert_eq!(stored.first_observed, 100);
        assert_eq!(stored.last_observed, 100);
    }

    #[test]
    fn repeat_launches_increment_rather_than_duplicate() {
        let index = index();
        for at in [100, 200, 300] {
            index
                .record_launches(&[("C:\\App\\app.exe".into(), "app.exe".into(), at)])
                .unwrap();
        }

        let stored = index
            .activity_for_executable("C:\\App\\app.exe")
            .unwrap()
            .unwrap();
        assert_eq!(stored.launch_count, 3);
        assert_eq!(
            stored.first_observed, 100,
            "the first sighting is preserved"
        );
        assert_eq!(stored.last_observed, 300);
        assert_eq!(index.all_activity().unwrap().len(), 1);
    }

    #[test]
    fn activity_is_matched_by_containment_within_an_install_folder() {
        let index = index();
        index
            .record_launches(&[(
                "C:\\Program Files\\App\\bin\\app.exe".into(),
                "app.exe".into(),
                500,
            )])
            .unwrap();

        let found = index
            .activity_within("C:\\Program Files\\App")
            .unwrap()
            .expect("nested executable matches");
        assert_eq!(found.last_observed, 500);
    }

    #[test]
    fn a_sibling_folder_with_a_shared_prefix_does_not_match() {
        let index = index();
        index
            .record_launches(&[(
                "C:\\Program Files\\App2\\app.exe".into(),
                "app.exe".into(),
                500,
            )])
            .unwrap();

        assert!(
            index
                .activity_within("C:\\Program Files\\App")
                .unwrap()
                .is_none(),
            "App2 must not be attributed to App"
        );
    }

    #[test]
    fn the_most_recent_observation_wins() {
        let index = index();
        index
            .record_launches(&[
                ("C:\\App\\old.exe".into(), "old.exe".into(), 100),
                ("C:\\App\\new.exe".into(), "new.exe".into(), 900),
            ])
            .unwrap();

        let found = index.activity_within("C:\\App").unwrap().unwrap();
        assert_eq!(found.last_observed, 900);
        assert_eq!(found.executable_name, "new.exe");
    }

    #[test]
    fn an_unobserved_application_returns_nothing_rather_than_a_zero() {
        let index = index();
        assert!(index
            .activity_within("C:\\Program Files\\Unused")
            .unwrap()
            .is_none());
        assert!(index
            .activity_for_executable("C:\\Nope\\x.exe")
            .unwrap()
            .is_none());
    }

    #[test]
    fn empty_batches_are_a_no_op() {
        let index = index();
        index.record_launches(&[]).unwrap();
        assert!(index.all_activity().unwrap().is_empty());
    }

    #[test]
    fn activity_can_be_cleared_for_privacy() {
        let index = index();
        index
            .record_launches(&[("C:\\App\\app.exe".into(), "app.exe".into(), 100)])
            .unwrap();
        index.clear_activity().unwrap();
        assert!(index.all_activity().unwrap().is_empty());
    }
}
