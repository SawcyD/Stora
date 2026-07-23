import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Button,
  CommandBar,
  CommandGroup,
  ContentDialog,
  DataGrid,
  InfoBar,
  InfoRow,
  SectionHeader,
  SettingsRow,
  SettingsSection,
  ToggleSwitch,
  Tooltip,
} from "@sawcy/memora-ui";

import { BreakdownRow, EmptyState, PageHeader } from "../components/common";
import { RefreshIcon } from "../components/icons";
import * as api from "../lib/api";
import { formatBytes, formatCount, formatTimestamp, shortenPath } from "../lib/format";
import type { DetectedArtifact, DeveloperScanResult, VirtualDisk } from "../lib/types";
import { useApp } from "../state/AppContext";

/** Labels that are shown but can never be selected for removal. */
const PROTECTED_LABELS = new Set(["projectSource", "userCreatedData", "unknown"]);

export default function DeveloperPage() {
  const { selectedDrive, notify, reportError, refreshDrives } = useApp();

  const [result, setResult] = useState<DeveloperScanResult | null>(null);
  const [disks, setDisks] = useState<VirtualDisk[]>([]);
  const [scanning, setScanning] = useState(false);
  const [includeCaches, setIncludeCaches] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  const [commandsFor, setCommandsFor] = useState<DetectedArtifact | null>(null);

  const root = selectedDrive?.root ?? null;

  useEffect(() => {
    setResult(null);
    setSelected(new Set());
  }, [root]);

  const scan = useCallback(async () => {
    if (!root) return;
    setScanning(true);
    try {
      const outcome = await api.scanDeveloperStorage(root, includeCaches);
      setResult(outcome);
      setSelected(new Set());
    } catch (error) {
      reportError(error);
    } finally {
      setScanning(false);
    }
  }, [root, includeCaches, reportError]);

  const loadDisks = useCallback(async () => {
    try {
      setDisks(await api.getVirtualDisks());
    } catch (error) {
      reportError(error);
    }
  }, [reportError]);

  useEffect(() => {
    void loadDisks();
  }, [loadDisks]);

  /** Every artifact that may legitimately be selected. */
  const selectable = useMemo(() => {
    if (!result) return [] as DetectedArtifact[];
    const fromProjects = result.projects.flatMap((project) =>
      project.artifacts.filter((artifact) => artifact.removable),
    );
    return [...fromProjects, ...result.packageCaches];
  }, [result]);

  const selectedArtifacts = useMemo(
    () => selectable.filter((artifact) => selected.has(artifact.path)),
    [selectable, selected],
  );

  const selectedBytes = selectedArtifacts.reduce(
    (total, artifact) => total + artifact.bytes,
    0,
  );

  const toggle = (path: string, on: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (on) next.add(path);
      else next.delete(path);
      return next;
    });
  };

  const runCleanup = async () => {
    setConfirming(false);
    try {
      const response = await api.buildDeveloperCleanupPlan(
        selectedArtifacts.map((artifact) => artifact.path),
      );

      const everyIndex = response.plan.items.map((_, index) => index);
      const outcome = await api.executeCleanupPlan(
        response.plan.planId,
        everyIndex,
        "recycleBin",
      );

      notify({
        tone: outcome.filesSkipped > 0 ? "warning" : "success",
        title: `Recovered ${formatBytes(outcome.recoveredBytes)}.`,
        detail:
          outcome.filesSkipped > 0
            ? `${formatCount(outcome.filesSkipped)} files were in use and were skipped.`
            : `${formatCount(outcome.filesRemoved)} files moved to the Recycle Bin.`,
      });

      void refreshDrives();
      void scan();
    } catch (error) {
      reportError(error);
    }
  };

  if (!selectedDrive) {
    return (
      <>
        <PageHeader title="Developer" />
        <EmptyState title="Select a drive on the Home page first." />
      </>
    );
  }

  return (
    <>
      <PageHeader
        title="Developer"
        description="Build caches and dependency folders, confirmed against each project's own manifest."
      />

      <CommandBar>
        <CommandGroup>
          <Button variant="primary" onClick={() => void scan()} disabled={scanning}>
            {scanning ? "Scanning…" : `Scan ${selectedDrive.root.replace("\\", "")}`}
          </Button>
          {scanning ? (
            <Button
              variant="subtle"
              onClick={() => void api.cancelDeveloperScan().catch(reportError)}
            >
              Cancel
            </Button>
          ) : null}
        </CommandGroup>
        <CommandGroup>
          <ToggleSwitch
            label="Include package manager caches"
            checked={includeCaches}
            onChange={setIncludeCaches}
          />
          <Button variant="subtle" onClick={() => void loadDisks()}>
            <RefreshIcon /> Virtual disks
          </Button>
        </CommandGroup>
      </CommandBar>

      {result === null ? (
        <EmptyState
          title="No development scan yet"
          detail="Stora looks for projects proven by a manifest — Cargo.toml, package.json, pyproject.toml, a .sln, a .uproject, and so on. A folder named build or target is never treated as an artifact on its own."
        />
      ) : (
        <>
          {selectedArtifacts.length > 0 ? (
            <div className="selection-summary">
              <div className="selection-summary__figures">
                <span>
                  Selected
                  <strong>{formatBytes(selectedBytes)}</strong>
                </span>
                <span>
                  Caches
                  <strong>{formatCount(selectedArtifacts.length)}</strong>
                </span>
              </div>
              <div className="selection-summary__actions">
                <Button variant="subtle" onClick={() => setSelected(new Set())}>
                  Clear selection
                </Button>
                <Button variant="primary" onClick={() => setConfirming(true)}>
                  Clean selected caches
                </Button>
              </div>
            </div>
          ) : null}

          <section className="page-section">
            <SectionHeader>Development storage</SectionHeader>
            <SettingsSection>
              {result.totalsByArtifact.length === 0 ? (
                <EmptyState
                  title="No reclaimable development caches were found"
                  detail={`${formatCount(result.projectsScanned)} projects were inspected.`}
                />
              ) : (
                <div className="breakdown">
                  {result.totalsByArtifact.map(([name, bytes]) => (
                    <BreakdownRow
                      key={name}
                      label={name}
                      bytes={bytes}
                      maxBytes={result.totalsByArtifact[0][1]}
                    />
                  ))}
                </div>
              )}
            </SettingsSection>
            <p className="text-secondary" style={{ marginTop: 6 }}>
              {formatCount(result.projectsScanned)} projects inspected ·{" "}
              {formatBytes(result.totalReclaimable)} reclaimable in total
            </p>
          </section>

          {result.packageCaches.length > 0 ? (
            <section className="page-section">
              <SectionHeader>Package manager caches</SectionHeader>
              <SettingsSection>
                {result.packageCaches.map((cache) => (
                  <ArtifactRow
                    key={cache.path}
                    artifact={cache}
                    selected={selected.has(cache.path)}
                    onToggle={(on) => toggle(cache.path, on)}
                    onShowCommand={() => setCommandsFor(cache)}
                    onReveal={() =>
                      void api.revealInExplorer(cache.path).catch(reportError)
                    }
                  />
                ))}
              </SettingsSection>
            </section>
          ) : null}

          {result.projects.map((project) => (
            <section className="page-section" key={project.path}>
              <SectionHeader>
                {project.name} — {project.kindLabels.join(", ")}
              </SectionHeader>
              <p className="path-text" style={{ margin: "0 0 6px" }}>
                {shortenPath(project.path, 72)} · last modified{" "}
                {formatTimestamp(project.lastModified)}
              </p>
              <SettingsSection>
                {project.artifacts.length === 0 ? (
                  <EmptyState title="No sizeable caches in this project." />
                ) : (
                  project.artifacts.map((artifact) => (
                    <ArtifactRow
                      key={artifact.path}
                      artifact={artifact}
                      selected={selected.has(artifact.path)}
                      onToggle={(on) => toggle(artifact.path, on)}
                      onShowCommand={() => setCommandsFor(artifact)}
                      onReveal={() =>
                        void api.revealInExplorer(artifact.path).catch(reportError)
                      }
                    />
                  ))
                )}
              </SettingsSection>
            </section>
          ))}
        </>
      )}

      <section className="page-section">
        <SectionHeader>Virtual disks</SectionHeader>
        {disks.length === 0 ? (
          <SettingsSection>
            <EmptyState
              title="No virtual disks were found"
              detail="Stora looks in the usual WSL, Docker, Hyper-V, VMware, and VirtualBox locations."
            />
          </SettingsSection>
        ) : (
          <>
            <div style={{ marginBottom: 8 }}>
              <InfoBar
                tone="info"
                title="Stora never deletes a virtual disk"
                message="A virtual disk does not shrink when files inside it are deleted, and removing the file destroys the machine. Each entry below shows the supported way to reclaim its space."
              />
            </div>
            <DataGrid
              ariaLabel="Virtual disks"
              rows={disks}
              rowKey={(row) => row.path}
              columns={[
                {
                  id: "owner",
                  header: "Name",
                  render: (row) => (
                    <Tooltip content={row.path}>
                      <span>{row.owner}</span>
                    </Tooltip>
                  ),
                },
                { id: "kind", header: "Type", width: 160, render: (row) => row.kindLabel },
                {
                  id: "size",
                  header: "Virtual disk size",
                  width: 140,
                  align: "end",
                  render: (row) => (
                    <span className="numeric">{formatBytes(row.bytes)}</span>
                  ),
                },
                {
                  id: "modified",
                  header: "Last activity",
                  width: 150,
                  render: (row) => (
                    <span className="numeric">{formatTimestamp(row.lastModified)}</span>
                  ),
                },
                {
                  id: "actions",
                  header: "",
                  width: 200,
                  render: (row) => (
                    <div className="row">
                      <Button
                        variant="subtle"
                        onClick={() =>
                          void api.revealInExplorer(row.path).catch(reportError)
                        }
                      >
                        Open location
                      </Button>
                      <Button
                        variant="subtle"
                        onClick={() =>
                          notify({
                            tone: "info",
                            title: `How to reclaim space from ${row.owner}`,
                            detail: row.guidance,
                          })
                        }
                      >
                        Guidance
                      </Button>
                    </div>
                  ),
                },
              ]}
            />
          </>
        )}
      </section>

      <ContentDialog
        open={confirming}
        title="Clean the selected development caches?"
        primaryText="Move to Recycle Bin"
        cancelText="Cancel"
        onPrimary={runCleanup}
        onCancel={() => setConfirming(false)}
      >
        <div className="stack">
          <InfoRow label="Caches" value={formatCount(selectedArtifacts.length)} />
          <InfoRow label="Selected size" value={formatBytes(selectedBytes)} />
          <p className="text-secondary" style={{ margin: 0 }}>
            These files move to the Recycle Bin and can be restored from there. Your
            tools will rebuild what they need; a dependency folder will require a
            reinstall, which needs network access. Files in use are skipped.
          </p>
        </div>
      </ContentDialog>

      <ContentDialog
        open={commandsFor !== null}
        title={commandsFor ? `${commandsFor.name} — supported command` : ""}
        primaryText="Copy command"
        cancelText="Close"
        onPrimary={() => {
          if (commandsFor?.cleanupCommand) {
            void navigator.clipboard
              .writeText(commandsFor.cleanupCommand)
              .then(() => notify({ tone: "info", title: "Command copied." }))
              .catch(reportError);
          }
          setCommandsFor(null);
        }}
        onCancel={() => setCommandsFor(null)}
      >
        <div className="stack">
          <p style={{ margin: 0 }}>{commandsFor?.explanation}</p>
          {commandsFor?.cleanupCommand ? (
            <>
              <p className="text-secondary" style={{ margin: 0 }}>
                The tool's own command is usually safer than deleting files, because it
                keeps the tool's internal bookkeeping consistent. Stora shows it rather
                than running it for you:
              </p>
              <code
                style={{
                  display: "block",
                  padding: "8px 10px",
                  borderRadius: "var(--memora-radius-sm)",
                  border: "1px solid var(--memora-stroke)",
                  background: "var(--memora-control)",
                  fontFamily: "Consolas, monospace",
                  fontSize: 12,
                }}
              >
                {commandsFor.cleanupCommand}
              </code>
            </>
          ) : (
            <p className="text-secondary" style={{ margin: 0 }}>
              This cache has no official cleanup command. Removing the folder is the
              supported approach.
            </p>
          )}
        </div>
      </ContentDialog>
    </>
  );
}

interface ArtifactRowProps {
  artifact: DetectedArtifact;
  selected: boolean;
  onToggle: (on: boolean) => void;
  onShowCommand: () => void;
  onReveal: () => void;
}

function ArtifactRow({
  artifact,
  selected,
  onToggle,
  onShowCommand,
  onReveal,
}: ArtifactRowProps) {
  const isProtected = PROTECTED_LABELS.has(artifact.label);

  return (
    <SettingsRow
      title={artifact.name}
      description={artifact.explanation}
      note={
        <span className="cleanup-row__meta">
          <span className={`badge${isProtected ? " badge--review" : ""}`}>
            {artifact.labelText}
          </span>
          <span className="text-secondary numeric">
            {formatCount(artifact.fileCount)} files
          </span>
          {isProtected ? (
            <span className="text-secondary">Not offered for removal</span>
          ) : null}
        </span>
      }
      control={
        <span className="cleanup-row__control">
          <span className="cleanup-row__size">{formatBytes(artifact.bytes)}</span>
          <Button variant="subtle" onClick={onShowCommand}>
            Details
          </Button>
          <Button variant="subtle" onClick={onReveal}>
            Open
          </Button>
          {artifact.removable ? (
            <ToggleSwitch
              label={`Select ${artifact.name}`}
              checked={selected}
              onChange={onToggle}
            />
          ) : null}
        </span>
      }
    />
  );
}
