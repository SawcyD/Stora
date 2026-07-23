/**
 * Typed wrappers around the Tauri command surface.
 *
 * Nothing outside this module calls `invoke` directly, which keeps the
 * command names in one place and guarantees consistent error shaping.
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  CleanupCategory,
  CleanupHistoryEntry,
  CleanupItem,
  CleanupItemError,
  CleanupPlanResponse,
  CleanupProgress,
  CleanupResult,
  CategoryBreakdown,
  DriveInfo,
  Exclusion,
  ExclusionKind,
  FolderAggregate,
  RelocationCandidate,
  RelocationPlan,
  RelocationResult,
  LargeFile,
  ScanProgress,
  ScanStarted,
  ScanSummary,
  Settings,
  StoraError,
  SystemAppearance,
  AdvisorKeyStatus,
  UiState,
  DeveloperScanResult,
  VirtualDisk,
  AppWithActivity,
  AppFootprint,
  DuplicateReport,
  DuplicateGroup,
  GrowthRow,
  Alert,
  AutomationRule,
  RuleRunRow,
  QuarantineItem,
  UninstallPreflight,
  UninstallStarted,
  Leftover,
  Explanation,
  AdvisorAnswer,
  AdvisorResearchAnswer,
} from "./types";

export const SCAN_PROGRESS_EVENT = "stora://scan-progress";
export const CLEANUP_PROGRESS_EVENT = "stora://cleanup-progress";
export const NAVIGATE_EVENT = "stora://navigate";
export const AUTOMATION_RUN_EVENT = "stora://automation-run";

/** True when a rejected command produced a structured Stora error. */
export function isStoraError(value: unknown): value is StoraError {
  return (
    typeof value === "object" &&
    value !== null &&
    "code" in value &&
    "message" in value
  );
}

/**
 * Converts a backend error into wording a person can act on.
 *
 * Raw Rust messages never reach the interface.
 */
export function describeError(error: unknown): { title: string; detail?: string } {
  if (!isStoraError(error)) {
    return {
      title: "Something went wrong",
      detail: typeof error === "string" ? error : undefined,
    };
  }

  const path = error.path ?? undefined;

  switch (error.code) {
    case "AccessDenied":
      return {
        title: "Stora could not read this location because Windows denied access.",
        detail: path,
      };
    case "PathNotFound":
      return { title: "This location no longer exists.", detail: path };
    case "VolumeUnavailable":
      return { title: "This drive is not available right now.", detail: path };
    case "FileLocked":
      return {
        title: "This item is currently in use by another program.",
        detail: path,
      };
    case "ProtectedPath":
      return {
        title: "This is a protected Windows location and Stora will not modify it.",
        detail: path,
      };
    case "PathChangedAfterPreview":
      return {
        title: "This item changed after you reviewed it, so it was skipped.",
        detail: path,
      };
    case "CleanupPlanExpired":
      return {
        title: "This cleanup review has expired. Scan the categories again to refresh it.",
      };
    case "PathNotAuthorized":
      return {
        title: "This item was not part of the reviewed cleanup and was not removed.",
        detail: path,
      };
    case "ElevationRequired":
      return { title: "This operation needs administrator permission." };
    case "ScanCancelled":
      return { title: "The scan was cancelled." };
    case "TaskAlreadyRunning":
      return { title: "That operation is already running." };
    case "DatabaseBusy":
      return { title: "Stora is busy writing results. Try again in a moment." };
    case "ReparseLoop":
      return {
        title: "Stora stopped following a folder link that pointed back into itself.",
        detail: path,
      };
    default:
      return { title: error.message };
  }
}

// ------------------------------------------------------------------- drives

export const listDrives = () => invoke<DriveInfo[]>("list_drives");

// -------------------------------------------------------------------- scans

export const startScan = (root: string) =>
  invoke<ScanStarted>("start_scan", { root });

export const pauseScan = (taskId: string) => invoke<void>("pause_scan", { taskId });
export const resumeScan = (taskId: string) => invoke<void>("resume_scan", { taskId });
export const cancelScan = (taskId: string) => invoke<void>("cancel_scan", { taskId });

export const getScanSummary = (root: string) =>
  invoke<ScanSummary | null>("get_scan_summary", { root });

export const onScanProgress = (
  handler: (progress: ScanProgress) => void,
): Promise<UnlistenFn> =>
  listen<ScanProgress>(SCAN_PROGRESS_EVENT, (event) => handler(event.payload));

// ------------------------------------------------------------------ storage

export const getFolderChildren = (root: string, parentPath: string) =>
  invoke<FolderAggregate[]>("get_folder_children", { root, parentPath });

export const getFolderDetails = (root: string, path: string) =>
  invoke<FolderAggregate | null>("get_folder_details", { root, path });

export const getLargeFiles = (root: string, minimumBytes: number, limit: number) =>
  invoke<LargeFile[]>("get_large_files", { root, minimumBytes, limit });

export const getStorageBreakdown = (root: string) =>
  invoke<CategoryBreakdown[]>("get_storage_breakdown", { root });

export const getRelocationCandidates = (root: string) =>
  invoke<RelocationCandidate[]>("get_relocation_candidates", { root });

export const buildRelocationPlan = (source: string, destinationRoot: string) =>
  invoke<RelocationPlan>("build_relocation_plan", { source, destinationRoot });

export const executeRelocation = (source: string, destinationRoot: string) =>
  invoke<RelocationResult>("execute_relocation", { source, destinationRoot });

export const revealInExplorer = (path: string) =>
  invoke<void>("reveal_in_explorer", { path });

// ------------------------------------------------------------------ cleanup

export const getCleanupCategories = () =>
  invoke<CleanupCategory[]>("get_cleanup_categories");

export const buildCleanupPlan = (categoryIds: string[]) =>
  invoke<CleanupPlanResponse>("build_cleanup_plan", { categoryIds });

export const executeCleanupPlan = (
  planId: string,
  selectedIndices: number[],
  method: string,
) =>
  invoke<CleanupResult>("execute_cleanup_plan", {
    request: { planId, selectedIndices, method },
  });

export const cancelCleanup = (taskId: string) =>
  invoke<void>("cancel_cleanup", { taskId });

export const getPlanItems = (planId: string, categoryId: string, limit: number) =>
  invoke<CleanupItem[]>("get_plan_items", { planId, categoryId, limit });

export const getCleanupHistory = (limit: number) =>
  invoke<CleanupHistoryEntry[]>("get_cleanup_history", { limit });

export const getCleanupErrors = (operationId: number) =>
  invoke<CleanupItemError[]>("get_cleanup_errors", { operationId });

export const getLockingProcesses = (path: string) =>
  invoke<string[]>("get_locking_processes", { path });

export const onCleanupProgress = (
  handler: (progress: CleanupProgress) => void,
): Promise<UnlistenFn> =>
  listen<CleanupProgress>(CLEANUP_PROGRESS_EVENT, (event) => handler(event.payload));

// ----------------------------------------------------------------- settings

export const getSettings = () => invoke<Settings>("get_settings");

export const updateSettings = (settings: Settings) =>
  invoke<Settings>("update_settings", { settings });

export const getUiState = () => invoke<UiState>("get_ui_state");

export const saveUiState = (uiState: UiState) =>
  invoke<void>("save_ui_state", { uiState });

export const getSystemAppearance = () =>
  invoke<SystemAppearance>("get_system_appearance");

export const getAdvisorKeyStatus = () =>
  invoke<AdvisorKeyStatus>("get_advisor_key_status");

export const saveAdvisorApiKey = (apiKey: string) =>
  invoke<AdvisorKeyStatus>("save_advisor_api_key", { apiKey });

export const deleteAdvisorApiKey = () =>
  invoke<AdvisorKeyStatus>("delete_advisor_api_key");

export const getExclusions = () => invoke<Exclusion[]>("get_exclusions");

export const createExclusion = (pattern: string, kind: ExclusionKind) =>
  invoke<Exclusion[]>("create_exclusion", { pattern, kind });

export const deleteExclusion = (id: number) =>
  invoke<Exclusion[]>("delete_exclusion", { id });

export const getRecoveredThisMonth = () =>
  invoke<number>("get_recovered_this_month");

export const clearLocalData = () => invoke<void>("clear_local_data");

export const getDataFolder = () => invoke<string>("get_data_folder");

export const onNavigate = (handler: (page: string) => void): Promise<UnlistenFn> =>
  listen<string>(NAVIGATE_EVENT, (event) => handler(event.payload));

export const onAutomationRun = (
  handler: (messages: string[]) => void,
): Promise<UnlistenFn> =>
  listen<string[]>(AUTOMATION_RUN_EVENT, (event) => handler(event.payload));

// ---------------------------------------------------- developer storage

export const scanDeveloperStorage = (root: string, includePackageCaches: boolean) =>
  invoke<DeveloperScanResult>("scan_developer_storage", {
    root,
    includePackageCaches,
  });

export const cancelDeveloperScan = () => invoke<void>("cancel_developer_scan");

export const getVirtualDisks = () => invoke<VirtualDisk[]>("get_virtual_disks");

export const buildDeveloperCleanupPlan = (artifactPaths: string[]) =>
  invoke<CleanupPlanResponse>("build_developer_cleanup_plan", { artifactPaths });

// -------------------------------------------------------- applications

export const getInstalledApps = () =>
  invoke<AppWithActivity[]>("get_installed_apps");

export const getAppFootprint = (appId: string) =>
  invoke<AppFootprint>("get_app_footprint", { appId });

export const preflightUninstall = (appId: string) =>
  invoke<UninstallPreflight>("preflight_uninstall", { appId });

export const startUninstall = (appId: string) =>
  invoke<UninstallStarted>("start_uninstall", { appId });

export const scanUninstallLeftovers = (appId: string) =>
  invoke<Leftover[]>("scan_uninstall_leftovers", { appId });

export const buildLeftoverCleanupPlan = (appId: string, paths: string[]) =>
  invoke<CleanupPlanResponse>("build_leftover_cleanup_plan", { appId, paths });

export const pollApplicationActivity = () =>
  invoke<number>("poll_application_activity");

export const clearApplicationActivity = () =>
  invoke<void>("clear_application_activity");

// ------------------------------------------------ duplicates & automation

export const findDuplicates = (root: string, minimumBytes: number, limit: number) =>
  invoke<DuplicateReport>("find_duplicates", { root, minimumBytes, limit });

export const cancelDuplicateScan = () => invoke<void>("cancel_duplicate_scan");

export const buildDuplicateCleanupPlan = (paths: string[]) =>
  invoke<CleanupPlanResponse>("build_duplicate_cleanup_plan", {
    selection: { paths },
  });

export const applyKeepStrategy = (group: DuplicateGroup, strategy: string) =>
  invoke<string[]>("apply_keep_strategy", { group, strategy });

export const recordGrowthSnapshot = (root: string) =>
  invoke<number>("record_growth_snapshot", { root });

export const getGrowthHistory = (range: string) =>
  invoke<GrowthRow[]>("get_growth_history", { range });

export const getAlerts = () => invoke<Alert[]>("get_alerts");

export const getAutomationRules = () =>
  invoke<AutomationRule[]>("get_automation_rules");

export const createAutomationRule = (rule: Record<string, unknown>) =>
  invoke<AutomationRule[]>("create_automation_rule", { rule });

export const setRuleEnabled = (id: number, enabled: boolean) =>
  invoke<AutomationRule[]>("set_rule_enabled", { id, enabled });

export const deleteAutomationRule = (id: number) =>
  invoke<AutomationRule[]>("delete_automation_rule", { id });

export const getRuleHistory = (id: number) =>
  invoke<RuleRunRow[]>("get_rule_history", { id });

export const evaluateAutomationRules = () =>
  invoke<string[]>("evaluate_automation_rules");

export const getQuarantineItems = () =>
  invoke<QuarantineItem[]>("get_quarantine_items");

export const restoreQuarantineItem = (id: number) =>
  invoke<void>("restore_quarantine_item", { id });

export const purgeQuarantineItem = (id: number) =>
  invoke<void>("purge_quarantine_item", { id });

export const getQuarantineSize = () => invoke<number>("get_quarantine_size");

// -------------------------------------------------------------- knowledge

export const explainLocation = (path: string) =>
  invoke<Explanation>("explain_location", { path });

export const advisePath = (path: string) => invoke<AdvisorAnswer>("advise_path", { path });

export const researchAdvisorPath = (path: string) =>
  invoke<AdvisorResearchAnswer>("research_advisor_path", { path });

export const knowledgeEntryCount = () => invoke<number>("knowledge_entry_count");
