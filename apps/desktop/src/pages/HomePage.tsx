import { useEffect, useState } from "react";
import {
  Button,
  ComboBox,
  CommandBar,
  CommandGroup,
  InfoRow,
  ProgressBar,
  SectionHeader,
  SettingsSection,
} from "@sawcy/memora-ui";

import { BreakdownRow, CapacityBar, EmptyState, PageHeader } from "../components/common";
import * as api from "../lib/api";
import {
  categoryLabel,
  formatBytes,
  formatCount,
  formatDuration,
  formatElapsed,
  formatPercent,
  formatTimestamp,
  shortenPath,
} from "../lib/format";
import type { CategoryBreakdown } from "../lib/types";
import { useApp } from "../state/AppContext";

export default function HomePage() {
  const {
    drives,
    selectedDrive,
    selectDrive,
    scanSummary,
    scanProgress,
    isScanning,
    startScan,
    pauseScan,
    resumeScan,
    cancelScan,
    setPage,
    scanRevision,
    reportError,
  } = useApp();

  const [breakdown, setBreakdown] = useState<CategoryBreakdown[]>([]);
  const [recoveredThisMonth, setRecoveredThisMonth] = useState<number | null>(null);

  useEffect(() => {
    if (!selectedDrive || !scanSummary) {
      setBreakdown([]);
      return;
    }
    let cancelled = false;

    api
      .getStorageBreakdown(selectedDrive.root)
      .then((result) => {
        if (!cancelled) setBreakdown(result);
      })
      .catch(() => {
        if (!cancelled) setBreakdown([]);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedDrive, scanSummary, scanRevision]);

  useEffect(() => {
    api
      .getRecoveredThisMonth()
      .then(setRecoveredThisMonth)
      .catch(() => setRecoveredThisMonth(null));
  }, [scanRevision]);

  if (!selectedDrive) {
    return (
      <>
        <PageHeader title="Home" />
        <EmptyState
          title="No local drives were found"
          detail="Stora analyzes fixed and removable volumes. Connect a drive, then refresh."
        />
      </>
    );
  }

  const paused = scanProgress?.state === "paused";
  const cancelling = scanProgress?.state === "cancelling";
  const maxCategoryBytes = breakdown.reduce((max, item) => Math.max(max, item.bytes), 0);

  return (
    <>
      <PageHeader
        title="Home"
        description="Understand your storage."
        actions={
          drives.length > 1 ? (
            <ComboBox
              label="Drive"
              value={selectedDrive.root}
              options={drives.map((drive) => ({
                value: drive.root,
                label: `${drive.label} (${drive.root.replace("\\", "")})`,
              }))}
              onChange={(root) => selectDrive(String(root))}
            />
          ) : null
        }
      />

      <section className="page-section" aria-labelledby="drive-heading">
        <SectionHeader id="drive-heading">
          {selectedDrive.label} ({selectedDrive.root.replace("\\", "")})
        </SectionHeader>
        <SettingsSection>
          <div style={{ padding: "12px 14px" }}>
            <CapacityBar
              usedBytes={selectedDrive.totalBytes - selectedDrive.freeBytes}
              totalBytes={selectedDrive.totalBytes}
              label={selectedDrive.label}
            />
            <p className="text-secondary numeric" style={{ margin: "10px 0 0" }}>
              {formatBytes(selectedDrive.totalBytes)} total ·{" "}
              {formatPercent(
                ((selectedDrive.totalBytes - selectedDrive.freeBytes) /
                  Math.max(selectedDrive.totalBytes, 1)) *
                  100,
              )}{" "}
              in use · {selectedDrive.filesystem || "Unknown filesystem"}
            </p>
          </div>
        </SettingsSection>
      </section>

      {isScanning ? (
        <section className="page-section" aria-labelledby="scan-heading">
          <SectionHeader id="scan-heading">
            {cancelling
              ? "Stopping scan"
              : paused
                ? "Scan paused"
                : `Scanning ${scanProgress?.root ?? ""}`}
          </SectionHeader>
          <SettingsSection>
            <div style={{ padding: "12px 14px" }}>
              <ProgressBar
                indeterminate={!paused}
                value={paused ? 0 : undefined}
                label={cancelling ? "Stopping" : paused ? "Paused" : "Scanning"}
              />
              <div className="scan-status" role="status" aria-live="polite">
                <span className="scan-status__dot" aria-hidden="true" />
                <span>
                  {cancelling
                    ? "Stopping after the current filesystem operation finishes."
                    : paused
                    ? "Scan is paused. No files are being read."
                    : (scanProgress?.filesScanned ?? 0) === 0 &&
                        (scanProgress?.foldersScanned ?? 0) === 0
                      ? "Starting scan — Windows is opening the first folder."
                      : "Scanning actively. Counts update as folders are read."}
                </span>
              </div>
              <div className="progress-readout">
                <InfoRow
                  label="Files scanned"
                  value={formatCount(scanProgress?.filesScanned ?? 0)}
                />
                <InfoRow
                  label="Folders scanned"
                  value={formatCount(scanProgress?.foldersScanned ?? 0)}
                />
                <InfoRow
                  label="Data analyzed"
                  value={formatBytes(scanProgress?.bytesAnalyzed ?? 0)}
                />
                <InfoRow
                  label="Elapsed"
                  value={formatElapsed(scanProgress?.elapsedMs ?? 0)}
                />
              </div>
              <p className="path-text" style={{ marginTop: 8 }}>
                {shortenPath(scanProgress?.currentPath ?? "")}
              </p>

              <CommandBar>
                <CommandGroup>
                  <Button
                    disabled={cancelling}
                    onClick={() => (paused ? resumeScan() : pauseScan())}
                  >
                    {paused ? "Resume" : "Pause"}
                  </Button>
                  <Button variant="subtle" disabled={cancelling} onClick={cancelScan}>
                    Cancel
                  </Button>
                </CommandGroup>
              </CommandBar>
            </div>
          </SettingsSection>
        </section>
      ) : (
        <CommandBar>
          <CommandGroup>
            <Button variant="primary" onClick={() => void startScan().catch(reportError)}>
              Scan storage
            </Button>
            <Button onClick={() => setPage("cleanup")}>Review cleanup</Button>
            <Button variant="subtle" onClick={() => setPage("largeFiles")}>
              View largest files
            </Button>
          </CommandGroup>
        </CommandBar>
      )}

      <section className="page-section" aria-labelledby="breakdown-heading">
        <SectionHeader id="breakdown-heading">Storage breakdown</SectionHeader>
        <SettingsSection>
          {breakdown.length === 0 ? (
            <EmptyState
              title="No scan results yet"
              detail="Scan this drive to see what is using its storage."
            />
          ) : (
            <div className="breakdown">
              {breakdown.map((item) => (
                <BreakdownRow
                  key={item.category}
                  label={categoryLabel(item.category)}
                  bytes={item.bytes}
                  maxBytes={maxCategoryBytes}
                  onSelect={() =>
                    setPage(item.category === "temporaryFiles" ? "cleanup" : "storage")
                  }
                />
              ))}
            </div>
          )}
        </SettingsSection>
      </section>

      <section className="page-section" aria-labelledby="summary-heading">
        <SectionHeader id="summary-heading">Summary</SectionHeader>
        <SettingsSection>
          <InfoRow
            label="Last scan"
            value={scanSummary ? formatTimestamp(scanSummary.startedAt) : "Never"}
          />
          <InfoRow
            label="Scanned files"
            value={scanSummary ? formatCount(scanSummary.filesScanned) : "—"}
          />
          <InfoRow
            label="Scan duration"
            value={scanSummary ? formatDuration(scanSummary.durationMs) : "—"}
          />
          <InfoRow
            label="Locations skipped"
            value={scanSummary ? formatCount(scanSummary.errors) : "—"}
            help="Folders Windows did not allow Stora to read. These are recorded, not hidden."
          />
          <InfoRow
            label="Space recovered this month"
            value={
              recoveredThisMonth === null ? "—" : formatBytes(recoveredThisMonth)
            }
            help="Counts only files that were actually removed."
          />
          <InfoRow label="Automatic cleanup" value="Off" />
        </SettingsSection>
      </section>
    </>
  );
}
