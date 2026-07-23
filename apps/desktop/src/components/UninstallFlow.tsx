/**
 * The uninstall conversation, from confirmation through to leftovers.
 *
 * Three stages, each of which the user drives:
 *
 * 1. Confirm — states the method that will actually be used, and whether a
 *    System Restore point was created rather than assuming one was.
 * 2. Running — the vendor's uninstaller is open; Stora waits.
 * 3. Leftovers — what survived, offered through the ordinary plan pipeline.
 */

import { useState } from "react";
import {
  ContentDialog,
  InfoBar,
  InfoRow,
  SectionHeader,
  ToggleSwitch,
  Tooltip,
} from "@sawcy/memora-ui";

import { EmptyState } from "./common";
import * as api from "../lib/api";
import { formatBytes, formatCount, shortenPath } from "../lib/format";
import type {
  AppWithActivity,
  Leftover,
  UninstallPreflight,
  UninstallStarted,
} from "../lib/types";

type Stage = "confirm" | "running" | "leftovers";

interface UninstallFlowProps {
  app: AppWithActivity;
  preflight: UninstallPreflight;
  onClose: () => void;
  onFinished: () => void;
  notify: (notice: {
    tone: "info" | "success" | "warning" | "error";
    title: string;
    detail?: string;
  }) => void;
  reportError: (error: unknown) => void;
}

export default function UninstallFlow({
  app,
  preflight,
  onClose,
  onFinished,
  notify,
  reportError,
}: UninstallFlowProps) {
  const [stage, setStage] = useState<Stage>("confirm");
  const [started, setStarted] = useState<UninstallStarted | null>(null);
  const [leftovers, setLeftovers] = useState<Leftover[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  const begin = async () => {
    setBusy(true);
    try {
      const outcome = await api.startUninstall(app.id);
      setStarted(outcome);
      setStage("running");
    } catch (error) {
      reportError(error);
      onClose();
    } finally {
      setBusy(false);
    }
  };

  const checkLeftovers = async () => {
    setBusy(true);
    try {
      setLeftovers(await api.scanUninstallLeftovers(app.id));
      setStage("leftovers");
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const removeLeftovers = async () => {
    setBusy(true);
    try {
      const response = await api.buildLeftoverCleanupPlan(app.id, [...selected]);
      const everyIndex = response.plan.items.map((_, index) => index);
      const outcome = await api.executeCleanupPlan(
        response.plan.planId,
        everyIndex,
        "recycleBin",
      );

      notify({
        tone: outcome.filesSkipped > 0 ? "warning" : "success",
        title: `Recovered ${formatBytes(outcome.recoveredBytes)} of leftovers.`,
        detail:
          outcome.filesSkipped > 0
            ? `${formatCount(outcome.filesSkipped)} files were in use and were skipped.`
            : undefined,
      });

      onFinished();
      onClose();
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const toggle = (path: string, on: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (on) next.add(path);
      else next.delete(path);
      return next;
    });
  };

  // ------------------------------------------------------------- confirm

  if (stage === "confirm") {
    return (
      <ContentDialog
        open
        title={`Uninstall ${app.name}?`}
        primaryText={preflight.canUninstall ? "Start uninstaller" : undefined}
        cancelText={preflight.canUninstall ? "Cancel" : "Close"}
        destructive
        onPrimary={preflight.canUninstall && !busy ? begin : undefined}
        onCancel={onClose}
      >
        <div className="stack">
          {preflight.blockedReason ? (
            <InfoBar
              tone="warning"
              title="Stora will not uninstall this"
              message={preflight.blockedReason}
            />
          ) : null}

          <InfoRow label="Publisher" value={app.publisher || "—"} />
          <InfoRow label="Activity" value={app.activityText} />
          <InfoRow
            label="Method"
            value={preflight.methodLabel}
            help="Stora never removes software by deleting its folder."
          />
          <InfoRow
            label="Footprint measured"
            value={`${formatBytes(preflight.footprintBytes)} across ${formatCount(
              preflight.locationCount,
            )} location${preflight.locationCount === 1 ? "" : "s"}`}
            help="Captured now so Stora can tell you what the uninstaller leaves behind."
          />

          {preflight.canUninstall ? (
            <p className="text-secondary" style={{ margin: 0 }}>
              Stora will try to create a System Restore point, start this
              application's own uninstaller, then step aside. Once the uninstaller
              has closed, come back and Stora will show you what survived.
            </p>
          ) : null}
        </div>
      </ContentDialog>
    );
  }

  // ------------------------------------------------------------- running

  if (stage === "running") {
    return (
      <ContentDialog
        open
        title={`${app.name} — uninstaller started`}
        primaryText={busy ? "Checking…" : "The uninstaller has closed"}
        cancelText="Not now"
        onPrimary={busy ? undefined : checkLeftovers}
        onCancel={onClose}
      >
        <div className="stack">
          <InfoBar
            tone={started?.restorePointCreated ? "success" : "info"}
            title={
              started?.restorePointCreated
                ? "System Restore point created"
                : "No System Restore point"
            }
            message={started?.restorePointMessage}
          />

          <InfoRow label="Method" value={started?.methodLabel ?? "—"} />

          <p className="text-secondary" style={{ margin: 0 }}>
            The uninstaller is running in its own window. Follow its prompts. When it
            has finished, choose the button below and Stora will re-measure the
            folders it recorded earlier to see what was left behind.
          </p>
        </div>
      </ContentDialog>
    );
  }

  // ----------------------------------------------------------- leftovers

  const removable = (leftovers ?? []).filter((entry) => entry.removable);
  const reported = (leftovers ?? []).filter((entry) => !entry.removable);
  const selectedBytes = removable
    .filter((entry) => selected.has(entry.path))
    .reduce((total, entry) => total + entry.bytes, 0);

  return (
    <ContentDialog
      open
      title={`${app.name} — what was left behind`}
      primaryText={selected.size > 0 && !busy ? "Move selected to Recycle Bin" : undefined}
      cancelText="Done"
      onPrimary={selected.size > 0 && !busy ? removeLeftovers : undefined}
      onCancel={() => {
        onFinished();
        onClose();
      }}
    >
      {leftovers === null ? (
        <EmptyState title="Measuring…" />
      ) : leftovers.length === 0 ? (
        <EmptyState
          title="Nothing was left behind"
          detail="Every folder Stora recorded before the uninstall is gone."
        />
      ) : (
        <div className="stack">
          {removable.length === 0 ? (
            <EmptyState
              title="No removable leftovers"
              detail="Only items Stora reports but will not remove were found."
            />
          ) : (
            <>
              <p className="text-secondary" style={{ margin: 0 }}>
                These folders survived the uninstaller. Some hold settings or
                documents you may want to keep, so nothing is selected for you.
                {selected.size > 0 ? ` Selected: ${formatBytes(selectedBytes)}.` : ""}
              </p>

              {removable.map((entry) => (
                <div
                  key={entry.path}
                  className="row"
                  style={{
                    justifyContent: "space-between",
                    padding: "8px 0",
                    borderTop: "1px solid var(--memora-stroke-surface)",
                  }}
                >
                  <div style={{ minWidth: 0, flex: 1 }}>
                    <div className="row" style={{ gap: 8 }}>
                      <strong style={{ fontSize: 13 }}>{entry.relationship}</strong>
                      <span className="numeric">{formatBytes(entry.bytes)}</span>
                    </div>
                    <Tooltip content={entry.path}>
                      <span className="path-text">{shortenPath(entry.path, 56)}</span>
                    </Tooltip>
                  </div>
                  <ToggleSwitch
                    label={`Remove ${entry.path}`}
                    checked={selected.has(entry.path)}
                    onChange={(on) => toggle(entry.path, on)}
                  />
                </div>
              ))}
            </>
          )}

          {reported.length > 0 ? (
            <>
              <SectionHeader>Reported, not removed</SectionHeader>
              {reported.map((entry) => (
                <div key={entry.path} style={{ paddingTop: 6 }}>
                  <span className="badge">{entry.relationship}</span>
                  <div className="path-text">{entry.path}</div>
                  <div className="text-secondary">{entry.reason}</div>
                </div>
              ))}
            </>
          ) : null}
        </div>
      )}
    </ContentDialog>
  );
}
