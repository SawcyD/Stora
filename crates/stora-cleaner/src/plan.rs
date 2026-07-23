use std::time::{SystemTime, UNIX_EPOCH};

use stora_core::cleanup::{
    CleanupCategoryResult, CleanupItem, CleanupPlan, CleanupTier, RiskLevel,
};
use stora_core::{Result, TaskControl};
use stora_security::ExclusionSet;

use crate::categories::{self, INSTALLER_AGE_DAYS, INSTALLER_EXTENSIONS};

/// How long a plan stays valid. Long enough to review carefully, short enough
/// that the filesystem has not drifted far when execution begins.
const PLAN_LIFETIME_SECONDS: i64 = 15 * 60;

/// Categories are enumerated to at most this depth, which keeps plan building
/// responsive on caches containing hundreds of thousands of files.
const MAX_DEPTH: usize = 12;

pub struct PlanRequest {
    /// Category ids the user asked to inspect.
    pub category_ids: Vec<String>,
    pub include_advanced: bool,
}

/// Enumerates the requested categories and produces an authoritative plan.
///
/// The plan is the only source of deletable paths: execution accepts indices
/// into `items` and nothing else.
pub fn build(
    request: &PlanRequest,
    exclusions: &ExclusionSet,
    control: &TaskControl,
    now: i64,
) -> Result<CleanupPlan> {
    let mut items: Vec<CleanupItem> = Vec::new();
    let mut results: Vec<CleanupCategoryResult> = Vec::new();

    for category in categories::all() {
        if !request.category_ids.contains(&category.id) {
            continue;
        }
        if category.tier == CleanupTier::Advanced && !request.include_advanced {
            continue;
        }
        if exclusions.excludes_category(&category.id) {
            continue;
        }

        control.checkpoint()?;

        let category_id = category.id.clone();
        let is_installers = category_id == "oldInstallers";

        let mut bytes = 0u64;
        let mut file_count = 0u64;
        let mut folder_count = 0u64;
        let mut unavailable_reason = None;

        // `oldInstallers` filters the Downloads folder rather than owning its
        // own location.
        let locations: Vec<&categories::CategoryLocation> = if is_installers {
            categories::LOCATIONS
                .iter()
                .filter(|l| l.category_id == "downloads")
                .collect()
        } else {
            categories::LOCATIONS
                .iter()
                .filter(|l| l.category_id == category_id)
                .collect()
        };

        if locations.is_empty() {
            // A category with no filesystem locations is guidance-only
            // (for example Windows Update cleanup).
            results.push(CleanupCategoryResult {
                category,
                bytes: 0,
                file_count: 0,
                folder_count: 0,
                unavailable_reason: Some(
                    "This category uses a supported Windows tool rather than direct deletion."
                        .into(),
                ),
            });
            continue;
        }

        let mut any_location_found = false;

        for location in locations {
            for pattern in location.patterns {
                let Some(expanded) = stora_winapi::expand_environment(pattern) else {
                    continue;
                };
                let Ok(root) = stora_security::normalize(&expanded) else {
                    continue;
                };
                if !std::path::Path::new(&root).exists() {
                    continue;
                }
                any_location_found = true;

                let collected = collect(
                    &root,
                    &category_id,
                    exclusions,
                    control,
                    is_installers,
                    now,
                    0,
                )?;

                for item in collected {
                    bytes += item.size;
                    if item.is_directory {
                        folder_count += 1;
                    } else {
                        file_count += 1;
                    }
                    items.push(item);
                }
            }
        }

        if !any_location_found {
            unavailable_reason = Some("No matching location was found on this system.".into());
        }

        results.push(CleanupCategoryResult {
            category,
            bytes,
            file_count,
            folder_count,
            unavailable_reason,
        });
    }

    let total_bytes = items.iter().map(|item| item.size).sum();
    let file_count = items.iter().filter(|item| !item.is_directory).count() as u64;
    let folder_count = items.iter().filter(|item| item.is_directory).count() as u64;

    Ok(CleanupPlan {
        plan_id: new_plan_id(now),
        created_at: now,
        expires_at: now + PLAN_LIFETIME_SECONDS,
        categories: results,
        items,
        total_bytes,
        file_count,
        folder_count,
    })
}

/// Walks a category location, returning the individual files it contains.
///
/// Directories are never returned as items: removing files individually means
/// a locked file in a cache folder cannot take its siblings' removal down with
/// it, and the cache folder itself always survives.
#[allow(clippy::too_many_arguments)]
fn collect(
    root: &str,
    category_id: &str,
    exclusions: &ExclusionSet,
    control: &TaskControl,
    installers_only: bool,
    now: i64,
    depth: usize,
) -> Result<Vec<CleanupItem>> {
    if depth > MAX_DEPTH {
        return Ok(Vec::new());
    }
    control.checkpoint()?;

    let extended = stora_security::to_extended_length(root);
    let Ok(read) = std::fs::read_dir(&extended) else {
        // An unreadable cache folder is normal (in use, or access denied) and
        // must not fail the whole plan.
        return Ok(Vec::new());
    };

    let mut items = Vec::new();

    for entry in read.flatten() {
        let path = entry.path().to_string_lossy().replace('/', "\\");

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        // Never descend into or delete a link: the target may be anywhere.
        if metadata.file_type().is_symlink() {
            continue;
        }
        if stora_security::is_protected(&path) || stora_security::is_sensitive(&path) {
            continue;
        }

        if metadata.is_dir() {
            if exclusions.excludes_directory(&path) {
                continue;
            }
            // Installers are only meaningful at the top of Downloads; do not
            // reach into a user's organized subfolders.
            if installers_only {
                continue;
            }
            items.extend(collect(
                &path,
                category_id,
                exclusions,
                control,
                installers_only,
                now,
                depth + 1,
            )?);
            continue;
        }

        if exclusions.excludes_file(&path) {
            continue;
        }

        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64);

        if installers_only && !is_old_installer(&path, modified, now) {
            continue;
        }

        items.push(CleanupItem {
            path,
            category_id: category_id.to_string(),
            size: metadata.len(),
            is_directory: false,
            modified,
        });
    }

    Ok(items)
}

fn is_old_installer(path: &str, modified: Option<i64>, now: i64) -> bool {
    let Some(extension) = stora_security::extension_of(path) else {
        return false;
    };
    if !INSTALLER_EXTENSIONS.contains(&extension.as_str()) {
        return false;
    }
    // Without a timestamp we cannot show age, so we do not suggest the file.
    let Some(modified) = modified else {
        return false;
    };
    now - modified >= INSTALLER_AGE_DAYS * 24 * 60 * 60
}

/// Indices the UI may preselect: safe, low-risk categories only.
///
/// Anything a user might want to keep — Downloads, the Recycle Bin, installers
/// — is left unselected regardless of how much space it would free.
pub fn default_selection(plan: &CleanupPlan) -> Vec<usize> {
    let safe: Vec<&str> = plan
        .categories
        .iter()
        .filter(|result| {
            result.category.tier == CleanupTier::Safe && result.category.risk == RiskLevel::Low
        })
        .map(|result| result.category.id.as_str())
        .collect();

    plan.items
        .iter()
        .enumerate()
        .filter(|(_, item)| safe.contains(&item.category_id.as_str()))
        .map(|(index, _)| index)
        .collect()
}

fn new_plan_id(now: i64) -> String {
    // A timestamp plus process-local entropy is enough: plan ids only need to
    // be unique within a running session.
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    format!("plan-{now}-{nanos}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use stora_core::cleanup::CleanupCategory;

    fn category_result(id: &str, tier: CleanupTier, risk: RiskLevel) -> CleanupCategoryResult {
        CleanupCategoryResult {
            category: CleanupCategory {
                id: id.into(),
                name: id.into(),
                explanation: String::new(),
                tier,
                risk,
                prefers_windows_mechanism: false,
                learn_more: None,
            },
            bytes: 0,
            file_count: 0,
            folder_count: 0,
            unavailable_reason: None,
        }
    }

    fn item(path: &str, category: &str) -> CleanupItem {
        CleanupItem {
            path: path.into(),
            category_id: category.into(),
            size: 100,
            is_directory: false,
            modified: None,
        }
    }

    #[test]
    fn default_selection_covers_only_safe_low_risk_categories() {
        let plan = CleanupPlan {
            plan_id: "p".into(),
            created_at: 0,
            expires_at: 1_000,
            categories: vec![
                category_result("userTemp", CleanupTier::Safe, RiskLevel::Low),
                category_result(
                    "downloads",
                    CleanupTier::ReviewRequired,
                    RiskLevel::UserReviewRequired,
                ),
                category_result("windowsUpdate", CleanupTier::Advanced, RiskLevel::Advanced),
            ],
            items: vec![
                item("C:\\Temp\\a.tmp", "userTemp"),
                item("C:\\Users\\Test\\Downloads\\photo.raw", "downloads"),
                item("C:\\Temp\\b.tmp", "userTemp"),
            ],
            total_bytes: 300,
            file_count: 3,
            folder_count: 0,
        };

        let selection = default_selection(&plan);
        assert_eq!(selection, vec![0, 2], "Downloads must never be preselected");
    }

    #[test]
    fn default_selection_is_empty_when_nothing_is_safe() {
        let plan = CleanupPlan {
            plan_id: "p".into(),
            created_at: 0,
            expires_at: 1_000,
            categories: vec![category_result(
                "downloads",
                CleanupTier::ReviewRequired,
                RiskLevel::UserReviewRequired,
            )],
            items: vec![item("C:\\Users\\Test\\Downloads\\a.zip", "downloads")],
            total_bytes: 100,
            file_count: 1,
            folder_count: 0,
        };
        assert!(default_selection(&plan).is_empty());
    }

    #[test]
    fn plan_expiry_window_is_applied() {
        let plan = CleanupPlan {
            plan_id: "p".into(),
            created_at: 1_000,
            expires_at: 1_000 + PLAN_LIFETIME_SECONDS,
            categories: vec![],
            items: vec![],
            total_bytes: 0,
            file_count: 0,
            folder_count: 0,
        };
        assert!(!plan.is_expired(1_000));
        assert!(!plan.is_expired(1_000 + PLAN_LIFETIME_SECONDS - 1));
        assert!(plan.is_expired(1_000 + PLAN_LIFETIME_SECONDS));
    }

    #[test]
    fn old_installers_are_matched_by_extension_and_age() {
        let old = 0i64;
        let now = INSTALLER_AGE_DAYS * 24 * 60 * 60 + 10;

        assert!(is_old_installer("C:\\Downloads\\setup.exe", Some(old), now));
        assert!(is_old_installer("C:\\Downloads\\pkg.msi", Some(old), now));

        // Recent installers are left alone.
        assert!(!is_old_installer(
            "C:\\Downloads\\setup.exe",
            Some(now),
            now
        ));
        // Non-installer files are never included.
        assert!(!is_old_installer(
            "C:\\Downloads\\photo.jpg",
            Some(old),
            now
        ));
        // Without a timestamp we cannot justify the suggestion.
        assert!(!is_old_installer("C:\\Downloads\\setup.exe", None, now));
    }

    #[test]
    fn collect_skips_protected_and_returns_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(dir.path().join("a.tmp"), vec![0u8; 50]).unwrap();
        std::fs::write(nested.join("b.tmp"), vec![0u8; 70]).unwrap();

        let control = TaskControl::new();
        let items = collect(
            &dir.path().to_string_lossy().replace('/', "\\"),
            "userTemp",
            &ExclusionSet::default(),
            &control,
            false,
            0,
            0,
        )
        .unwrap();

        assert_eq!(items.len(), 2, "both files, no directories");
        assert!(items.iter().all(|item| !item.is_directory));
        assert_eq!(items.iter().map(|i| i.size).sum::<u64>(), 120);
    }

    #[test]
    fn collect_honors_exclusions() {
        use stora_core::model::{Exclusion, ExclusionKind, ExclusionReason};

        let dir = tempfile::tempdir().unwrap();
        let keep = dir.path().join("keep");
        std::fs::create_dir(&keep).unwrap();
        std::fs::write(keep.join("important.tmp"), vec![0u8; 500]).unwrap();
        std::fs::write(dir.path().join("clear.tmp"), vec![0u8; 50]).unwrap();

        let exclusions = ExclusionSet::from_rules(&[Exclusion {
            id: 0,
            pattern: keep.to_string_lossy().replace('/', "\\"),
            kind: ExclusionKind::Folder,
            reason: ExclusionReason::UserExclusion,
            created_at: 0,
        }]);

        let control = TaskControl::new();
        let items = collect(
            &dir.path().to_string_lossy().replace('/', "\\"),
            "userTemp",
            &exclusions,
            &control,
            false,
            0,
            0,
        )
        .unwrap();

        assert_eq!(items.len(), 1);
        assert!(items[0].path.ends_with("clear.tmp"));
    }

    #[test]
    fn collect_is_cancellable() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.tmp"), b"x").unwrap();

        let control = TaskControl::new();
        control.cancel();

        let err = collect(
            &dir.path().to_string_lossy().replace('/', "\\"),
            "userTemp",
            &ExclusionSet::default(),
            &control,
            false,
            0,
            0,
        )
        .unwrap_err();
        assert_eq!(err.code(), "ScanCancelled");
    }

    #[test]
    fn collect_returns_empty_for_an_unreadable_root() {
        let control = TaskControl::new();
        let items = collect(
            "C:\\this\\path\\does\\not\\exist",
            "userTemp",
            &ExclusionSet::default(),
            &control,
            false,
            0,
            0,
        )
        .unwrap();
        assert!(items.is_empty(), "a missing cache folder is not an error");
    }

    #[test]
    fn plan_ids_are_distinct() {
        let a = new_plan_id(100);
        let b = new_plan_id(100);
        // Same second, different nanosecond component.
        assert!(a.starts_with("plan-100-"));
        assert!(b.starts_with("plan-100-"));
    }
}
