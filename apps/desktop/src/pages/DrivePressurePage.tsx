import { useEffect, useState } from "react";
import {
  Button,
  InfoBar,
  InfoRow,
  SectionHeader,
  SettingsRow,
  SettingsSection,
} from "@sawcy/memora-ui";

import { CapacityBar, EmptyState, PageHeader } from "../components/common";
import * as api from "../lib/api";
import { formatBytes, formatPercent } from "../lib/format";
import type { DriveInfo, RelocationCandidate, RelocationPlan, RelocationResult } from "../lib/types";
import { useApp } from "../state/AppContext";

const GIGABYTE = 1024 * 1024 * 1024;

export default function DrivePressurePage() {
  const { drives, setPage, selectDrive, reportError, refreshDrives, lastDriveRefresh } = useApp();
  const target = choosePressuredDrive(drives);
  const [candidates, setCandidates] = useState<RelocationCandidate[]>([]);
  const [candidateState, setCandidateState] = useState<"loading" | "ready" | "needsScan">(
    "loading",
  );
  const [selectedCandidate, setSelectedCandidate] = useState<RelocationCandidate | null>(null);
  const [relocationPlan, setRelocationPlan] = useState<RelocationPlan | null>(null);
  const [planDestinationRoot, setPlanDestinationRoot] = useState<string | null>(null);
  const [planning, setPlanning] = useState(false);
  const [moveConfirmed, setMoveConfirmed] = useState(false);
  const [moveResult, setMoveResult] = useState<RelocationResult | null>(null);
  const destinations = target
    ? drives.filter((drive) => drive.root !== target.root && drive.freeBytes >= 5 * GIGABYTE)
    : [];

  useEffect(() => {
    if (!target) return;
    let cancelled = false;
    setCandidateState("loading");
    api
      .getRelocationCandidates(target.root)
      .then((result) => {
        if (cancelled) return;
        setCandidates(result);
        setSelectedCandidate(null);
        setRelocationPlan(null);
        setPlanDestinationRoot(null);
        setMoveConfirmed(false);
        setMoveResult(null);
        setCandidateState("ready");
      })
      .catch(() => {
        if (!cancelled) {
          setCandidates([]);
          setCandidateState("needsScan");
        }
      });
    return () => {
      cancelled = true;
    };
  }, [target?.root]);

  if (!target) {
    return (
      <>
        <PageHeader title="Drive pressure" description="Find safe ways to keep a drive comfortable." />
        <EmptyState title="No local drives were found" />
      </>
    );
  }

  const usedPercent = percentUsed(target);
  const underPressure = target.freeBytes < 20 * GIGABYTE || usedPercent >= 90;

  return (
    <>
      <PageHeader
        title="Drive pressure"
        description="Practical storage options, without moving system or application data automatically."
      />

      <section className="page-section">
        <SettingsSection>
          <div style={{ padding: "12px 14px" }}>
            <CapacityBar
              label={`${target.label} (${target.root.replace("\\", "")})`}
              usedBytes={target.totalBytes - target.freeBytes}
              totalBytes={target.totalBytes}
            />
            <p className="text-secondary numeric" style={{ margin: "10px 0 0" }}>
              {formatBytes(target.freeBytes)} available · {formatPercent(usedPercent)} in use
            </p>
            <p className="text-secondary" style={{ margin: "6px 0 0", fontSize: 12 }}>
              Live capacity refreshes every 10 seconds
              {lastDriveRefresh ? ` · updated ${new Date(lastDriveRefresh).toLocaleTimeString()}` : ""}.
            </p>
          </div>
        </SettingsSection>
      </section>

      {underPressure ? (
        <section className="page-section">
          <InfoBar
            tone="warning"
            title={`${target.root.replace("\\", "")} is running low on free space`}
            message="Stora will help you review safe options. It never silently moves AppData, Windows folders, games, or installed applications."
          />
        </section>
      ) : (
        <section className="page-section">
          <InfoBar
            tone="info"
            title={`${target.root.replace("\\", "")} has comfortable free space`}
            message="Use this view to stay ahead of growth before storage becomes urgent."
          />
        </section>
      )}

      <section className="page-section">
        <SectionHeader>Available destinations</SectionHeader>
        <SettingsSection>
          {destinations.length === 0 ? (
            <InfoRow
              label="No suitable destination drive"
              value="Review cleanup or add storage"
              help="A destination must have at least 5 GB free before Stora will suggest it."
            />
          ) : (
            destinations.map((drive) => (
              <InfoRow
                key={drive.root}
                label={`${drive.label} (${drive.root.replace("\\", "")})`}
                value={`${formatBytes(drive.freeBytes)} available`}
                help="Suitable for user-owned folders after a reviewed move plan is implemented."
              />
            ))
          )}
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>Move assistant — personal folders</SectionHeader>
        <SettingsSection>
          {candidateState === "loading" ? (
            <InfoRow label="Checking the latest scan" value="Loading…" />
          ) : candidateState === "needsScan" ? (
            <SettingsRow
              title="Scan this drive to prepare move suggestions"
              description="Stora uses completed scan results for its estimates; it does not start another filesystem walk here."
              control={
                <Button
                  onClick={() => {
                    selectDrive(target.root);
                    setPage("home");
                  }}
                >
                  Go to scan
                </Button>
              }
            />
          ) : candidates.length === 0 ? (
            <InfoRow
              label="No large personal folders found"
              value="Nothing suggested"
              help="Downloads, Documents, Pictures, and Videos are the only folders this preview considers. AppData, games, installed apps, Desktop, and Windows folders are blocked."
            />
          ) : (
            candidates.map((candidate) => (
              <SettingsRow
                key={candidate.path}
                title={`${candidate.name} · ${formatBytes(candidate.allocatedSize)}`}
                description={`${candidate.reason} ${candidate.fileCount.toLocaleString()} files estimated. This is a preview only; nothing is moved yet.`}
                control={
                  <span className="row">
                    <Button variant="subtle" onClick={() => void api.revealInExplorer(candidate.path)}>
                      Review folder
                    </Button>
                    <Button
                      onClick={() => {
                        setSelectedCandidate(candidate);
                        setRelocationPlan(null);
                        setPlanDestinationRoot(null);
                        setMoveConfirmed(false);
                        setMoveResult(null);
                      }}
                    >
                      Plan move
                    </Button>
                  </span>
                }
              />
            ))
          )}
        </SettingsSection>
        {selectedCandidate ? (
          <div className="move-plan" aria-live="polite">
            <p className="move-plan__title">Plan a move for {selectedCandidate.name}</p>
            <p className="text-secondary" style={{ margin: "0 0 10px", fontSize: 12 }}>
              Choose a destination. Stora will only create a reviewed proposal at this stage.
            </p>
            <div className="row">
              {destinations.map((drive) => (
                <Button
                  key={drive.root}
                  disabled={planning}
                  onClick={() => {
                    setPlanning(true);
                    api
                      .buildRelocationPlan(selectedCandidate.path, drive.root)
                      .then((plan) => {
                        setRelocationPlan(plan);
                        setPlanDestinationRoot(drive.root);
                        setMoveConfirmed(false);
                        setMoveResult(null);
                      })
                      .catch((error) => {
                        setRelocationPlan(null);
                        reportError(error);
                      })
                      .finally(() => setPlanning(false));
                  }}
                >
                  Plan for {drive.root.replace("\\", "")}
                </Button>
              ))}
            </div>
            {destinations.length === 0 ? (
              <InfoBar
                tone="warning"
                title="No destination drive is ready"
                message="Connect or free space on another local drive before planning a move."
              />
            ) : null}
            {relocationPlan ? (
              <div className="move-plan__result">
                <InfoBar
                  tone={relocationPlan.canProceed ? "info" : "warning"}
                  title={relocationPlan.canProceed ? "Move plan is ready for review" : "Move plan needs attention"}
                  message={`${formatBytes(relocationPlan.estimatedBytes)} across ${relocationPlan.fileCount.toLocaleString()} files would be copied to ${relocationPlan.destination}. Nothing has been moved.`}
                />
                {relocationPlan.checks.map((check) => (
                  <InfoRow
                    key={check.label}
                    label={check.label}
                    value={check.passed ? "Ready" : "Blocked"}
                    help={check.detail}
                  />
                ))}
                {relocationPlan.canProceed && planDestinationRoot ? (
                  <div className="move-plan__confirm">
                    <label>
                      <input
                        type="checkbox"
                        checked={moveConfirmed}
                        onChange={(event) => setMoveConfirmed(event.target.checked)}
                      />{" "}
                      I understand Stora will copy, verify, redirect this Windows folder, then
                      remove the original only if all earlier steps succeed.
                    </label>
                    <Button
                      variant="primary"
                      disabled={!moveConfirmed || planning}
                      onClick={() => {
                        if (
                          !window.confirm(
                            `Move ${selectedCandidate.name} to ${relocationPlan.destination}? Stora will copy and verify first.`,
                          )
                        ) {
                          return;
                        }
                        setPlanning(true);
                        api
                          .executeRelocation(selectedCandidate.path, planDestinationRoot)
                          .then((result) => {
                            setMoveResult(result);
                            void refreshDrives();
                          })
                          .catch((error) => {
                            setMoveResult(null);
                            reportError(error);
                          })
                          .finally(() => setPlanning(false));
                      }}
                    >
                      Copy, verify, and redirect
                    </Button>
                  </div>
                ) : null}
                {moveResult ? (
                  <InfoBar
                    tone="info"
                    title="Folder moved and redirected"
                    message={`${formatBytes(moveResult.bytesMoved)} across ${moveResult.filesMoved.toLocaleString()} files now lives at ${moveResult.destination}.`}
                  />
                ) : null}
              </div>
            ) : null}
          </div>
        ) : null}
        <p className="text-secondary" style={{ margin: "8px 0 0", fontSize: 12 }}>
          The next step will create a copy-and-verify move plan for one selected folder and
          destination. Stora will never move a folder automatically.
        </p>
      </section>

      <section className="page-section">
        <SectionHeader>Recommended next steps</SectionHeader>
        <SettingsSection>
          <SettingsRow
            title="Review regeneratable caches"
            description="Temporary files, thumbnails, shader caches, crash dumps, and other regeneratable data can free space without touching your documents."
            control={<Button onClick={() => setPage("cleanup")}>Review cleanup</Button>}
          />
          <SettingsRow
            title="Find large personal files"
            description="Pictures, videos, downloads, and project files may be candidates for an intentional move to another drive. Stora will not move them automatically."
            control={
              <Button
                variant="subtle"
                onClick={() => {
                  selectDrive(target.root);
                  setPage("largeFiles");
                }}
              >
                View largest files
              </Button>
            }
          />
          <SettingsRow
            title="Review applications and games"
            description="Do not move an installed program, game library, Docker disk, or AppData folder in File Explorer. Their owning tool should perform the move."
            control={<Button variant="subtle" onClick={() => setPage("applications")}>Applications</Button>}
          />
          <SettingsRow
            title="Check developer storage"
            description="Project artifacts and package caches may be regeneratable; virtual disks are guidance-only."
            control={<Button variant="subtle" onClick={() => setPage("developer")}>Developer</Button>}
          />
        </SettingsSection>
      </section>
    </>
  );
}

function choosePressuredDrive(drives: DriveInfo[]): DriveInfo | null {
  const fixed = drives.filter((drive) => drive.driveType === "fixed");
  if (fixed.length === 0) return null;
  return (
    fixed.find((drive) => drive.root.toUpperCase() === "C:\\") ??
    [...fixed].sort((a, b) => percentUsed(b) - percentUsed(a))[0]
  );
}

function percentUsed(drive: DriveInfo): number {
  return ((drive.totalBytes - drive.freeBytes) / Math.max(drive.totalBytes, 1)) * 100;
}
