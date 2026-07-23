use stora_core::cleanup::{CleanupCategory, CleanupTier, RiskLevel};

/// Where a category's files live. Patterns use `%VAR%` environment syntax and
/// are expanded at detection time, so Stora never hardcodes a user name.
pub struct CategoryLocation {
    pub category_id: &'static str,
    pub patterns: &'static [&'static str],
    /// When true, the folder itself is kept and only its contents removed.
    pub contents_only: bool,
}

/// Every cleanup category Stora knows about.
///
/// Wording here is deliberately factual: each explanation says what the data
/// is and what happens after removal. Nothing is called "junk", and no
/// category claims a benefit it cannot demonstrate.
pub fn all() -> Vec<CleanupCategory> {
    vec![
        CleanupCategory {
            id: "userTemp".into(),
            name: "User temporary files".into(),
            explanation:
                "Files applications wrote to your temporary folder. Programs recreate these as \
                 needed. Files currently in use are skipped."
                    .into(),
            tier: CleanupTier::Safe,
            risk: RiskLevel::Low,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "thumbnailCache".into(),
            name: "Thumbnail cache".into(),
            explanation:
                "Windows will recreate image and video thumbnails as needed. Folders may take \
                 slightly longer to display the first time you open them."
                    .into(),
            tier: CleanupTier::Safe,
            risk: RiskLevel::Low,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "shaderCache".into(),
            name: "DirectX shader cache".into(),
            explanation:
                "Compiled graphics shaders. Games and applications recompile them automatically, \
                 which can cause brief stutter the first time a scene loads."
                    .into(),
            tier: CleanupTier::Safe,
            risk: RiskLevel::Low,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "crashDumps".into(),
            name: "Application crash dumps".into(),
            explanation:
                "Memory snapshots written when an application stopped responding. Useful only \
                 while diagnosing that specific crash."
                    .into(),
            tier: CleanupTier::Safe,
            risk: RiskLevel::Low,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "errorReports".into(),
            name: "Old error reports".into(),
            explanation:
                "Queued Windows Error Reporting data. Removing these does not affect diagnostics \
                 already sent to Microsoft."
                    .into(),
            tier: CleanupTier::Safe,
            risk: RiskLevel::Low,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "windowsTemp".into(),
            name: "System temporary files".into(),
            explanation:
                "Temporary files written by Windows and installers. Files still held open by a \
                 running process are skipped."
                    .into(),
            tier: CleanupTier::Safe,
            risk: RiskLevel::Low,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "deliveryOptimization".into(),
            name: "Delivery Optimization files".into(),
            explanation:
                "Cached Windows Update data shared with other devices on your network. Windows \
                 downloads it again if it is needed."
                    .into(),
            tier: CleanupTier::Safe,
            risk: RiskLevel::Low,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "browserCache".into(),
            name: "Browser caches".into(),
            explanation:
                "Cached web page data. Sites will load more slowly once, then return to normal. \
                 Your history, passwords, and open tabs are not affected."
                    .into(),
            tier: CleanupTier::Safe,
            risk: RiskLevel::Low,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "downloads".into(),
            name: "Downloads".into(),
            explanation:
                "Files you downloaded. Stora never selects these for you — review each one, \
                 because this folder often holds files that exist nowhere else."
                    .into(),
            tier: CleanupTier::ReviewRequired,
            risk: RiskLevel::UserReviewRequired,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "oldInstallers".into(),
            name: "Old installers".into(),
            explanation:
                "Setup packages in your Downloads folder that have not been modified in over 60 \
                 days. The installed application is unaffected."
                    .into(),
            tier: CleanupTier::ReviewRequired,
            risk: RiskLevel::Moderate,
            prefers_windows_mechanism: false,
            learn_more: None,
        },
        CleanupCategory {
            id: "recycleBin".into(),
            name: "Recycle Bin".into(),
            explanation:
                "Items you already deleted. Emptying the Recycle Bin cannot be undone, so this is \
                 never selected automatically."
                    .into(),
            tier: CleanupTier::ReviewRequired,
            risk: RiskLevel::UserReviewRequired,
            prefers_windows_mechanism: true,
            learn_more: None,
        },
        CleanupCategory {
            id: "windowsUpdateCleanup".into(),
            name: "Windows Update cleanup".into(),
            explanation:
                "Superseded update files in the component store. This must be done through the \
                 Windows servicing tools, not by deleting files. Stora shows you the supported \
                 command and does not run it for you."
                    .into(),
            tier: CleanupTier::Advanced,
            risk: RiskLevel::Advanced,
            prefers_windows_mechanism: true,
            learn_more: Some(
                "https://learn.microsoft.com/windows-server/administration/windows-commands/dism"
                    .into(),
            ),
        },
    ]
}

/// Filesystem locations for the categories Stora removes directly.
///
/// Advanced categories are intentionally absent: they are surfaced with
/// guidance toward the supported Windows mechanism instead.
pub const LOCATIONS: &[CategoryLocation] = &[
    CategoryLocation {
        category_id: "userTemp",
        patterns: &["%LOCALAPPDATA%\\Temp"],
        contents_only: true,
    },
    CategoryLocation {
        category_id: "thumbnailCache",
        patterns: &["%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer"],
        contents_only: true,
    },
    CategoryLocation {
        category_id: "shaderCache",
        patterns: &[
            "%LOCALAPPDATA%\\D3DSCache",
            "%LOCALAPPDATA%\\NVIDIA\\DXCache",
            "%LOCALAPPDATA%\\AMD\\DxCache",
        ],
        contents_only: true,
    },
    CategoryLocation {
        category_id: "crashDumps",
        patterns: &["%LOCALAPPDATA%\\CrashDumps"],
        contents_only: true,
    },
    CategoryLocation {
        category_id: "errorReports",
        patterns: &[
            "%LOCALAPPDATA%\\Microsoft\\Windows\\WER\\ReportArchive",
            "%LOCALAPPDATA%\\Microsoft\\Windows\\WER\\ReportQueue",
        ],
        contents_only: true,
    },
    CategoryLocation {
        category_id: "windowsTemp",
        patterns: &["C:\\Windows\\Temp"],
        contents_only: true,
    },
    CategoryLocation {
        category_id: "deliveryOptimization",
        patterns: &["C:\\Windows\\SoftwareDistribution\\Download"],
        contents_only: true,
    },
    CategoryLocation {
        category_id: "browserCache",
        patterns: &[
            "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\Default\\Cache",
            "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\Default\\Cache",
            "%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data\\Default\\Cache",
            "%APPDATA%\\Mozilla\\Firefox\\Profiles",
        ],
        contents_only: true,
    },
    CategoryLocation {
        category_id: "downloads",
        patterns: &["%USERPROFILE%\\Downloads"],
        contents_only: true,
    },
];

/// Extensions treated as installer packages for the `oldInstallers` category.
pub const INSTALLER_EXTENSIONS: &[&str] = &["exe", "msi", "msix", "appx", "msu"];

/// Age in days before an installer is considered old enough to suggest.
pub const INSTALLER_AGE_DAYS: i64 = 60;

pub fn find(id: &str) -> Option<CleanupCategory> {
    all().into_iter().find(|category| category.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_location_maps_to_a_declared_category() {
        let categories = all();
        for location in LOCATIONS {
            assert!(
                categories.iter().any(|c| c.id == location.category_id),
                "location references unknown category: {}",
                location.category_id
            );
        }
    }

    #[test]
    fn category_ids_are_unique() {
        let categories = all();
        let mut ids: Vec<&str> = categories.iter().map(|c| c.id.as_str()).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "duplicate category id");
    }

    #[test]
    fn no_explanation_uses_scare_or_junk_wording() {
        // Product principle: Stora never calls user data junk or implies risk
        // to pressure a cleanup.
        let banned = [
            "junk",
            "garbage",
            "crap",
            "infected",
            "at risk",
            "danger",
            "boost",
            "speed up",
            "optimize your pc",
        ];
        for category in all() {
            let text = format!("{} {}", category.name, category.explanation).to_lowercase();
            for word in banned {
                assert!(
                    !text.contains(word),
                    "category {} uses banned wording: {word}",
                    category.id
                );
            }
        }
    }

    #[test]
    fn advanced_categories_defer_to_windows() {
        for category in all() {
            if category.tier == CleanupTier::Advanced {
                assert!(
                    category.prefers_windows_mechanism,
                    "advanced category {} must use a supported Windows mechanism",
                    category.id
                );
            }
        }
    }

    #[test]
    fn advanced_categories_have_no_direct_delete_locations() {
        for category in all().iter().filter(|c| c.tier == CleanupTier::Advanced) {
            assert!(
                !LOCATIONS.iter().any(|l| l.category_id == category.id),
                "advanced category {} must not be deleted directly",
                category.id
            );
        }
    }

    #[test]
    fn downloads_is_never_a_safe_tier() {
        let downloads = find("downloads").unwrap();
        assert_eq!(downloads.tier, CleanupTier::ReviewRequired);
        assert_eq!(downloads.risk, RiskLevel::UserReviewRequired);
    }

    #[test]
    fn recycle_bin_requires_review() {
        let bin = find("recycleBin").unwrap();
        assert_eq!(bin.risk, RiskLevel::UserReviewRequired);
    }

    #[test]
    fn lookup_returns_none_for_unknown_ids() {
        assert!(find("notARealCategory").is_none());
    }
}
