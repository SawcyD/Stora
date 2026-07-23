/**
 * Shared application state.
 *
 * Presentation lives in the page components; this module owns data loading,
 * the scan lifecycle, and persisted UI preferences.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";

import * as api from "../lib/api";
import type {
  DriveInfo,
  ScanProgress,
  ScanSummary,
  Settings,
  UiState,
} from "../lib/types";

export type PageId =
  | "home"
  | "storage"
  | "cleanup"
  | "applications"
  | "largeFiles"
  | "duplicates"
  | "developer"
  | "drivePressure"
  | "history"
  | "automation"
  | "settings"
  | "about";

export interface Notice {
  id: number;
  tone: "info" | "success" | "warning" | "error";
  title: string;
  detail?: string;
}

interface AppContextValue {
  drives: DriveInfo[];
  selectedDrive: DriveInfo | null;
  selectDrive: (root: string) => void;
  refreshDrives: () => Promise<void>;
  /** Client timestamp of the most recent capacity refresh. */
  lastDriveRefresh: number | null;

  page: PageId;
  setPage: (page: PageId) => void;
  sidebarCollapsed: boolean;
  toggleSidebar: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;

  settings: Settings | null;
  saveSettings: (settings: Settings) => Promise<void>;
  /** Windows accent color, or null when it could not be read. */
  accentColor: string | null;

  scanProgress: ScanProgress | null;
  scanSummary: ScanSummary | null;
  isScanning: boolean;
  startScan: () => Promise<void>;
  pauseScan: () => Promise<void>;
  resumeScan: () => Promise<void>;
  cancelScan: () => Promise<void>;
  /** Incremented whenever new scan results become available. */
  scanRevision: number;

  notices: Notice[];
  notify: (notice: Omit<Notice, "id">) => void;
  dismissNotice: (id: number) => void;
  reportError: (error: unknown) => void;

  ready: boolean;
}

const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const context = useContext(AppContext);
  if (!context) {
    throw new Error("useApp must be used inside AppProvider");
  }
  return context;
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [drives, setDrives] = useState<DriveInfo[]>([]);
  const [lastDriveRefresh, setLastDriveRefresh] = useState<number | null>(null);
  const [selectedRoot, setSelectedRoot] = useState<string | null>(null);
  const [page, setPageState] = useState<PageId>("home");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [scanProgress, setScanProgress] = useState<ScanProgress | null>(null);
  const [scanSummary, setScanSummary] = useState<ScanSummary | null>(null);
  const [scanRevision, setScanRevision] = useState(0);
  const [notices, setNotices] = useState<Notice[]>([]);
  const [accentColor, setAccentColor] = useState<string | null>(null);
  const [ready, setReady] = useState(false);

  const noticeId = useRef(0);
  const activeTaskId = useRef<string | null>(null);
  // The scanner reports its own elapsed time, but Windows can spend a few
  // seconds opening a large directory before the first useful count exists.
  // Keep the clock visibly alive during that window so "0 files" never reads
  // like a stalled task.
  const scanStartedAt = useRef<number | null>(null);

  const notify = useCallback((notice: Omit<Notice, "id">) => {
    noticeId.current += 1;
    const id = noticeId.current;
    setNotices((current) => {
      // A burst of failures (an unreadable drive, say) must not bury the page
      // in stacked bars, so only the most recent few are kept.
      const deduped = current.filter(
        (existing) =>
          existing.title !== notice.title || existing.detail !== notice.detail,
      );
      return [...deduped, { ...notice, id }].slice(-3);
    });
  }, []);

  const dismissNotice = useCallback((id: number) => {
    setNotices((current) => current.filter((notice) => notice.id !== id));
  }, []);

  const reportError = useCallback(
    (error: unknown) => {
      const described = api.describeError(error);
      notify({ tone: "error", title: described.title, detail: described.detail });
    },
    [notify],
  );

  const refreshDrives = useCallback(async () => {
    try {
      const list = await api.listDrives();
      setDrives(list);
      setLastDriveRefresh(Date.now());
      setSelectedRoot((current) => {
        if (current && list.some((drive) => drive.root === current)) return current;
        return list[0]?.root ?? null;
      });
    } catch (error) {
      reportError(error);
    }
  }, [reportError]);

  // Initial load: settings, saved UI state, and the drive list.
  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        const [loadedSettings, uiState, appearance] = await Promise.all([
          api.getSettings(),
          api.getUiState(),
          api.getSystemAppearance(),
        ]);
        if (cancelled) return;

        setSettings(loadedSettings);
        if (uiState.selectedPage) setPageState(uiState.selectedPage as PageId);
        if (uiState.sidebarCollapsed !== null) {
          setSidebarCollapsed(uiState.sidebarCollapsed);
        }
        if (uiState.selectedDrive) setSelectedRoot(uiState.selectedDrive);

        // Handed to FluentProvider so controls use the real system accent.
        setAccentColor(appearance.accentColor);
      } catch (error) {
        if (!cancelled) reportError(error);
      }

      await refreshDrives();
      if (!cancelled) setReady(true);
    })();

    return () => {
      cancelled = true;
    };
  }, [refreshDrives, reportError]);

  // Drive capacity is cheap for Windows to report and reflects changes made
  // outside Stora too. Keep it live without triggering a costly folder scan.
  useEffect(() => {
    const refreshWhenVisible = () => {
      if (document.visibilityState === "visible") void refreshDrives();
    };
    const timer = window.setInterval(refreshWhenVisible, 10_000);
    document.addEventListener("visibilitychange", refreshWhenVisible);
    return () => {
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", refreshWhenVisible);
    };
  }, [refreshDrives]);

  // Load the newest completed scan whenever the selected drive changes.
  useEffect(() => {
    if (!selectedRoot) return;
    let cancelled = false;

    api
      .getScanSummary(selectedRoot)
      .then((summary) => {
        if (cancelled) return;
        setScanSummary(summary);
        setScanRevision((value) => value + 1);
      })
      .catch(() => {
        // A drive with no scan yet is an expected state, not an error.
        if (!cancelled) setScanSummary(null);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedRoot]);

  // Scan progress events.
  useEffect(() => {
    const unlisten = api.onScanProgress((progress) => {
      setScanProgress((current) => ({
        ...progress,
        // A locally-started clock may have advanced slightly further than a
        // progress event that was queued on the native side.
        elapsedMs: Math.max(
          progress.elapsedMs,
          current?.taskId === progress.taskId && scanStartedAt.current
            ? Date.now() - scanStartedAt.current
            : 0,
        ),
      }));

      const finished =
        progress.state === "completed" ||
        progress.state === "failed" ||
        progress.state === "idle";

      if (!finished) return;

      activeTaskId.current = null;
      scanStartedAt.current = null;

      if (progress.state === "completed") {
        notify({
          tone: "success",
          title: `Scan of ${progress.root} finished.`,
          detail: `${progress.filesScanned.toLocaleString()} files analyzed.`,
        });
      } else if (progress.state === "failed") {
        notify({
          tone: "error",
          title: "The scan could not be completed.",
          detail: progress.root,
        });
      }

      // Refresh the stored summary so every page picks up the new results.
      api
        .getScanSummary(progress.root)
        .then((summary) => {
          setScanSummary(summary);
          setScanRevision((value) => value + 1);
        })
        .catch(() => undefined);

      // Free-space figures change after a scan-and-clean cycle.
      void refreshDrives();
    });

    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [notify, refreshDrives]);

  // A scan has no honest percentage: the total number of files is unknown
  // until it has been traversed. This lightweight heartbeat makes progress
  // visible before the next native event arrives, without inventing a number.
  useEffect(() => {
    if (scanProgress?.state !== "scanning" || !scanStartedAt.current) return;

    const timer = window.setInterval(() => {
      setScanProgress((current) => {
        if (current?.state !== "scanning" || !scanStartedAt.current) return current;
        return {
          ...current,
          elapsedMs: Math.max(current.elapsedMs, Date.now() - scanStartedAt.current),
        };
      });
    }, 250);

    return () => window.clearInterval(timer);
  }, [scanProgress?.state]);

  // Tray menu navigation.
  useEffect(() => {
    const unlisten = api.onNavigate((target) => setPageState(target as PageId));
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  // Background rules run while the window is hidden. When it is open, surface
  // their recorded outcome as a compact in-app notice instead of leaving the
  // user to discover it later in Automation history.
  useEffect(() => {
    const unlisten = api.onAutomationRun((messages) => {
      for (const detail of messages) {
        notify({ tone: "info", title: "Automation checked", detail });
      }
      if (messages.length > 0) void refreshDrives();
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [notify, refreshDrives]);

  const persistUiState = useCallback((partial: Partial<UiState>) => {
    void api
      .saveUiState({
        selectedPage: partial.selectedPage ?? null,
        sidebarCollapsed: partial.sidebarCollapsed ?? null,
        selectedDrive: partial.selectedDrive ?? null,
        largeFileSort: partial.largeFileSort ?? null,
      })
      .catch(() => undefined);
  }, []);

  // Persist navigation state, debounced through an effect rather than on
  // every click, so rapid navigation does not thrash the database.
  useEffect(() => {
    if (!ready) return;
    const timer = window.setTimeout(() => {
      persistUiState({
        selectedPage: page,
        sidebarCollapsed,
        selectedDrive: selectedRoot,
      });
    }, 400);
    return () => window.clearTimeout(timer);
  }, [page, sidebarCollapsed, selectedRoot, ready, persistUiState]);

  const selectedDrive = useMemo(
    () => drives.find((drive) => drive.root === selectedRoot) ?? null,
    [drives, selectedRoot],
  );

  const isScanning =
    scanProgress?.state === "scanning" ||
    scanProgress?.state === "preparing" ||
    scanProgress?.state === "paused" ||
    scanProgress?.state === "cancelling";

  const startScan = useCallback(async () => {
    if (!selectedRoot) return;
    try {
      scanStartedAt.current = Date.now();
      const started = await api.startScan(selectedRoot);
      activeTaskId.current = started.taskId;
      setScanProgress((current) => {
        // The first native progress event can win the race with startScan().
        // Preserve it instead of replacing real counts with the empty state.
        if (current?.taskId === started.taskId) return current;
        return {
          taskId: started.taskId,
          state: "scanning",
          root: selectedRoot,
          filesScanned: 0,
          foldersScanned: 0,
          bytesAnalyzed: 0,
          currentPath: selectedRoot,
          errors: 0,
          elapsedMs: 0,
        };
      });
    } catch (error) {
      scanStartedAt.current = null;
      reportError(error);
    }
  }, [selectedRoot, reportError]);

  const withTask = useCallback(
    async (
      action: (taskId: string) => Promise<void>,
      optimisticState?: ScanProgress["state"],
    ) => {
      const taskId = activeTaskId.current ?? scanProgress?.taskId;
      if (!taskId) return;
      try {
        await action(taskId);
        if (optimisticState) {
          setScanProgress((current) =>
            current?.taskId === taskId ? { ...current, state: optimisticState } : current,
          );
        }
      } catch (error) {
        // Native tasks disappear as soon as they finish. This can happen while
        // a development reload preserved the React view, or in the narrow
        // interval between a scan finishing and its final event being shown.
        // Treat it as a stale view, not as a user-facing failure.
        if (String(error).includes("was not found")) {
          activeTaskId.current = null;
          scanStartedAt.current = null;
          setScanProgress((current) => (current?.taskId === taskId ? null : current));

          const root = scanProgress?.root ?? selectedRoot;
          if (root) {
            void api
              .getScanSummary(root)
              .then((summary) => {
                setScanSummary(summary);
                setScanRevision((value) => value + 1);
              })
              .catch(() => undefined);
          }
          notify({
            tone: "info",
            title: "That scan is no longer active.",
            detail: "The scan view was refreshed.",
          });
          return;
        }
        reportError(error);
      }
    },
    [scanProgress?.taskId, scanProgress?.root, selectedRoot, notify, reportError],
  );

  const value: AppContextValue = {
    drives,
    selectedDrive,
    selectDrive: setSelectedRoot,
    refreshDrives,
    lastDriveRefresh,

    page,
    setPage: setPageState,
    sidebarCollapsed,
    toggleSidebar: () => setSidebarCollapsed((value) => !value),
    setSidebarCollapsed,

    settings,
    accentColor,
    saveSettings: async (next) => {
      try {
        const saved = await api.updateSettings(next);
        setSettings(saved);
      } catch (error) {
        reportError(error);
      }
    },

    scanProgress,
    scanSummary,
    isScanning: Boolean(isScanning),
    startScan,
    pauseScan: () => withTask(api.pauseScan, "paused"),
    resumeScan: () => withTask(api.resumeScan, "scanning"),
    cancelScan: () => withTask(api.cancelScan, "cancelling"),
    scanRevision,

    notices,
    notify,
    dismissNotice,
    reportError,

    ready,
  };

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}
