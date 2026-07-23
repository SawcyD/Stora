use serde::{Deserialize, Serialize};

/// A folder's size at a point in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub taken_at: i64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TimeRange {
    Day,
    Week,
    Month,
    Quarter,
    SinceInstall,
}

impl TimeRange {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Day => "24 hours",
            Self::Week => "7 days",
            Self::Month => "30 days",
            Self::Quarter => "90 days",
            Self::SinceInstall => "Since Stora was installed",
        }
    }

    pub fn seconds(&self) -> Option<i64> {
        match self {
            Self::Day => Some(86_400),
            Self::Week => Some(7 * 86_400),
            Self::Month => Some(30 * 86_400),
            Self::Quarter => Some(90 * 86_400),
            Self::SinceInstall => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrowthEntry {
    pub path: String,
    pub name: String,
    pub current_bytes: u64,
    /// Positive when the folder grew, negative when it shrank.
    pub change_bytes: i64,
    /// When the comparison snapshot was taken, so the figure can be judged.
    pub compared_at: i64,
    /// False when there is no earlier snapshot to compare against.
    pub has_baseline: bool,
}

/// Computes a folder's change over a time range from stored snapshots.
///
/// Uses the newest snapshot at or before the cutoff as the baseline. When no
/// snapshot is old enough, `has_baseline` is false and the change is reported
/// as zero rather than as growth — a folder Stora has only just started
/// watching has not "grown by its whole size".
pub fn change_over(snapshots: &[Snapshot], range: TimeRange, now: i64) -> GrowthEntry {
    let mut ordered: Vec<Snapshot> = snapshots.to_vec();
    ordered.sort_by_key(|snapshot| snapshot.taken_at);

    let Some(latest) = ordered.last().copied() else {
        return GrowthEntry {
            path: String::new(),
            name: String::new(),
            current_bytes: 0,
            change_bytes: 0,
            compared_at: now,
            has_baseline: false,
        };
    };

    let baseline = match range.seconds() {
        Some(window) => {
            let cutoff = now - window;
            ordered
                .iter()
                .rfind(|snapshot| snapshot.taken_at <= cutoff)
                .copied()
        }
        // "Since install" compares against the very first snapshot.
        None => ordered.first().copied(),
    };

    match baseline {
        // A single snapshot cannot describe a change.
        Some(base) if base.taken_at == latest.taken_at => GrowthEntry {
            path: String::new(),
            name: String::new(),
            current_bytes: latest.bytes,
            change_bytes: 0,
            compared_at: base.taken_at,
            has_baseline: false,
        },
        Some(base) => GrowthEntry {
            path: String::new(),
            name: String::new(),
            current_bytes: latest.bytes,
            change_bytes: latest.bytes as i64 - base.bytes as i64,
            compared_at: base.taken_at,
            has_baseline: true,
        },
        None => GrowthEntry {
            path: String::new(),
            name: String::new(),
            current_bytes: latest.bytes,
            change_bytes: 0,
            compared_at: latest.taken_at,
            has_baseline: false,
        },
    }
}

/// A local alert. Informative, never alarming.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub title: String,
    pub detail: String,
}

/// Builds an alert for low free space, if the threshold is crossed.
pub fn low_space_alert(root: &str, free_bytes: u64, threshold: u64) -> Option<Alert> {
    if free_bytes >= threshold {
        return None;
    }
    Some(Alert {
        id: format!("lowSpace:{root}"),
        title: format!(
            "Available storage on {} fell below {}.",
            root.trim_end_matches('\\'),
            stora_core::format_bytes(threshold)
        ),
        detail: format!("{} is available now.", stora_core::format_bytes(free_bytes)),
    })
}

/// Builds an alert for a folder that grew sharply.
pub fn growth_alert(
    path: &str,
    change_bytes: i64,
    threshold: u64,
    range: TimeRange,
) -> Option<Alert> {
    if change_bytes <= 0 || (change_bytes as u64) < threshold {
        return None;
    }
    Some(Alert {
        id: format!("growth:{path}"),
        title: format!(
            "{} grew by {} in {}.",
            stora_security::file_name_of(path),
            stora_core::format_bytes(change_bytes as u64),
            range.label()
        ),
        detail: path.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    fn snapshot(days_ago: i64, bytes: u64, now: i64) -> Snapshot {
        Snapshot {
            taken_at: now - days_ago * DAY,
            bytes,
        }
    }

    #[test]
    fn growth_is_measured_against_the_snapshot_before_the_cutoff() {
        let now = 100 * DAY;
        let snapshots = vec![
            snapshot(10, 1_000, now),
            snapshot(3, 5_000, now),
            snapshot(0, 9_000, now),
        ];

        let week = change_over(&snapshots, TimeRange::Week, now);
        assert!(week.has_baseline);
        assert_eq!(week.current_bytes, 9_000);
        assert_eq!(
            week.change_bytes, 8_000,
            "compared against the 10-day-old snapshot"
        );
    }

    #[test]
    fn a_shrinking_folder_reports_a_negative_change() {
        let now = 100 * DAY;
        let snapshots = vec![snapshot(10, 9_000, now), snapshot(0, 1_000, now)];

        let week = change_over(&snapshots, TimeRange::Week, now);
        assert_eq!(week.change_bytes, -8_000);
    }

    #[test]
    fn a_folder_with_no_old_snapshot_is_not_reported_as_having_grown() {
        // The important case: Stora started watching yesterday, so it cannot
        // claim the folder grew by its entire size.
        let now = 100 * DAY;
        let snapshots = vec![snapshot(1, 50_000, now), snapshot(0, 50_000, now)];

        let month = change_over(&snapshots, TimeRange::Month, now);
        assert!(!month.has_baseline);
        assert_eq!(month.change_bytes, 0);
        assert_eq!(month.current_bytes, 50_000);
    }

    #[test]
    fn a_single_snapshot_reports_no_change() {
        let now = 100 * DAY;
        let entry = change_over(&[snapshot(0, 1_234, now)], TimeRange::Week, now);
        assert!(!entry.has_baseline);
        assert_eq!(entry.change_bytes, 0);
        assert_eq!(entry.current_bytes, 1_234);
    }

    #[test]
    fn no_snapshots_yields_an_empty_entry() {
        let entry = change_over(&[], TimeRange::Week, 100);
        assert!(!entry.has_baseline);
        assert_eq!(entry.current_bytes, 0);
    }

    #[test]
    fn since_install_compares_against_the_earliest_snapshot() {
        let now = 500 * DAY;
        let snapshots = vec![
            snapshot(400, 1_000, now),
            snapshot(100, 4_000, now),
            snapshot(0, 7_000, now),
        ];

        let entry = change_over(&snapshots, TimeRange::SinceInstall, now);
        assert!(entry.has_baseline);
        assert_eq!(entry.change_bytes, 6_000);
    }

    #[test]
    fn snapshots_out_of_order_are_handled() {
        let now = 100 * DAY;
        let snapshots = vec![
            snapshot(0, 9_000, now),
            snapshot(10, 1_000, now),
            snapshot(3, 5_000, now),
        ];

        let entry = change_over(&snapshots, TimeRange::Week, now);
        assert_eq!(entry.current_bytes, 9_000);
        assert_eq!(entry.change_bytes, 8_000);
    }

    #[test]
    fn a_low_space_alert_appears_only_below_the_threshold() {
        let threshold = 20 * 1024 * 1024 * 1024;
        assert!(low_space_alert("C:\\", threshold + 1, threshold).is_none());

        let alert =
            low_space_alert("C:\\", 5 * 1024 * 1024 * 1024, threshold).expect("threshold crossed");
        assert!(alert.title.contains("C:"));
        assert!(alert.title.contains("20.0 GB"));
    }

    #[test]
    fn alerts_are_informative_rather_than_alarming() {
        let alert = low_space_alert("C:\\", 1024, 2048).unwrap();
        let text = format!("{} {}", alert.title, alert.detail).to_lowercase();

        for word in [
            "risk",
            "danger",
            "warning!",
            "critical",
            "immediately",
            "act now",
        ] {
            assert!(
                !text.contains(word),
                "alert must not use scare wording: {word}"
            );
        }
    }

    #[test]
    fn a_growth_alert_ignores_shrinking_and_small_changes() {
        assert!(growth_alert("C:\\Downloads", -5_000, 1_000, TimeRange::Week).is_none());
        assert!(growth_alert("C:\\Downloads", 500, 1_000, TimeRange::Week).is_none());

        let alert = growth_alert(
            "C:\\Users\\Test\\Downloads",
            9 * 1024 * 1024 * 1024,
            1_000,
            TimeRange::Week,
        )
        .expect("threshold crossed");
        assert!(alert.title.contains("Downloads"));
        assert!(alert.title.contains("7 days"));
    }

    #[test]
    fn every_range_has_a_readable_label() {
        for range in [
            TimeRange::Day,
            TimeRange::Week,
            TimeRange::Month,
            TimeRange::Quarter,
            TimeRange::SinceInstall,
        ] {
            assert!(!range.label().is_empty());
        }
        assert_eq!(TimeRange::SinceInstall.seconds(), None);
        assert_eq!(TimeRange::Week.seconds(), Some(7 * DAY));
    }
}
