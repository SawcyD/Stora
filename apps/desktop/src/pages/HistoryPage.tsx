import { useCallback, useEffect, useState } from "react";
import {
  Button,
  CommandBar,
  CommandGroup,
  DataGrid,
  InfoRow,
  SectionHeader,
  SettingsSection,
  Tooltip,
} from "@sawcy/memora-ui";

import { EmptyState, PageHeader } from "../components/common";
import { RefreshIcon } from "../components/icons";
import * as api from "../lib/api";
import {
  formatBytes,
  formatCount,
  formatDuration,
  formatTimestamp,
  methodLabel,
  shortenPath,
} from "../lib/format";
import type { CleanupHistoryEntry, CleanupItemError, QuarantineItem } from "../lib/types";
import { useApp } from "../state/AppContext";

const CATEGORY_NAMES: Record<string, string> = {
  userTemp: "User temporary files",
  thumbnailCache: "Thumbnail cache",
  shaderCache: "DirectX shader cache",
  crashDumps: "Application crash dumps",
  errorReports: "Old error reports",
  windowsTemp: "System temporary files",
  deliveryOptimization: "Delivery Optimization files",
  browserCache: "Browser caches",
  downloads: "Downloads",
  oldInstallers: "Old installers",
  recycleBin: "Recycle Bin",
};

export default function HistoryPage() {
  const { reportError, notify } = useApp();

  const [entries, setEntries] = useState<CleanupHistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [expanded, setExpanded] = useState<number | null>(null);
  const [errors, setErrors] = useState<CleanupItemError[]>([]);
  const [quarantine, setQuarantine] = useState<QuarantineItem[]>([]);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const [history, held] = await Promise.all([
        api.getCleanupHistory(200),
        api.getQuarantineItems(),
      ]);
      setEntries(history);
      setQuarantine(held);
    } catch (error) {
      reportError(error);
    } finally {
      setLoading(false);
    }
  }, [reportError]);

  useEffect(() => {
    void load();
  }, [load]);

  const toggleDetails = async (entry: CleanupHistoryEntry) => {
    if (expanded === entry.operationId) {
      setExpanded(null);
      setErrors([]);
      return;
    }
    try {
      setErrors(
        entry.errorCount > 0 ? await api.getCleanupErrors(entry.operationId) : [],
      );
      setExpanded(entry.operationId);
    } catch (error) {
      reportError(error);
    }
  };

  const restore = async (item: QuarantineItem) => {
    try {
      await api.restoreQuarantineItem(item.id);
      notify({ tone: "success", title: "Item restored to its original location." });
      await load();
    } catch (error) {
      reportError(error);
    }
  };

  const copyReport = async (entry: CleanupHistoryEntry) => {
    const report = [
      `Stora cleanup — ${formatTimestamp(entry.startedAt)}`,
      `Categories: ${entry.categories.map(categoryName).join(", ") || "None"}`,
      `Method: ${methodLabel(entry.method)}`,
      `Files removed: ${formatCount(entry.filesRemoved)}`,
      `Files skipped: ${formatCount(entry.filesSkipped)}`,
      `Space recovered: ${formatBytes(entry.recoveredBytes)}`,
      `Duration: ${formatDuration(entry.durationMs)}`,
    ].join("\n");

    try {
      await navigator.clipboard.writeText(report);
      notify({ tone: "info", title: "Report copied to the clipboard." });
    } catch (error) {
      reportError(error);
    }
  };

  return (
    <>
      <PageHeader
        title="History"
        description="Every cleanup Stora has performed on this device."
      />

      <CommandBar>
        <CommandGroup>
          <Button variant="subtle" onClick={() => void load()}>
            <RefreshIcon /> Refresh
          </Button>
        </CommandGroup>
      </CommandBar>

      <section className="page-section">
        <SectionHeader>Recoverable items</SectionHeader>
        <p className="text-secondary" style={{ marginTop: 0 }}>
          Files Stora moved to quarantine are shown here until they expire or you remove them.
          Windows cannot provide a complete history of files deleted outside Stora.
        </p>
        {quarantine.length === 0 ? (
          <SettingsSection>
            <EmptyState
              title="Nothing is in quarantine"
              detail="Choose quarantine as a removal method to keep eligible files recoverable."
            />
          </SettingsSection>
        ) : (
          <DataGrid
            ariaLabel="Recoverable items"
            rows={quarantine}
            rowKey={(row) => row.id}
            columns={[
              {
                id: "path",
                header: "Original location",
                render: (row) => (
                  <Tooltip content={row.originalPath}>
                    <span>{shortenPath(row.originalPath, 52)}</span>
                  </Tooltip>
                ),
              },
              {
                id: "size",
                header: "Size",
                width: 110,
                align: "end",
                render: (row) => <span className="numeric">{formatBytes(row.size)}</span>,
              },
              {
                id: "held",
                header: "Held since",
                width: 170,
                render: (row) => formatTimestamp(row.quarantinedAt),
              },
              {
                id: "restore",
                header: "",
                width: 110,
                render: (row) => (
                  <Button variant="subtle" onClick={() => void restore(row)}>
                    Restore
                  </Button>
                ),
              },
            ]}
          />
        )}
      </section>

      {loading ? (
        <EmptyState title="Loading history…" />
      ) : entries.length === 0 ? (
        <EmptyState
          title="No cleanups yet"
          detail="Once you complete a cleanup, it will be recorded here with exactly what was removed."
        />
      ) : (
        entries.map((entry) => (
          <section className="page-section" key={entry.operationId}>
            <SectionHeader>{formatTimestamp(entry.startedAt)}</SectionHeader>
            <SettingsSection>
              <InfoRow
                label="Categories"
                value={entry.categories.map(categoryName).join(", ") || "None"}
              />
              <InfoRow
                label="Space recovered"
                value={formatBytes(entry.recoveredBytes)}
                help="Counts only files that were actually removed."
              />
              <InfoRow label="Files removed" value={formatCount(entry.filesRemoved)} />
              <InfoRow label="Files skipped" value={formatCount(entry.filesSkipped)} />
              <InfoRow label="Method" value={methodLabel(entry.method)} />
              <InfoRow label="Duration" value={formatDuration(entry.durationMs)} />
              {entry.automationRule ? (
                <InfoRow label="Automation rule" value={entry.automationRule} />
              ) : null}
            </SettingsSection>

            <CommandBar>
              <CommandGroup>
                <Button variant="subtle" onClick={() => void copyReport(entry)}>
                  Copy report
                </Button>
                {entry.errorCount > 0 ? (
                  <Button variant="subtle" onClick={() => void toggleDetails(entry)}>
                    {expanded === entry.operationId
                      ? "Hide skipped items"
                      : `View ${entry.errorCount} skipped item${entry.errorCount === 1 ? "" : "s"}`}
                  </Button>
                ) : null}
              </CommandGroup>
            </CommandBar>

            {expanded === entry.operationId && errors.length > 0 ? (
              <DataGrid
                ariaLabel={`Items skipped on ${formatTimestamp(entry.startedAt)}`}
                rows={errors}
                rowKey={(row) => row.path}
                columns={[
                  {
                    id: "path",
                    header: "Item",
                    render: (row) => (
                      <Tooltip content={row.path}>
                        <span>{shortenPath(row.path, 52)}</span>
                      </Tooltip>
                    ),
                  },
                  {
                    id: "reason",
                    header: "Reason",
                    width: 320,
                    render: (row) =>
                      api.describeError({
                        code: row.code,
                        message: row.message,
                        path: null,
                      }).title,
                  },
                ]}
              />
            ) : null}
          </section>
        ))
      )}
    </>
  );
}

function categoryName(id: string): string {
  return CATEGORY_NAMES[id] ?? id;
}
