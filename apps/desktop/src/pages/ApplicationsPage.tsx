import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Button,
  ComboBox,
  CommandBar,
  CommandGroup,
  ContentDialog,
  DataGrid,
  InfoBar,
  InfoRow,
  SearchBox,
  SectionHeader,
  SettingsSection,
  TeachingTip,
  Tooltip,
  type DataGridSort,
} from "@sawcy/memora-ui";

import { EmptyState, PageHeader } from "../components/common";
import UninstallFlow from "../components/UninstallFlow";
import { RefreshIcon } from "../components/icons";
import * as api from "../lib/api";
import { formatBytes, formatCount, formatTimestamp, shortenPath } from "../lib/format";
import type {
  AppFootprint,
  AppWithActivity,
  UninstallPreflight,
} from "../lib/types";
import { useApp } from "../state/AppContext";

const DAY = 86_400;

type FilterId =
  | "all"
  | "notObserved30"
  | "notObserved90"
  | "notObserved6Months"
  | "neverObserved"
  | "largerThan1Gb"
  | "games"
  | "storeApps";

const FILTERS: { value: FilterId; label: string }[] = [
  { value: "all", label: "All applications" },
  { value: "notObserved30", label: "Not observed in 30 days" },
  { value: "notObserved90", label: "Not observed in 90 days" },
  { value: "notObserved6Months", label: "Not observed in 6 months" },
  { value: "neverObserved", label: "Never observed by Stora" },
  { value: "largerThan1Gb", label: "Larger than 1 GB" },
  { value: "games", label: "Games" },
  { value: "storeApps", label: "Microsoft Store apps" },
];

export default function ApplicationsPage() {
  const { settings, saveSettings, notify, reportError } = useApp();

  const [apps, setApps] = useState<AppWithActivity[]>([]);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState<FilterId>("all");
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<DataGridSort>({
    columnId: "size",
    direction: "descending",
  });
  const [inspecting, setInspecting] = useState<AppWithActivity | null>(null);
  const [footprint, setFootprint] = useState<AppFootprint | null>(null);
  const [uninstalling, setUninstalling] = useState<{
    app: AppWithActivity;
    preflight: UninstallPreflight;
  } | null>(null);
  const [showTrackingTip, setShowTrackingTip] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      setApps(await api.getInstalledApps());
    } catch (error) {
      reportError(error);
    } finally {
      setLoading(false);
    }
  }, [reportError]);

  useEffect(() => {
    void load();
  }, [load]);

  // Poll for launches only while the user has tracking switched on.
  useEffect(() => {
    if (!settings?.trackApplicationLaunches) return;

    const tick = () => void api.pollApplicationActivity().catch(() => undefined);
    tick();
    // A coarse interval: this is not a per-second sweep of every process.
    const timer = window.setInterval(tick, 20_000);
    return () => window.clearInterval(timer);
  }, [settings?.trackApplicationLaunches]);

  const inspect = async (app: AppWithActivity) => {
    setInspecting(app);
    setFootprint(null);
    try {
      setFootprint(await api.getAppFootprint(app.id));
    } catch (error) {
      reportError(error);
    }
  };

  const beginUninstall = async (app: AppWithActivity) => {
    try {
      // Measure the footprint and resolve the method before asking, so the
      // dialog states what will actually happen rather than guessing.
      const preflight = await api.preflightUninstall(app.id);
      setUninstalling({ app, preflight });
    } catch (error) {
      reportError(error);
    }
  };

  const now = Math.floor(Date.now() / 1000);

  const visible = useMemo(() => {
    const term = search.trim().toLowerCase();

    return apps
      .filter((app) => {
        if (settings?.excludeBackgroundUtilities && app.appType === "backgroundUtility") {
          return false;
        }
        if (term && !`${app.name} ${app.publisher}`.toLowerCase().includes(term)) {
          return false;
        }

        // Runtimes and drivers never appear under a "potentially unused"
        // filter, however idle they look.
        const unusedFilter =
          filter === "notObserved30" ||
          filter === "notObserved90" ||
          filter === "notObserved6Months" ||
          filter === "neverObserved";
        if (unusedFilter && !app.suggestable) return false;

        const size = app.detectedBytes ?? app.reportedBytes ?? 0;
        const last = app.activity.lastObserved;

        switch (filter) {
          case "notObserved30":
            return last === null || now - last >= 30 * DAY;
          case "notObserved90":
            return last === null || now - last >= 90 * DAY;
          case "notObserved6Months":
            return last === null || now - last >= 182 * DAY;
          case "neverObserved":
            // Keyed on the source, not the count: an application with only a
            // Windows estimate was still never observed by Stora itself.
            return app.activity.source !== "observedByStora";
          case "largerThan1Gb":
            return size >= 1024 * 1024 * 1024;
          case "games":
            return app.appType === "game";
          case "storeApps":
            return app.appType === "storeApplication";
          default:
            return true;
        }
      })
      .sort((a, b) => {
        const direction = sort.direction === "ascending" ? 1 : -1;
        switch (sort.columnId) {
          case "name":
            return a.name.localeCompare(b.name) * direction;
          case "publisher":
            return a.publisher.localeCompare(b.publisher) * direction;
          case "installed":
            return ((a.installDate ?? 0) - (b.installDate ?? 0)) * direction;
          case "activity":
            return (
              ((a.activity.lastObserved ?? 0) - (b.activity.lastObserved ?? 0)) * direction
            );
          default:
            return (
              ((a.detectedBytes ?? a.reportedBytes ?? 0) -
                (b.detectedBytes ?? b.reportedBytes ?? 0)) *
              direction
            );
        }
      });
  }, [apps, filter, search, sort, settings?.excludeBackgroundUtilities, now]);

  const trackingOn = settings?.trackApplicationLaunches ?? false;
  const reviewCandidates = useMemo(
    () =>
      apps
        .filter((app) => {
          const size = app.detectedBytes ?? app.reportedBytes ?? 0;
          const last = app.activity.lastObserved;
          return (
            app.suggestable &&
            last !== null &&
            now - last >= 90 * DAY &&
            size >= 1024 * 1024 * 1024
          );
        })
        .sort(
          (a, b) =>
            (b.detectedBytes ?? b.reportedBytes ?? 0) -
            (a.detectedBytes ?? a.reportedBytes ?? 0),
        )
        .slice(0, 5),
    [apps, now],
  );

  return (
    <>
      <PageHeader
        title="Applications"
        description="Installed software, with the evidence behind every figure."
      />

      {!trackingOn ? (
        <div className="page-section">
          <InfoBar
            tone="info"
            title="Launch tracking is off"
            message="Without it, Stora has no record of what you actually use, so activity below reads as “No reliable activity data”. Windows does not keep one dependable last-opened value, so Stora will not invent one."
            action={
              <Button
                onClick={() => {
                  if (settings) {
                    // Turning both on together: Stora starts watching from
                    // now, and Windows' own record fills in the past.
                    void saveSettings({
                      ...settings,
                      trackApplicationLaunches: true,
                      enableWindowsActivityEstimates: true,
                    });
                    setShowTrackingTip(true);
                  }
                }}
              >
                Turn on tracking
              </Button>
            }
          />
        </div>
      ) : null}

      {showTrackingTip ? (
        <div className="page-section">
          <TeachingTip
            title="What tracking records, and what it misses"
            onDismiss={() => setShowTrackingTip(false)}
          >
            Stora notes when a program starts, on this device only. Nothing is uploaded.
            It samples running processes periodically rather than hooking every start, so
            something that opens and closes between samples may be missed. Windows' own
            shell record fills in history from before today, but only for programs
            launched from the Start menu, desktop, or File Explorer — anything started
            from a terminal or a game launcher stays invisible to it. An application will
            never show more use than it actually had.
          </TeachingTip>
        </div>
      ) : null}

      <CommandBar>
        <CommandGroup>
          <ComboBox
            label="Filter"
            value={filter}
            options={FILTERS}
            onChange={(value) => setFilter(value as FilterId)}
          />
          <SearchBox
            label="Search applications"
            placeholder="Search by name or publisher"
            value={search}
            onChange={setSearch}
          />
        </CommandGroup>
        <CommandGroup>
          <Button variant="subtle" onClick={() => void load()}>
            <RefreshIcon /> Refresh
          </Button>
        </CommandGroup>
      </CommandBar>

      {trackingOn ? (
        <section className="page-section">
          <SectionHeader>Worth reviewing</SectionHeader>
          <p className="text-secondary" style={{ marginTop: 0 }}>
            Large applications with no activity evidence in 90 days. This is a review
            list, not a claim that an application is unused.
          </p>
          {reviewCandidates.length === 0 ? (
            <SettingsSection>
              <EmptyState
                title="No review candidates yet"
                detail="Stora only lists an application here when it has activity evidence, is large, and is not a protected runtime or driver."
              />
            </SettingsSection>
          ) : (
            <DataGrid
              ariaLabel="Applications worth reviewing"
              rows={reviewCandidates}
              rowKey={(row) => row.id}
              columns={[
                {
                  id: "app",
                  header: "Application",
                  render: (row) => (
                    <div>
                      <div>{row.name}</div>
                      <div className="text-secondary">{row.activity.sourceLabel}</div>
                    </div>
                  ),
                },
                {
                  id: "size",
                  header: "Size",
                  width: 110,
                  align: "end",
                  render: (row) => (
                    <span className="numeric">
                      {formatBytes(row.detectedBytes ?? row.reportedBytes ?? 0)}
                    </span>
                  ),
                },
                {
                  id: "observed",
                  header: "Activity evidence",
                  width: 190,
                  render: (row) => formatTimestamp(row.activity.lastObserved),
                },
                {
                  id: "actions",
                  header: "",
                  width: 180,
                  render: (row) => (
                    <div className="row">
                      <Button variant="subtle" onClick={() => void inspect(row)}>
                        Footprint
                      </Button>
                      <Button variant="subtle" onClick={() => void beginUninstall(row)}>
                        Review
                      </Button>
                    </div>
                  ),
                },
              ]}
            />
          )}
        </section>
      ) : null}

      {loading ? (
        <EmptyState title="Reading installed applications…" />
      ) : (
        <DataGrid
          ariaLabel="Installed applications"
          rows={visible}
          rowKey={(row) => row.id}
          sort={sort}
          onSortChange={setSort}
          emptyMessage={
            apps.length === 0
              ? "No installed applications were found."
              : "No applications match these filters."
          }
          columns={[
            {
              id: "name",
              header: "Application",
              sortable: true,
              render: (row) => (
                <div>
                  <div>{row.name}</div>
                  <div className="text-secondary">
                    {row.appTypeLabel}
                    {row.version ? ` · ${row.version}` : ""}
                  </div>
                </div>
              ),
            },
            {
              id: "publisher",
              header: "Publisher",
              sortable: true,
              width: 180,
              render: (row) => row.publisher || "—",
            },
            {
              id: "size",
              header: "Size",
              sortable: true,
              width: 110,
              align: "end",
              render: (row) => (
                <Tooltip
                  content={
                    row.detectedBytes !== null
                      ? "Measured by Stora"
                      : row.reportedBytes !== null
                        ? "Reported by the installer"
                        : "No size information available"
                  }
                >
                  <span className="numeric">
                    {row.detectedBytes !== null
                      ? formatBytes(row.detectedBytes)
                      : row.reportedBytes !== null
                        ? formatBytes(row.reportedBytes)
                        : "—"}
                  </span>
                </Tooltip>
              ),
            },
            {
              id: "activity",
              header: "Activity",
              sortable: true,
              width: 230,
              render: (row) => (
                <Tooltip content={row.activity.explanation}>
                  <span>
                    {row.activityText}
                    {settings?.showConfidenceLevels ? (
                      <span className="text-secondary">
                        {" "}
                        · {row.activity.confidenceLabel}
                      </span>
                    ) : null}
                  </span>
                </Tooltip>
              ),
            },
            {
              id: "installed",
              header: "Installed",
              sortable: true,
              width: 130,
              render: (row) => (
                <span className="numeric">
                  {row.installDate ? formatTimestamp(row.installDate) : "—"}
                </span>
              ),
            },
            {
              id: "actions",
              header: "",
              width: 180,
              render: (row) => (
                <div className="row">
                  <Button variant="subtle" onClick={() => void inspect(row)}>
                    Footprint
                  </Button>
                  <Button
                    variant="subtle"
                    disabled={!row.suggestable}
                    onClick={() => void beginUninstall(row)}
                  >
                    Uninstall
                  </Button>
                </div>
              ),
            },
          ]}
        />
      )}

      <ContentDialog
        open={inspecting !== null}
        title={inspecting ? `${inspecting.name} — storage footprint` : ""}
        cancelText="Close"
        onCancel={() => {
          setInspecting(null);
          setFootprint(null);
        }}
      >
        {footprint === null ? (
          <EmptyState title="Measuring…" />
        ) : footprint.locations.length === 0 ? (
          <EmptyState
            title="No folders could be attributed to this application"
            detail="Stora only links a folder when there is real evidence for it, rather than guessing from a partial name match."
          />
        ) : (
          <div className="stack">
            <InfoRow
              label="Total detected"
              value={formatBytes(footprint.totalBytes)}
              help="Only folders Stora could attribute with stated evidence."
            />
            <SectionHeader>Locations</SectionHeader>
            {footprint.locations.map((location) => (
              <div
                key={location.path}
                style={{
                  padding: "8px 0",
                  borderTop: "1px solid var(--memora-stroke-surface)",
                }}
              >
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <strong style={{ fontSize: 13 }}>{location.relationship}</strong>
                  <span className="numeric">{formatBytes(location.bytes)}</span>
                </div>
                <div className="path-text">{shortenPath(location.path, 64)}</div>
                <div className="text-secondary" style={{ marginTop: 2 }}>
                  <span className="badge">{location.confidenceLabel} confidence</span>{" "}
                  {location.reason}
                </div>
              </div>
            ))}
          </div>
        )}
      </ContentDialog>

      {uninstalling ? (
        <UninstallFlow
          app={uninstalling.app}
          preflight={uninstalling.preflight}
          onClose={() => setUninstalling(null)}
          onFinished={() => void load()}
          notify={notify}
          reportError={reportError}
        />
      ) : null}

      <section className="page-section">
        <SectionHeader>How activity is reported</SectionHeader>
        <SettingsSection>
          <InfoRow
            label="Last observed by Stora"
            value="High confidence"
            help="Stora directly observed this application launch."
          />
          <InfoRow
            label="Windows activity estimate"
            value="Medium confidence"
            help="Windows recorded this program being launched from the Start menu, desktop, or File Explorer. It cannot see launches from a terminal or a game launcher, so an application missing from it has not been shown to be unused."
          />
          <InfoRow
            label="File activity only"
            value="Low confidence"
            help="Often reflects an update rather than someone using the application."
          />
          <InfoRow
            label="No reliable activity data"
            value="Unknown"
            help="Stora would rather say nothing than guess."
          />
        </SettingsSection>
        <p className="text-secondary" style={{ marginTop: 6 }}>
          {formatCount(apps.length)} applications found. Runtimes, redistributables, and
          drivers are never listed as potentially unused: nothing launches them directly,
          so they always look idle, yet other software depends on them.
        </p>
      </section>
    </>
  );
}
