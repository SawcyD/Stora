/**
 * Mirrors the serde representations in `stora-core`.
 *
 * Every type here has a Rust counterpart; keep the two in sync when either
 * side changes.
 */

export type DriveType =
  | "fixed"
  | "removable"
  | "network"
  | "cdRom"
  | "ramDisk"
  | "unknown";

export interface DriveInfo {
  root: string;
  label: string;
  filesystem: string;
  totalBytes: number;
  freeBytes: number;
  driveType: DriveType;
  isRemovable: boolean;
}

export type ScanState =
  | "idle"
  | "preparing"
  | "scanning"
  | "paused"
  | "cancelling"
  | "completed"
  | "failed";

export interface ScanProgress {
  taskId: string;
  state: ScanState;
  root: string;
  filesScanned: number;
  foldersScanned: number;
  bytesAnalyzed: number;
  currentPath: string;
  errors: number;
  elapsedMs: number;
}

export interface ScanSummary {
  scanId: number;
  root: string;
  startedAt: number;
  finishedAt: number | null;
  durationMs: number;
  filesScanned: number;
  foldersScanned: number;
  bytesAnalyzed: number;
  errors: number;
  state: ScanState;
}

export interface ScanStarted {
  taskId: string;
  scanId: number;
}

export type StorageCategoryId =
  | "applications"
  | "system"
  | "development"
  | "documents"
  | "games"
  | "temporaryFiles"
  | "other";

export interface CategoryBreakdown {
  category: StorageCategoryId;
  bytes: number;
  fileCount: number;
}

export interface FolderAggregate {
  path: string;
  parentPath: string | null;
  name: string;
  logicalSize: number;
  allocatedSize: number;
  fileCount: number;
  folderCount: number;
  modified: number | null;
  hasChildren: boolean;
}

export interface LargeFile {
  path: string;
  name: string;
  extension: string | null;
  logicalSize: number;
  allocatedSize: number;
  created: number | null;
  modified: number | null;
  accessed: number | null;
}

export type RiskLevel = "low" | "moderate" | "advanced" | "userReviewRequired";
export type CleanupTier = "safe" | "reviewRequired" | "advanced";

export interface CleanupCategory {
  id: string;
  name: string;
  explanation: string;
  tier: CleanupTier;
  risk: RiskLevel;
  prefersWindowsMechanism: boolean;
  learnMore: string | null;
}

export interface CleanupCategoryResult extends CleanupCategory {
  bytes: number;
  fileCount: number;
  folderCount: number;
  unavailableReason: string | null;
}

export interface CleanupItem {
  path: string;
  categoryId: string;
  size: number;
  isDirectory: boolean;
  modified: number | null;
}

export interface CleanupPlan {
  planId: string;
  createdAt: number;
  expiresAt: number;
  categories: CleanupCategoryResult[];
  items: CleanupItem[];
  totalBytes: number;
  fileCount: number;
  folderCount: number;
}

export interface CleanupPlanResponse {
  plan: CleanupPlan;
  defaultSelection: number[];
}

export type DeletionMethod =
  | "recycleBin"
  | "permanent"
  | "quarantine"
  | "windowsCleanup"
  | "applicationCleanup";

export type CleanupState =
  | "idle"
  | "preparing"
  | "awaitingApproval"
  | "cleaning"
  | "cancelling"
  | "completed"
  | "completedWithErrors"
  | "failed";

export interface CleanupProgress {
  taskId: string;
  state: CleanupState;
  completed: number;
  total: number;
  recoveredBytes: number;
  currentPath: string;
  errors: number;
  elapsedMs: number;
}

export interface CleanupItemError {
  path: string;
  code: string;
  message: string;
}

export interface CleanupResult {
  operationId: number;
  state: CleanupState;
  recoveredBytes: number;
  filesRemoved: number;
  filesSkipped: number;
  durationMs: number;
  method: DeletionMethod;
  errors: CleanupItemError[];
}

export interface CleanupHistoryEntry {
  operationId: number;
  startedAt: number;
  durationMs: number;
  categories: string[];
  filesSelected: number;
  filesRemoved: number;
  filesSkipped: number;
  recoveredBytes: number;
  method: DeletionMethod;
  errorCount: number;
  automationRule: string | null;
}

export type ExclusionKind = "file" | "folder" | "extension" | "volume" | "category";

export type ExclusionReason =
  | "userExclusion"
  | "protectedWindowsPath"
  | "activeApplication"
  | "systemComponent"
  | "reparsePoint"
  | "unsupportedVolume";

export interface Exclusion {
  id: number;
  pattern: string;
  kind: ExclusionKind;
  reason: ExclusionReason;
  createdAt: number;
}

export type ThemePreference = "system" | "light" | "dark";
export type TrayDoubleClickAction = "open" | "scan" | "cleanup" | "largeFiles";

export interface Settings {
  startWithWindows: boolean;
  minimizeToTray: boolean;
  closeToTray: boolean;
  showNotifications: boolean;
  theme: ThemePreference;
  trayDoubleClickAction: TrayDoubleClickAction;

  scanAllLocalDrives: boolean;
  followSymlinks: boolean;
  followJunctions: boolean;
  scanHiddenFiles: boolean;
  scanSystemFiles: boolean;
  scanConcurrency: number;
  useAllocatedSize: boolean;

  defaultDeletionMethod: string;
  preferRecycleBin: boolean;
  enableQuarantine: boolean;
  quarantineRetentionDays: number;
  confirmPermanentDeletion: boolean;
  showAdvancedCategories: boolean;

  trackApplicationLaunches: boolean;
  enableWindowsActivityEstimates: boolean;
  showConfidenceLevels: boolean;
  excludeBackgroundUtilities: boolean;

  detectDevelopmentProjects: boolean;
  scanPackageCaches: boolean;
  detectVirtualDisks: boolean;

  storeScanHistory: boolean;
  storeCleanupHistory: boolean;
  historyRetentionDays: number;

  debugLogging: boolean;
}

export interface UiState {
  selectedPage: string | null;
  sidebarCollapsed: boolean | null;
  selectedDrive: string | null;
  largeFileSort: string | null;
}

export interface SystemAppearance {
  accentColor: string | null;
}

/** A user-owned folder estimated from the latest completed scan. */
export interface RelocationCandidate {
  name: string;
  path: string;
  allocatedSize: number;
  fileCount: number;
  reason: string;
}

export interface RelocationCheck {
  label: string;
  passed: boolean;
  detail: string;
}

export interface RelocationPlan {
  source: string;
  destination: string;
  estimatedBytes: number;
  fileCount: number;
  canProceed: boolean;
  checks: RelocationCheck[];
}

export interface RelocationResult {
  source: string;
  destination: string;
  bytesMoved: number;
  filesMoved: number;
}

export interface AdvisorKeyStatus {
  /** The secret is never returned to the frontend. */
  saved: boolean;
}

/** Structured error returned by every failing command. */
export interface StoraError {
  code: string;
  message: string;
  path: string | null;
}

// ------------------------------------------------------ developer storage

export interface DetectedArtifact {
  path: string;
  name: string;
  bytes: number;
  fileCount: number;
  label: string;
  labelText: string;
  explanation: string;
  cleanupCommand: string | null;
  /** False for project source and user-created data, which are never removable. */
  removable: boolean;
}

export interface DetectedProject {
  path: string;
  name: string;
  kinds: string[];
  kindLabels: string[];
  artifacts: DetectedArtifact[];
  reclaimableBytes: number;
  lastModified: number | null;
}

export interface DeveloperScanResult {
  projects: DetectedProject[];
  totalsByArtifact: [string, number][];
  totalReclaimable: number;
  projectsScanned: number;
  packageCaches: DetectedArtifact[];
}

export interface VirtualDisk {
  path: string;
  name: string;
  owner: string;
  kind: string;
  kindLabel: string;
  bytes: number;
  lastModified: number | null;
  guidance: string;
  /** Always false. Stora does not delete virtual disks. */
  removable: boolean;
}

// ---------------------------------------------------------- applications

export type Confidence = "unknown" | "low" | "medium" | "high";

export type AppType =
  | "desktopApplication"
  | "storeApplication"
  | "game"
  | "portableApplication"
  | "backgroundUtility"
  | "driverOrSystemComponent"
  | "unknown";

export type ActivitySource =
  | "observedByStora"
  | "windowsEstimate"
  | "fileActivity"
  | "none";

export interface AppActivity {
  appId: string;
  executablePath: string | null;
  firstObserved: number | null;
  lastObserved: number | null;
  launchCount: number;
  source: ActivitySource;
  sourceLabel: string;
  confidence: Confidence;
  confidenceLabel: string;
  explanation: string;
}

export interface InstalledApp {
  id: string;
  name: string;
  publisher: string;
  version: string;
  reportedBytes: number | null;
  detectedBytes: number | null;
  installLocation: string | null;
  installDate: number | null;
  appType: AppType;
  appTypeLabel: string;
  uninstallCommand: string | null;
  source: string;
  confidence: Confidence;
  confidenceLabel: string;
  /** False for runtimes and drivers, which are never suggested for removal. */
  suggestable: boolean;
}

export interface AppWithActivity extends InstalledApp {
  activity: AppActivity;
  activityText: string;
}

export interface FootprintLocation {
  path: string;
  relationship: string;
  bytes: number;
  confidence: Confidence;
  confidenceLabel: string;
  reason: string;
}

export interface AppFootprint {
  appId: string;
  locations: FootprintLocation[];
  totalBytes: number;
}

// ------------------------------------------------ duplicates & automation

export interface DuplicateFile {
  path: string;
  name: string;
  size: number;
  modified: number | null;
  /** True when this path is a hard link to another entry in the group. */
  isHardLink: boolean;
}

export interface DuplicateGroup {
  hash: string;
  size: number;
  files: DuplicateFile[];
  reclaimableBytes: number;
}

export interface DuplicateReport {
  groups: DuplicateGroup[];
  totalReclaimable: number;
  filesCompared: number;
  filesFullyHashed: number;
}

export interface GrowthRow {
  path: string;
  name: string;
  currentBytes: number;
  changeBytes: number;
  hasBaseline: boolean;
  comparedAt: number;
}

export interface Alert {
  id: string;
  title: string;
  detail: string;
}

export type RuleTrigger = "weekly" | "lowFreeSpace" | "folderGrowth";
export type RuleAction = "notify" | "openCleanupReview" | "cleanSafeCategories";

export interface AutomationRule {
  id: number;
  name: string;
  enabled: boolean;
  trigger: RuleTrigger;
  action: RuleAction;
  weekday: number;
  freeSpaceThreshold: number;
  growthThreshold: number;
  watchedPath: string | null;
  categories: string[];
  minimumAgeDays: number;
  lastRun: number | null;
  consecutiveErrors: number;
}

export interface RuleRunRow {
  ranAt: number;
  outcome: string;
  detail: string;
  recoveredBytes: number;
}

export interface QuarantineItem {
  id: number;
  originalPath: string;
  quarantinePath: string;
  size: number;
  quarantinedAt: number;
  expiresAt: number | null;
}

// ------------------------------------------------------------ uninstall

export interface UninstallPreflight {
  appId: string;
  appName: string;
  methodLabel: string;
  canUninstall: boolean;
  /** Explains why, when canUninstall is false. */
  blockedReason: string | null;
  footprintBytes: number;
  locationCount: number;
}

export interface UninstallStarted {
  started: boolean;
  methodLabel: string;
  /** Shown verbatim. Never implies a restore point exists when it does not. */
  restorePointMessage: string;
  restorePointCreated: boolean;
}

export interface Leftover {
  path: string;
  relationship: string;
  bytes: number;
  /** False for registry keys, which Stora reports but never removes. */
  removable: boolean;
  reason: string;
}

// ------------------------------------------------------------ knowledge

export interface KnowledgeEntry {
  id: string;
  pattern: string;
  title: string;
  /** What puts data in this location. */
  writtenBy: string;
  /** What actually happens if it is removed. */
  ifRemoved: string;
  removable: boolean;
  sourceTitle: string;
  sourceUrl: string;
}

export interface Explanation {
  path: string;
  /** Null when nothing is known. The interface says so rather than guessing. */
  entry: KnowledgeEntry | null;
}

export type AdvisorVerdict = "doNotRemove" | "reviewFirst" | "unknown";

export interface AdvisorAnswer {
  path: string;
  verdict: AdvisorVerdict;
  summary: string;
  reasons: string[];
  sourceTitle: string | null;
  sourceUrl: string | null;
  localOnly: boolean;
}

export interface AdvisorResearchSource {
  title: string;
  url: string;
}

export interface AdvisorResearchAnswer {
  verdict: AdvisorVerdict;
  summary: string;
  reasons: string[];
  sources: AdvisorResearchSource[];
}
