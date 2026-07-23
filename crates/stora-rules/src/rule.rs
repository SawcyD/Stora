use serde::{Deserialize, Serialize};

/// What causes a rule to be considered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Trigger {
    /// Every week, on the given weekday.
    Weekly,
    /// When free space on the watched drive falls below a threshold.
    LowFreeSpace,
    /// When a watched folder grows by more than a threshold in a week.
    FolderGrowth,
}

/// What a rule does when it fires.
///
/// There is deliberately no "delete anything" action. The most a rule can do
/// unattended is remove regeneratable cache categories; everything else can
/// only raise a notification for a person to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    /// Show a notification. Never touches the filesystem.
    Notify,
    /// Open the cleanup review with the rule's categories preselected.
    OpenCleanupReview,
    /// Remove the rule's safe categories. Restricted to `SAFE_CATEGORIES`.
    CleanSafeCategories,
}

impl Action {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Notify => "Notify me",
            Self::OpenCleanupReview => "Notify me and open cleanup review",
            Self::CleanSafeCategories => "Remove the selected safe categories",
        }
    }

    pub fn modifies_files(&self) -> bool {
        matches!(self, Self::CleanSafeCategories)
    }
}

/// The only categories an automated rule may ever remove.
///
/// Every one is regeneratable data that the owning program recreates on
/// demand. Downloads, the Recycle Bin, and anything requiring review are
/// absent by design: automation must never delete something a person made.
pub const SAFE_CATEGORIES: &[&str] = &[
    "userTemp",
    "thumbnailCache",
    "shaderCache",
    "crashDumps",
    "errorReports",
    "windowsTemp",
    "deliveryOptimization",
];

/// How many consecutive failures disable a rule.
pub const ERROR_LIMIT: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    pub id: i64,
    pub name: String,
    /// Disabled until the user explicitly turns it on.
    pub enabled: bool,
    pub trigger: Trigger,
    pub action: Action,
    /// Weekday for `Weekly`, 0 = Sunday.
    pub weekday: u8,
    /// Free-space threshold in bytes for `LowFreeSpace`.
    pub free_space_threshold: u64,
    /// Growth threshold in bytes for `FolderGrowth`.
    pub growth_threshold: u64,
    /// Folder watched by `FolderGrowth`.
    pub watched_path: Option<String>,
    /// Cleanup categories this rule affects. Always shown in the interface.
    pub categories: Vec<String>,
    /// Minimum age in days before a temporary file is eligible.
    pub minimum_age_days: u32,
    pub last_run: Option<i64>,
    pub consecutive_errors: u32,
}

impl Default for Rule {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            // The central safety property: nothing runs until asked.
            enabled: false,
            trigger: Trigger::Weekly,
            action: Action::Notify,
            weekday: 0,
            free_space_threshold: 20 * 1024 * 1024 * 1024,
            growth_threshold: 8 * 1024 * 1024 * 1024,
            watched_path: None,
            categories: Vec::new(),
            minimum_age_days: 14,
            last_run: None,
            consecutive_errors: 0,
        }
    }
}

/// Why a rule did not run, when it did not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    Disabled,
    /// Too many consecutive failures.
    ErrorLimitReached,
    /// Already ran within the last day.
    AlreadyRanRecently,
    /// The trigger's condition is not met.
    ConditionNotMet,
    /// The action would touch categories automation may not remove.
    UnsafeCategories(Vec<String>),
}

/// The state a rule is evaluated against.
#[derive(Debug, Clone, Default)]
pub struct Conditions {
    pub now: i64,
    /// 0 = Sunday.
    pub weekday: u8,
    pub free_bytes: u64,
    /// Growth of the watched folder over the last seven days.
    pub weekly_growth_bytes: u64,
}

/// Decides whether a rule should run.
///
/// Every guard is checked in order of severity, so the reason returned is the
/// most important one — a disabled rule reports `Disabled` rather than
/// complaining about its categories.
pub fn should_run(rule: &Rule, conditions: &Conditions) -> std::result::Result<(), Skip> {
    if !rule.enabled {
        return Err(Skip::Disabled);
    }

    if rule.consecutive_errors >= ERROR_LIMIT {
        return Err(Skip::ErrorLimitReached);
    }

    // Guard against a rule firing repeatedly within one day.
    if let Some(last) = rule.last_run {
        if conditions.now.saturating_sub(last) < 86_400 {
            return Err(Skip::AlreadyRanRecently);
        }
    }

    // A rule that deletes may only name categories on the safe list. This is
    // checked at evaluation time, not just when the rule is created, so an
    // edited or restored rule cannot slip past.
    if rule.action.modifies_files() {
        let unsafe_categories: Vec<String> = rule
            .categories
            .iter()
            .filter(|category| !SAFE_CATEGORIES.contains(&category.as_str()))
            .cloned()
            .collect();

        if !unsafe_categories.is_empty() {
            return Err(Skip::UnsafeCategories(unsafe_categories));
        }
        if rule.categories.is_empty() {
            return Err(Skip::ConditionNotMet);
        }
    }

    let triggered = match rule.trigger {
        Trigger::Weekly => conditions.weekday == rule.weekday,
        Trigger::LowFreeSpace => conditions.free_bytes < rule.free_space_threshold,
        Trigger::FolderGrowth => {
            rule.watched_path.is_some() && conditions.weekly_growth_bytes >= rule.growth_threshold
        }
    };

    if triggered {
        Ok(())
    } else {
        Err(Skip::ConditionNotMet)
    }
}

/// Filters a rule's categories down to those automation may remove.
pub fn permitted_categories(rule: &Rule) -> Vec<String> {
    rule.categories
        .iter()
        .filter(|category| SAFE_CATEGORIES.contains(&category.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conditions() -> Conditions {
        Conditions {
            now: 1_000_000,
            weekday: 0,
            free_bytes: 100 * 1024 * 1024 * 1024,
            weekly_growth_bytes: 0,
        }
    }

    fn enabled_rule() -> Rule {
        Rule {
            enabled: true,
            ..Default::default()
        }
    }

    #[test]
    fn rules_are_disabled_when_created() {
        assert!(!Rule::default().enabled);
    }

    #[test]
    fn a_disabled_rule_never_runs() {
        let rule = Rule::default();
        assert_eq!(should_run(&rule, &conditions()), Err(Skip::Disabled));
    }

    #[test]
    fn a_weekly_rule_runs_only_on_its_weekday() {
        let mut rule = enabled_rule();
        rule.weekday = 3;

        let mut on_day = conditions();
        on_day.weekday = 3;
        assert!(should_run(&rule, &on_day).is_ok());

        let mut other_day = conditions();
        other_day.weekday = 4;
        assert_eq!(should_run(&rule, &other_day), Err(Skip::ConditionNotMet));
    }

    #[test]
    fn a_low_space_rule_runs_below_its_threshold() {
        let mut rule = enabled_rule();
        rule.trigger = Trigger::LowFreeSpace;
        rule.free_space_threshold = 20 * 1024 * 1024 * 1024;

        let mut plenty = conditions();
        plenty.free_bytes = 50 * 1024 * 1024 * 1024;
        assert_eq!(should_run(&rule, &plenty), Err(Skip::ConditionNotMet));

        let mut low = conditions();
        low.free_bytes = 10 * 1024 * 1024 * 1024;
        assert!(should_run(&rule, &low).is_ok());
    }

    #[test]
    fn a_growth_rule_needs_a_watched_folder() {
        let mut rule = enabled_rule();
        rule.trigger = Trigger::FolderGrowth;
        rule.growth_threshold = 1024;

        let mut grown = conditions();
        grown.weekly_growth_bytes = 8192;

        assert_eq!(
            should_run(&rule, &grown),
            Err(Skip::ConditionNotMet),
            "no folder is being watched"
        );

        rule.watched_path = Some("C:\\Users\\Test\\Downloads".into());
        assert!(should_run(&rule, &grown).is_ok());
    }

    #[test]
    fn a_rule_does_not_fire_twice_in_one_day() {
        let mut rule = enabled_rule();
        let now = conditions().now;
        rule.last_run = Some(now - 3600);

        assert_eq!(
            should_run(&rule, &conditions()),
            Err(Skip::AlreadyRanRecently)
        );

        rule.last_run = Some(now - 90_000);
        assert!(should_run(&rule, &conditions()).is_ok());
    }

    #[test]
    fn repeated_errors_stop_a_rule() {
        let mut rule = enabled_rule();
        rule.consecutive_errors = ERROR_LIMIT;
        assert_eq!(
            should_run(&rule, &conditions()),
            Err(Skip::ErrorLimitReached)
        );

        rule.consecutive_errors = ERROR_LIMIT - 1;
        assert!(should_run(&rule, &conditions()).is_ok());
    }

    #[test]
    fn automation_may_never_delete_user_content() {
        // The defining safety property of the whole feature.
        let mut rule = enabled_rule();
        rule.action = Action::CleanSafeCategories;
        rule.categories = vec!["userTemp".into(), "downloads".into()];

        match should_run(&rule, &conditions()) {
            Err(Skip::UnsafeCategories(found)) => {
                assert_eq!(found, vec!["downloads".to_string()]);
            }
            other => panic!("expected the Downloads category to be refused, got {other:?}"),
        }
    }

    #[test]
    fn the_recycle_bin_can_never_be_emptied_automatically() {
        let mut rule = enabled_rule();
        rule.action = Action::CleanSafeCategories;
        rule.categories = vec!["recycleBin".into()];
        assert!(matches!(
            should_run(&rule, &conditions()),
            Err(Skip::UnsafeCategories(_))
        ));
    }

    #[test]
    fn a_notify_only_rule_may_reference_any_category() {
        // Naming a category in a notification does not delete anything.
        let mut rule = enabled_rule();
        rule.action = Action::Notify;
        rule.categories = vec!["downloads".into()];
        assert!(should_run(&rule, &conditions()).is_ok());
    }

    #[test]
    fn a_deleting_rule_with_no_categories_does_nothing() {
        let mut rule = enabled_rule();
        rule.action = Action::CleanSafeCategories;
        rule.categories = Vec::new();
        assert_eq!(should_run(&rule, &conditions()), Err(Skip::ConditionNotMet));
    }

    #[test]
    fn permitted_categories_filters_out_everything_unsafe() {
        let mut rule = enabled_rule();
        rule.categories = vec![
            "userTemp".into(),
            "downloads".into(),
            "thumbnailCache".into(),
            "recycleBin".into(),
        ];

        assert_eq!(
            permitted_categories(&rule),
            vec!["userTemp".to_string(), "thumbnailCache".to_string()]
        );
    }

    #[test]
    fn the_safe_list_contains_no_user_content() {
        for category in SAFE_CATEGORIES {
            assert!(
                !matches!(*category, "downloads" | "recycleBin" | "oldInstallers"),
                "{category} is user content and must not be automatable"
            );
        }
    }

    #[test]
    fn only_the_cleaning_action_touches_files() {
        assert!(!Action::Notify.modifies_files());
        assert!(!Action::OpenCleanupReview.modifies_files());
        assert!(Action::CleanSafeCategories.modifies_files());
    }

    #[test]
    fn a_disabled_rule_reports_disabled_even_when_otherwise_invalid() {
        let rule = Rule {
            action: Action::CleanSafeCategories,
            categories: vec!["downloads".into()],
            ..Default::default()
        };
        assert_eq!(should_run(&rule, &conditions()), Err(Skip::Disabled));
    }
}
