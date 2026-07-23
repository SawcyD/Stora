import { useCallback, useMemo, useState } from "react";
import {
  Button,
  ComboBox,
  CommandBar,
  CommandGroup,
  ContentDialog,
  InfoBar,
  InfoRow,
  SectionHeader,
  SettingsSection,
  ToggleSwitch,
  Tooltip,
} from "@sawcy/memora-ui";

import { EmptyState, PageHeader } from "../components/common";
import * as api from "../lib/api";
import { formatBytes, formatCount, formatTimestamp, shortenPath } from "../lib/format";
import type { DuplicateGroup, DuplicateReport } from "../lib/types";
import { useApp } from "../state/AppContext";

const MEGABYTE = 1024 * 1024;

const MINIMUM_SIZES = [
  { value: 1 * MEGABYTE, label: "Larger than 1 MB" },
  { value: 10 * MEGABYTE, label: "Larger than 10 MB" },
  { value: 100 * MEGABYTE, label: "Larger than 100 MB" },
  { value: 1024 * MEGABYTE, label: "Larger than 1 GB" },
];

const STRATEGIES = [
  { value: "newest", label: "Keep newest" },
  { value: "oldest", label: "Keep oldest" },
  { value: "shortestPath", label: "Keep shortest path" },
];

export default function DuplicatesPage() {
  const { selectedDrive, scanSummary, notify, reportError, refreshDrives } = useApp();

  const [report, setReport] = useState<DuplicateReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [minimum, setMinimum] = useState(MINIMUM_SIZES[1].value);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);

  const root = selectedDrive?.root ?? null;

  const scan = useCallback(async () => {
    if (!root) return;
    setScanning(true);
    setSelected(new Set());
    try {
      setReport(await api.findDuplicates(root, minimum, 5000));
    } catch (error) {
      reportError(error);
    } finally {
      setScanning(false);
    }
  }, [root, minimum, reportError]);

  const toggle = (path: string, on: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (on) next.add(path);
      else next.delete(path);
      return next;
    });
  };

  const applyStrategy = async (group: DuplicateGroup, strategy: string) => {
    try {
      const paths = await api.applyKeepStrategy(group, strategy);
      setSelected((current) => {
        const next = new Set(current);
        // Clear this group first so switching strategy replaces rather than
        // accumulates.
        group.files.forEach((file) => next.delete(file.path));
        paths.forEach((path) => next.add(path));
        return next;
      });
    } catch (error) {
      reportError(error);
    }
  };

  const selectedBytes = useMemo(() => {
    if (!report) return 0;
    let total = 0;
    for (const group of report.groups) {
      for (const file of group.files) {
        if (selected.has(file.path)) total += file.size;
      }
    }
    return total;
  }, [report, selected]);

  const remove = async () => {
    setConfirming(false);
    try {
      const response = await api.buildDuplicateCleanupPlan([...selected]);
      const everyIndex = response.plan.items.map((_, index) => index);
      const outcome = await api.executeCleanupPlan(
        response.plan.planId,
        everyIndex,
        "recycleBin",
      );

      notify({
        tone: outcome.filesSkipped > 0 ? "warning" : "success",
        title: `Recovered ${formatBytes(outcome.recoveredBytes)}.`,
        detail: `${formatCount(outcome.filesRemoved)} copies moved to the Recycle Bin.`,
      });

      void refreshDrives();
      void scan();
    } catch (error) {
      reportError(error);
    }
  };

  if (!scanSummary) {
    return (
      <>
        <PageHeader title="Duplicates" />
        <EmptyState
          title="No scan results for this drive yet"
          detail="Duplicate detection compares files found by a storage scan. Run one from the Home page first."
        />
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Duplicates"
        description="Files verified byte-for-byte identical with a full SHA-256, not just a matching size."
      />

      <CommandBar>
        <CommandGroup>
          <ComboBox
            label="Minimum size"
            value={minimum}
            options={MINIMUM_SIZES}
            onChange={(value) => setMinimum(Number(value))}
          />
          <Button variant="primary" onClick={() => void scan()} disabled={scanning}>
            {scanning ? "Comparing…" : "Find duplicates"}
          </Button>
          {scanning ? (
            <Button
              variant="subtle"
              onClick={() => void api.cancelDuplicateScan().catch(reportError)}
            >
              Cancel
            </Button>
          ) : null}
        </CommandGroup>
      </CommandBar>

      {selected.size > 0 ? (
        <div className="selection-summary">
          <div className="selection-summary__figures">
            <span>
              Selected
              <strong>{formatBytes(selectedBytes)}</strong>
            </span>
            <span>
              Copies
              <strong>{formatCount(selected.size)}</strong>
            </span>
          </div>
          <div className="selection-summary__actions">
            <Button variant="subtle" onClick={() => setSelected(new Set())}>
              Clear selection
            </Button>
            <Button variant="primary" onClick={() => setConfirming(true)}>
              Move copies to Recycle Bin
            </Button>
          </div>
        </div>
      ) : null}

      {report === null ? (
        <EmptyState
          title="No comparison yet"
          detail="Stora groups files by size, samples each end, then fully hashes only the survivors — so a large drive is not read end to end."
        />
      ) : report.groups.length === 0 ? (
        <EmptyState
          title="No duplicates were found"
          detail={`${formatCount(report.filesCompared)} same-size files were compared and none were identical.`}
        />
      ) : (
        <>
          <div className="page-section">
            <InfoBar
              tone="info"
              title="Nothing is selected for you"
              message="Identical files often belong in separate backup or program locations on purpose. Choose what to keep yourself, or apply a rule per group."
            />
          </div>

          <p className="text-secondary" style={{ marginBottom: 8 }}>
            {formatCount(report.groups.length)} groups ·{" "}
            {formatBytes(report.totalReclaimable)} reclaimable ·{" "}
            {formatCount(report.filesFullyHashed)} files fully verified
          </p>

          {report.groups.map((group) => (
            <section className="page-section" key={group.hash}>
              <SectionHeader>
                {formatCount(group.files.length)} copies ·{" "}
                {formatBytes(group.size)} each · {formatBytes(group.reclaimableBytes)}{" "}
                reclaimable
              </SectionHeader>

              <CommandBar>
                <CommandGroup>
                  {STRATEGIES.map((strategy) => (
                    <Button
                      key={strategy.value}
                      variant="subtle"
                      onClick={() => void applyStrategy(group, strategy.value)}
                    >
                      {strategy.label}
                    </Button>
                  ))}
                </CommandGroup>
              </CommandBar>

              <SettingsSection>
                {group.files.map((file) => (
                  <div
                    key={file.path}
                    className="row"
                    style={{
                      justifyContent: "space-between",
                      padding: "8px 14px",
                      borderBottom: "1px solid var(--memora-stroke-surface)",
                    }}
                  >
                    <div style={{ minWidth: 0, flex: 1 }}>
                      <Tooltip content={file.path}>
                        <span style={{ fontSize: 13 }}>{shortenPath(file.path, 62)}</span>
                      </Tooltip>
                      <div className="text-secondary">
                        Modified {formatTimestamp(file.modified)}
                        {file.isHardLink ? (
                          <> · Hard link — removing it would free no space</>
                        ) : null}
                      </div>
                    </div>
                    <div className="row">
                      <Button
                        variant="subtle"
                        onClick={() =>
                          void api.revealInExplorer(file.path).catch(reportError)
                        }
                      >
                        Open
                      </Button>
                      {file.isHardLink ? (
                        <span className="badge">Hard link</span>
                      ) : (
                        <ToggleSwitch
                          label={`Remove ${file.path}`}
                          checked={selected.has(file.path)}
                          onChange={(on) => toggle(file.path, on)}
                        />
                      )}
                    </div>
                  </div>
                ))}
              </SettingsSection>
            </section>
          ))}
        </>
      )}

      <ContentDialog
        open={confirming}
        title="Move the selected copies to the Recycle Bin?"
        primaryText="Move to Recycle Bin"
        cancelText="Cancel"
        onPrimary={remove}
        onCancel={() => setConfirming(false)}
      >
        <div className="stack">
          <InfoRow label="Copies" value={formatCount(selected.size)} />
          <InfoRow label="Selected size" value={formatBytes(selectedBytes)} />
          <p className="text-secondary" style={{ margin: 0 }}>
            Check that at least one copy of each file remains where you want it. These
            move to the Recycle Bin and can be restored from there.
          </p>
        </div>
      </ContentDialog>
    </>
  );
}
