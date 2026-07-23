use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AppType {
    DesktopApplication,
    StoreApplication,
    Game,
    PortableApplication,
    BackgroundUtility,
    DriverOrSystemComponent,
    Unknown,
}

impl AppType {
    pub fn label(&self) -> &'static str {
        match self {
            Self::DesktopApplication => "Desktop application",
            Self::StoreApplication => "Microsoft Store application",
            Self::Game => "Game",
            Self::PortableApplication => "Portable application",
            Self::BackgroundUtility => "Background utility",
            Self::DriverOrSystemComponent => "Driver or system component",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether Stora may ever suggest removing this.
    ///
    /// Runtimes, redistributables, and drivers are excluded outright — they
    /// look unused because nothing launches them directly, yet removing one
    /// can break unrelated software.
    pub fn is_suggestable(&self) -> bool {
        !matches!(self, Self::DriverOrSystemComponent)
    }
}

/// How much Stora trusts a piece of information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Confidence {
    Unknown,
    Low,
    Medium,
    High,
}

impl Confidence {
    pub fn label(&self) -> &'static str {
        match self {
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
            Self::Unknown => "Unknown",
        }
    }
}

/// Where a "last used" figure came from.
///
/// Windows has no single reliable last-opened value, so the source is always
/// stated rather than collapsing everything into one number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ActivitySource {
    /// Stora watched the process start.
    ObservedByStora,
    /// Inferred from Windows shell artifacts. An estimate, always labelled.
    WindowsEstimate,
    /// Only indirect file timestamps were available.
    FileActivity,
    None,
}

impl ActivitySource {
    /// The exact wording shown in the interface.
    pub fn label(&self) -> &'static str {
        match self {
            Self::ObservedByStora => "Last observed by Stora",
            Self::WindowsEstimate => "Windows activity estimate",
            Self::FileActivity => "File activity only",
            Self::None => "No reliable activity data",
        }
    }

    pub fn confidence(&self) -> Confidence {
        match self {
            Self::ObservedByStora => Confidence::High,
            Self::WindowsEstimate => Confidence::Medium,
            Self::FileActivity => Confidence::Low,
            Self::None => Confidence::Unknown,
        }
    }

    /// Why the interface is claiming this confidence level.
    pub fn explanation(&self) -> &'static str {
        match self {
            Self::ObservedByStora => "Stora directly observed this application launch.",
            Self::WindowsEstimate => {
                "Windows shell activity suggests this application was used. This is an \
                 estimate, not a recorded launch."
            }
            Self::FileActivity => {
                "Only indirect file activity was detected. This often reflects an update \
                 rather than someone using the application."
            }
            Self::None => "No reliable usage information is available.",
        }
    }
}

/// A folder attributed to an application, with the evidence for the link.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FootprintLocation {
    pub path: String,
    /// What this folder holds: application files, cache, user data, and so on.
    pub relationship: String,
    pub bytes: u64,
    pub confidence: Confidence,
    pub confidence_label: String,
    /// Stated in the interface so the attribution can be judged.
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    /// Size the installer reported, when it reported one.
    pub reported_bytes: Option<u64>,
    /// Size Stora measured on disk.
    pub detected_bytes: Option<u64>,
    pub install_location: Option<String>,
    pub install_date: Option<i64>,
    pub app_type: AppType,
    pub app_type_label: String,
    /// The registered uninstall command. Removal always goes through this.
    pub uninstall_command: Option<String>,
    pub source: String,
    pub confidence: Confidence,
    pub confidence_label: String,
    /// Whether Stora may include this in "potentially unused".
    pub suggestable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppActivity {
    pub app_id: String,
    pub executable_path: Option<String>,
    pub first_observed: Option<i64>,
    pub last_observed: Option<i64>,
    pub launch_count: u64,
    pub source: ActivitySource,
    pub source_label: String,
    pub confidence: Confidence,
    pub confidence_label: String,
    pub explanation: String,
}

impl AppActivity {
    /// An entry meaning "we genuinely do not know".
    pub fn unknown(app_id: &str) -> Self {
        let source = ActivitySource::None;
        Self {
            app_id: app_id.to_string(),
            executable_path: None,
            first_observed: None,
            last_observed: None,
            launch_count: 0,
            source,
            source_label: source.label().to_string(),
            confidence: source.confidence(),
            confidence_label: source.confidence().label().to_string(),
            explanation: source.explanation().to_string(),
        }
    }

    pub fn from_source(app_id: &str, source: ActivitySource, last_observed: Option<i64>) -> Self {
        Self {
            app_id: app_id.to_string(),
            executable_path: None,
            first_observed: None,
            last_observed,
            launch_count: 0,
            source,
            source_label: source.label().to_string(),
            confidence: source.confidence(),
            confidence_label: source.confidence().label().to_string(),
            explanation: source.explanation().to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppFootprint {
    pub app_id: String,
    pub locations: Vec<FootprintLocation>,
    pub total_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_sources_map_to_the_right_confidence() {
        assert_eq!(
            ActivitySource::ObservedByStora.confidence(),
            Confidence::High
        );
        assert_eq!(
            ActivitySource::WindowsEstimate.confidence(),
            Confidence::Medium
        );
        assert_eq!(ActivitySource::FileActivity.confidence(), Confidence::Low);
        assert_eq!(ActivitySource::None.confidence(), Confidence::Unknown);
    }

    #[test]
    fn no_source_is_ever_labelled_simply_last_opened() {
        // Windows cannot supply one reliable "last opened" value, so the
        // interface must never imply it has one.
        for source in [
            ActivitySource::ObservedByStora,
            ActivitySource::WindowsEstimate,
            ActivitySource::FileActivity,
            ActivitySource::None,
        ] {
            assert_ne!(source.label(), "Last opened");
        }
    }

    #[test]
    fn the_windows_estimate_is_labelled_as_an_estimate() {
        assert!(ActivitySource::WindowsEstimate.label().contains("estimate"));
        assert!(ActivitySource::WindowsEstimate
            .explanation()
            .contains("estimate"));
    }

    #[test]
    fn unknown_activity_claims_nothing() {
        let activity = AppActivity::unknown("app");
        assert!(activity.last_observed.is_none());
        assert_eq!(activity.launch_count, 0);
        assert_eq!(activity.confidence, Confidence::Unknown);
    }

    #[test]
    fn system_components_are_never_suggested_for_removal() {
        assert!(!AppType::DriverOrSystemComponent.is_suggestable());
        assert!(AppType::DesktopApplication.is_suggestable());
        assert!(AppType::Game.is_suggestable());
    }

    #[test]
    fn confidence_orders_from_unknown_up_to_high() {
        assert!(Confidence::High > Confidence::Medium);
        assert!(Confidence::Medium > Confidence::Low);
        assert!(Confidence::Low > Confidence::Unknown);
    }
}
