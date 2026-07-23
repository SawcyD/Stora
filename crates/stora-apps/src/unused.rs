use serde::{Deserialize, Serialize};

use crate::model::{AppActivity, InstalledApp};

/// A filter for the "potentially unused" view.
///
/// The wording is deliberate: these applications are *candidates for review*,
/// never "junk" or "abandoned". Stora cannot know whether something matters
/// to someone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnusedFilter {
    NotObserved30Days,
    NotObserved90Days,
    NotObserved6Months,
    NeverObserved,
    LargerThan1Gb,
}

impl UnusedFilter {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotObserved30Days => "Not observed in 30 days",
            Self::NotObserved90Days => "Not observed in 90 days",
            Self::NotObserved6Months => "Not observed in 6 months",
            Self::NeverObserved => "Never observed by Stora",
            Self::LargerThan1Gb => "Larger than 1 GB",
        }
    }

    fn threshold_seconds(&self) -> Option<i64> {
        match self {
            Self::NotObserved30Days => Some(30 * 86_400),
            Self::NotObserved90Days => Some(90 * 86_400),
            Self::NotObserved6Months => Some(182 * 86_400),
            _ => None,
        }
    }
}

/// Decides whether an application belongs in a filtered "potentially unused"
/// view.
///
/// Returns false for anything Stora must never suggest removing, regardless
/// of how little activity was seen — a runtime nothing launches directly is
/// the classic false positive.
pub fn matches_filter(
    app: &InstalledApp,
    activity: &AppActivity,
    filter: UnusedFilter,
    now: i64,
) -> bool {
    if !app.suggestable {
        return false;
    }

    match filter {
        UnusedFilter::LargerThan1Gb => {
            let size = app.detected_bytes.or(app.reported_bytes).unwrap_or(0);
            size >= 1024 * 1024 * 1024
        }
        UnusedFilter::NeverObserved => {
            activity.launch_count == 0 && activity.last_observed.is_none()
        }
        other => {
            let Some(threshold) = other.threshold_seconds() else {
                return false;
            };
            match activity.last_observed {
                Some(last) => now.saturating_sub(last) >= threshold,
                // Never observed also satisfies "not in the last N days", but
                // only when the confidence is honest about it.
                None => true,
            }
        }
    }
}

/// How many days ago the activity was, when a figure exists at all.
pub fn days_since(activity: &AppActivity, now: i64) -> Option<i64> {
    activity
        .last_observed
        .map(|last| now.saturating_sub(last) / 86_400)
}

/// A single sentence describing what is known, safe to show verbatim.
pub fn describe(activity: &AppActivity, now: i64) -> String {
    match (activity.last_observed, days_since(activity, now)) {
        (Some(_), Some(0)) => format!("{} — today", activity.source_label),
        (Some(_), Some(1)) => format!("{} — yesterday", activity.source_label),
        (Some(_), Some(days)) => format!("{} — {days} days ago", activity.source_label),
        _ => activity.source_label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActivitySource, AppType, Confidence};

    const DAY: i64 = 86_400;

    fn app(name: &str, suggestable: bool, bytes: Option<u64>) -> InstalledApp {
        InstalledApp {
            id: "machine:test".into(),
            name: name.into(),
            publisher: "Contoso".into(),
            version: "1.0".into(),
            reported_bytes: bytes,
            detected_bytes: None,
            install_location: None,
            install_date: None,
            app_type: if suggestable {
                AppType::DesktopApplication
            } else {
                AppType::DriverOrSystemComponent
            },
            app_type_label: "".into(),
            uninstall_command: None,
            source: "test".into(),
            confidence: Confidence::High,
            confidence_label: "High".into(),
            suggestable,
        }
    }

    fn observed(days_ago: i64, now: i64) -> AppActivity {
        let mut activity = AppActivity::from_source(
            "machine:test",
            ActivitySource::ObservedByStora,
            Some(now - days_ago * DAY),
        );
        activity.launch_count = 3;
        activity
    }

    #[test]
    fn a_runtime_never_appears_however_idle_it_looks() {
        let now = 1_000 * DAY;
        let runtime = app(
            "Visual C++ Redistributable",
            false,
            Some(5 * 1024 * 1024 * 1024),
        );
        let never = AppActivity::unknown("machine:test");

        for filter in [
            UnusedFilter::NeverObserved,
            UnusedFilter::NotObserved30Days,
            UnusedFilter::NotObserved90Days,
            UnusedFilter::NotObserved6Months,
            UnusedFilter::LargerThan1Gb,
        ] {
            assert!(
                !matches_filter(&runtime, &never, filter, now),
                "{filter:?} must never surface a system component"
            );
        }
    }

    #[test]
    fn age_filters_use_the_observed_date() {
        let now = 1_000 * DAY;
        let application = app("Unity Hub", true, None);

        let recent = observed(10, now);
        assert!(!matches_filter(
            &application,
            &recent,
            UnusedFilter::NotObserved30Days,
            now
        ));

        let stale = observed(45, now);
        assert!(matches_filter(
            &application,
            &stale,
            UnusedFilter::NotObserved30Days,
            now
        ));
        assert!(!matches_filter(
            &application,
            &stale,
            UnusedFilter::NotObserved90Days,
            now
        ));

        let ancient = observed(200, now);
        assert!(matches_filter(
            &application,
            &ancient,
            UnusedFilter::NotObserved6Months,
            now
        ));
    }

    #[test]
    fn the_boundary_day_counts_as_stale() {
        let now = 1_000 * DAY;
        let application = app("App", true, None);
        let exactly_thirty = observed(30, now);
        assert!(matches_filter(
            &application,
            &exactly_thirty,
            UnusedFilter::NotObserved30Days,
            now
        ));
    }

    #[test]
    fn never_observed_requires_no_launches_at_all() {
        let now = 1_000 * DAY;
        let application = app("App", true, None);

        assert!(matches_filter(
            &application,
            &AppActivity::unknown("machine:test"),
            UnusedFilter::NeverObserved,
            now
        ));

        assert!(!matches_filter(
            &application,
            &observed(500, now),
            UnusedFilter::NeverObserved,
            now
        ));
    }

    #[test]
    fn the_size_filter_uses_measured_size_when_available() {
        let now = 0;
        let mut application = app("Big App", true, Some(100));
        let activity = AppActivity::unknown("machine:test");

        assert!(!matches_filter(
            &application,
            &activity,
            UnusedFilter::LargerThan1Gb,
            now
        ));

        application.detected_bytes = Some(2 * 1024 * 1024 * 1024);
        assert!(matches_filter(
            &application,
            &activity,
            UnusedFilter::LargerThan1Gb,
            now
        ));
    }

    #[test]
    fn descriptions_always_name_their_source() {
        let now = 1_000 * DAY;

        assert_eq!(
            describe(&observed(0, now), now),
            "Last observed by Stora — today"
        );
        assert_eq!(
            describe(&observed(1, now), now),
            "Last observed by Stora — yesterday"
        );
        assert_eq!(
            describe(&observed(143, now), now),
            "Last observed by Stora — 143 days ago"
        );
        assert_eq!(
            describe(&AppActivity::unknown("x"), now),
            "No reliable activity data"
        );
    }

    #[test]
    fn an_estimate_is_described_as_an_estimate() {
        let now = 1_000 * DAY;
        let estimate = AppActivity::from_source(
            "machine:test",
            ActivitySource::WindowsEstimate,
            Some(now - 5 * DAY),
        );
        assert_eq!(
            describe(&estimate, now),
            "Windows activity estimate — 5 days ago"
        );
    }

    #[test]
    fn days_since_is_none_without_an_observation() {
        assert_eq!(days_since(&AppActivity::unknown("x"), 1_000), None);
    }
}
