import { useEffect, useState } from "react";
import {
  Button,
  ComboBox,
  ContentDialog,
  DataGrid,
  InfoBar,
  NumberBox,
  SectionHeader,
  SettingsRow,
  SettingsSection,
  TeachingTip,
  ToggleSwitch,
} from "@sawcy/memora-ui";

import { EmptyState, PageHeader } from "../components/common";
import * as api from "../lib/api";
import { formatBytes, formatTimestamp } from "../lib/format";
import type { Exclusion, QuarantineItem, Settings } from "../lib/types";
import { useApp } from "../state/AppContext";

const EXCLUSION_REASONS: Record<string, string> = {
  userExclusion: "User exclusion",
  protectedWindowsPath: "Protected Windows path",
  activeApplication: "Active application",
  systemComponent: "System component",
  reparsePoint: "Reparse point",
  unsupportedVolume: "Unsupported volume",
};

export default function SettingsPage() {
  const { settings, saveSettings, notify, reportError } = useApp();

  const [exclusions, setExclusions] = useState<Exclusion[]>([]);
  const [dataFolder, setDataFolder] = useState("");
  const [confirmClear, setConfirmClear] = useState(false);
  const [quarantine, setQuarantine] = useState<QuarantineItem[]>([]);
  const [quarantineBytes, setQuarantineBytes] = useState(0);
  const [showReparseTip, setShowReparseTip] = useState(false);
  const [advisorKeySaved, setAdvisorKeySaved] = useState(false);
  const [editingAdvisorKey, setEditingAdvisorKey] = useState(false);
  const [advisorKey, setAdvisorKey] = useState("");

  useEffect(() => {
    api.getExclusions().then(setExclusions).catch(reportError);
    api.getDataFolder().then(setDataFolder).catch(() => undefined);
    api.getAdvisorKeyStatus().then((status) => setAdvisorKeySaved(status.saved)).catch(() => undefined);
    void loadQuarantine();
  }, [reportError]);

  async function loadQuarantine() {
    try {
      setQuarantine(await api.getQuarantineItems());
      setQuarantineBytes(await api.getQuarantineSize());
    } catch {
      // Quarantine is optional; an empty list is a valid state.
    }
  }

  if (!settings) {
    return (
      <>
        <PageHeader title="Settings" />
        <EmptyState title="Loading settings…" />
      </>
    );
  }

  const update = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    void saveSettings({ ...settings, [key]: value });
  };

  const removeExclusion = async (id: number) => {
    try {
      setExclusions(await api.deleteExclusion(id));
      notify({ tone: "success", title: "Exclusion removed." });
    } catch (error) {
      reportError(error);
    }
  };

  return (
    <>
      <PageHeader title="Settings" />

      <section className="page-section">
        <SectionHeader>General</SectionHeader>
        <SettingsSection>
          <SettingsRow
            title="Start Stora with Windows"
            description="Stora starts minimized to the notification area."
            control={
              <ToggleSwitch
                label="Start Stora with Windows"
                checked={settings.startWithWindows}
                onChange={(value) => update("startWithWindows", value)}
              />
            }
          />
          <SettingsRow
            title="Minimize to the notification area"
            control={
              <ToggleSwitch
                label="Minimize to the notification area"
                checked={settings.minimizeToTray}
                onChange={(value) => update("minimizeToTray", value)}
              />
            }
          />
          <SettingsRow
            title="Close to the notification area"
            description="Closing the window keeps Stora running so background monitoring continues."
            control={
              <ToggleSwitch
                label="Close to the notification area"
                checked={settings.closeToTray}
                onChange={(value) => update("closeToTray", value)}
              />
            }
          />
          <SettingsRow
            title="Show notifications"
            control={
              <ToggleSwitch
                label="Show notifications"
                checked={settings.showNotifications}
                onChange={(value) => update("showNotifications", value)}
              />
            }
          />
          <SettingsRow
            title="Double-click notification-area icon"
            description="Choose which non-destructive Stora view opens from the tray."
            control={
              <ComboBox
                label="Double-click notification-area icon"
                value={settings.trayDoubleClickAction}
                options={[
                  { value: "open", label: "Open Stora" },
                  { value: "scan", label: "Open storage scan" },
                  { value: "cleanup", label: "Open cleanup review" },
                  { value: "largeFiles", label: "Open largest files" },
                ]}
                onChange={(value) =>
                  update("trayDoubleClickAction", value as Settings["trayDoubleClickAction"])
                }
              />
            }
          />
          <SettingsRow
            title="Theme"
            description="Stora follows your Windows theme by default."
            control={
              <ComboBox
                label="Theme"
                value={settings.theme}
                options={[
                  { value: "system", label: "Use system setting" },
                  { value: "light", label: "Light" },
                  { value: "dark", label: "Dark" },
                ]}
                onChange={(value) => update("theme", value as Settings["theme"])}
              />
            }
          />
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>Scanning</SectionHeader>
        <SettingsSection>
          <SettingsRow
            title="Scan hidden files"
            control={
              <ToggleSwitch
                label="Scan hidden files"
                checked={settings.scanHiddenFiles}
                onChange={(value) => update("scanHiddenFiles", value)}
              />
            }
          />
          <SettingsRow
            title="Scan system files"
            description="System files are reported but never offered for removal."
            control={
              <ToggleSwitch
                label="Scan system files"
                checked={settings.scanSystemFiles}
                onChange={(value) => update("scanSystemFiles", value)}
              />
            }
          />
          <SettingsRow
            title="Follow symbolic links"
            note="Off by default. Following links can count the same data more than once."
            control={
              <ToggleSwitch
                label="Follow symbolic links"
                checked={settings.followSymlinks}
                onChange={(value) => {
                  update("followSymlinks", value);
                  if (value) setShowReparseTip(true);
                }}
              />
            }
          />
          <SettingsRow
            title="Follow junctions"
            note="Off by default, for the same reason as symbolic links."
            control={
              <ToggleSwitch
                label="Follow junctions"
                checked={settings.followJunctions}
                onChange={(value) => {
                  update("followJunctions", value);
                  if (value) setShowReparseTip(true);
                }}
              />
            }
          />
          <SettingsRow
            title="Measure size on disk"
            description="Reports the space files actually occupy, which differs from their logical size for compressed and sparse files."
            control={
              <ToggleSwitch
                label="Measure size on disk"
                checked={settings.useAllocatedSize}
                onChange={(value) => update("useAllocatedSize", value)}
              />
            }
          />
          <SettingsRow
            title="Scanning intensity"
            description="Lower values leave more of the system free while a scan runs."
            control={
              <NumberBox
                label="Scanning intensity"
                value={settings.scanConcurrency}
                min={1}
                max={16}
                onChange={(value) => update("scanConcurrency", value)}
              />
            }
          />
        </SettingsSection>

        {showReparseTip ? (
          <div style={{ marginTop: 8 }}>
            <TeachingTip
              title="Following links changes what totals mean"
              onDismiss={() => setShowReparseTip(false)}
            >
              A junction or symbolic link points at data that lives elsewhere. When
              Stora follows one, that data is counted in both locations, so folder
              totals will add up to more than the drive holds. Stora still refuses to
              walk a link that points back into itself.
            </TeachingTip>
          </div>
        ) : null}
      </section>

      <section className="page-section">
        <SectionHeader>Cleanup</SectionHeader>
        <SettingsSection>
          <SettingsRow
            title="Default removal method"
            control={
              <ComboBox
                label="Default removal method"
                value={settings.defaultDeletionMethod}
                options={[
                  { value: "recycleBin", label: "Move to Recycle Bin" },
                  { value: "permanent", label: "Delete permanently" },
                  {
                    value: "quarantine",
                    label: "Move to Stora quarantine",
                    disabled: !settings.enableQuarantine,
                  },
                ]}
                onChange={(value) => update("defaultDeletionMethod", String(value))}
              />
            }
          />
          <SettingsRow
            title="Confirm permanent deletion"
            description="Always ask before removing files without using the Recycle Bin."
            control={
              <ToggleSwitch
                label="Confirm permanent deletion"
                checked={settings.confirmPermanentDeletion}
                onChange={(value) => update("confirmPermanentDeletion", value)}
              />
            }
          />
          <SettingsRow
            title="Enable quarantine"
            description="Moves removed files to a Stora folder so they can be restored."
            control={
              <ToggleSwitch
                label="Enable quarantine"
                checked={settings.enableQuarantine}
                onChange={(value) => update("enableQuarantine", value)}
              />
            }
          />
          <SettingsRow
            title="Quarantine retention"
            description="How long quarantined files are kept before they can expire."
            control={
              <NumberBox
                label="Quarantine retention"
                value={settings.quarantineRetentionDays}
                min={1}
                max={90}
                suffix="days"
                disabled={!settings.enableQuarantine}
                onChange={(value) => update("quarantineRetentionDays", value)}
              />
            }
          />
          <SettingsRow
            title="Show advanced categories"
            note="Advanced categories affect Windows servicing state. Off by default."
            control={
              <ToggleSwitch
                label="Show advanced categories"
                checked={settings.showAdvancedCategories}
                onChange={(value) => update("showAdvancedCategories", value)}
              />
            }
          />
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>Stora Advisor</SectionHeader>
        <SettingsSection>
          <SettingsRow
            title="OpenAI API key"
            description={
              advisorKeySaved
                ? "Saved in Windows Credential Manager on this device. Stora never stores or displays the key in its database."
                : "Optional. Required only when you enable cloud-backed Advisor explanations."
            }
            control={
              <div className="row">
                <Button variant="subtle" onClick={() => setEditingAdvisorKey(true)}>
                  {advisorKeySaved ? "Replace key" : "Add key"}
                </Button>
                {advisorKeySaved ? (
                  <Button
                    variant="danger"
                    onClick={() =>
                      void api
                        .deleteAdvisorApiKey()
                        .then((status) => {
                          setAdvisorKeySaved(status.saved);
                          notify({ tone: "success", title: "Advisor key removed from Windows Credential Manager." });
                        })
                        .catch(reportError)
                    }
                  >
                    Remove
                  </Button>
                ) : null}
              </div>
            }
          />
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>Exclusions</SectionHeader>
        {exclusions.length === 0 ? (
          <SettingsSection>
            <EmptyState
              title="No exclusions yet"
              detail="Right-click a folder or file in Storage or Large files to exclude it from future scans."
            />
          </SettingsSection>
        ) : (
          <DataGrid
            ariaLabel="Exclusions"
            rows={exclusions}
            rowKey={(row) => row.id}
            columns={[
              { id: "pattern", header: "Pattern", render: (row) => row.pattern },
              {
                id: "kind",
                header: "Type",
                width: 120,
                render: (row) => row.kind,
              },
              {
                id: "reason",
                header: "Why",
                width: 200,
                render: (row) => EXCLUSION_REASONS[row.reason] ?? row.reason,
              },
              {
                id: "actions",
                header: "",
                width: 100,
                render: (row) => (
                  <Button
                    variant="subtle"
                    onClick={() => void removeExclusion(row.id)}
                    disabled={row.reason !== "userExclusion"}
                  >
                    Remove
                  </Button>
                ),
              },
            ]}
          />
        )}
      </section>

      <section className="page-section">
        <SectionHeader>Quarantine</SectionHeader>
        {quarantine.length === 0 ? (
          <SettingsSection>
            <EmptyState
              title="Nothing is in quarantine"
              detail="Files removed with the quarantine method are held here so they can be put back."
            />
          </SettingsSection>
        ) : (
          <>
            <p className="text-secondary" style={{ marginBottom: 6 }}>
              {quarantine.length} item(s) held · {formatBytes(quarantineBytes)}
            </p>
            <DataGrid
              ariaLabel="Quarantined files"
              rows={quarantine}
              rowKey={(row) => row.id}
              columns={[
                {
                  id: "path",
                  header: "Original location",
                  render: (row) => row.originalPath,
                },
                {
                  id: "size",
                  header: "Size",
                  width: 110,
                  align: "end",
                  render: (row) => (
                    <span className="numeric">{formatBytes(row.size)}</span>
                  ),
                },
                {
                  id: "expires",
                  header: "Kept until",
                  width: 170,
                  render: (row) =>
                    row.expiresAt ? (
                      <span className="numeric">{formatTimestamp(row.expiresAt)}</span>
                    ) : (
                      "Until removed manually"
                    ),
                },
                {
                  id: "actions",
                  header: "",
                  width: 190,
                  render: (row) => (
                    <div className="row">
                      <Button
                        onClick={() =>
                          void api
                            .restoreQuarantineItem(row.id)
                            .then(() => {
                              notify({
                                tone: "success",
                                title: "File restored to its original location.",
                              });
                              return loadQuarantine();
                            })
                            .catch(reportError)
                        }
                      >
                        Restore
                      </Button>
                      <Button
                        variant="danger"
                        onClick={() =>
                          void api
                            .purgeQuarantineItem(row.id)
                            .then(() => {
                              notify({ tone: "info", title: "File permanently removed." });
                              return loadQuarantine();
                            })
                            .catch(reportError)
                        }
                      >
                        Delete
                      </Button>
                    </div>
                  ),
                },
              ]}
            />
          </>
        )}
      </section>

      <section className="page-section">
        <SectionHeader>Applications</SectionHeader>
        <SettingsSection>
          <SettingsRow
            title="Track application launches"
            description="Records when a program starts, on this device only. Nothing is uploaded."
            note="Off by default. Without it, Stora reports no reliable activity data rather than guessing."
            control={
              <ToggleSwitch
                label="Track application launches"
                checked={settings.trackApplicationLaunches}
                onChange={(value) => update("trackApplicationLaunches", value)}
              />
            }
          />
          <SettingsRow
            title="Use Windows activity estimates"
            description="Reads Windows' own record of programs launched from the Start menu, desktop, or File Explorer."
            note="An estimate, not a launch Stora watched. It cannot see programs started from a terminal or a game launcher, so silence never means unused."
            control={
              <ToggleSwitch
                label="Use Windows activity estimates"
                checked={settings.enableWindowsActivityEstimates}
                onChange={(value) => update("enableWindowsActivityEstimates", value)}
              />
            }
          />
          <SettingsRow
            title="Show confidence levels"
            description="Displays how far each activity figure can be trusted."
            control={
              <ToggleSwitch
                label="Show confidence levels"
                checked={settings.showConfidenceLevels}
                onChange={(value) => update("showConfidenceLevels", value)}
              />
            }
          />
          <SettingsRow
            title="Hide background utilities"
            description="Keeps helpers and updaters out of the application list."
            control={
              <ToggleSwitch
                label="Hide background utilities"
                checked={settings.excludeBackgroundUtilities}
                onChange={(value) => update("excludeBackgroundUtilities", value)}
              />
            }
          />
          <SettingsRow
            title="Clear observed activity"
            description="Removes every launch Stora has recorded."
            control={
              <Button
                variant="danger"
                onClick={() =>
                  void api
                    .clearApplicationActivity()
                    .then(() => notify({ tone: "success", title: "Activity cleared." }))
                    .catch(reportError)
                }
              >
                Clear activity
              </Button>
            }
          />
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>Developer</SectionHeader>
        <SettingsSection>
          <SettingsRow
            title="Detect development projects"
            description="Finds projects proven by a manifest, then classifies the caches they generate."
            control={
              <ToggleSwitch
                label="Detect development projects"
                checked={settings.detectDevelopmentProjects}
                onChange={(value) => update("detectDevelopmentProjects", value)}
              />
            }
          />
          <SettingsRow
            title="Scan package manager caches"
            control={
              <ToggleSwitch
                label="Scan package manager caches"
                checked={settings.scanPackageCaches}
                onChange={(value) => update("scanPackageCaches", value)}
              />
            }
          />
          <SettingsRow
            title="Detect virtual disks"
            description="Reports WSL, Docker, and virtual machine disks. Stora never deletes one."
            control={
              <ToggleSwitch
                label="Detect virtual disks"
                checked={settings.detectVirtualDisks}
                onChange={(value) => update("detectVirtualDisks", value)}
              />
            }
          />
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>Privacy</SectionHeader>
        <div style={{ marginBottom: 8 }}>
          <InfoBar
            tone="info"
            title="All analysis stays on this device"
            message="Stora does not upload file names, paths, or activity anywhere."
          />
        </div>
        <SettingsSection>
          <SettingsRow
            title="Store scan history"
            control={
              <ToggleSwitch
                label="Store scan history"
                checked={settings.storeScanHistory}
                onChange={(value) => update("storeScanHistory", value)}
              />
            }
          />
          <SettingsRow
            title="Store cleanup history"
            control={
              <ToggleSwitch
                label="Store cleanup history"
                checked={settings.storeCleanupHistory}
                onChange={(value) => update("storeCleanupHistory", value)}
              />
            }
          />
          <SettingsRow
            title="Delete history after"
            control={
              <NumberBox
                label="Delete history after"
                value={settings.historyRetentionDays}
                min={7}
                max={730}
                suffix="days"
                onChange={(value) => update("historyRetentionDays", value)}
              />
            }
          />
          <SettingsRow
            title="Clear local data"
            description="Removes stored scan results and cleanup history. Your settings and exclusions are kept, and no files on disk are touched."
            control={
              <Button variant="danger" onClick={() => setConfirmClear(true)}>
                Clear local data
              </Button>
            }
          />
        </SettingsSection>
      </section>

      <section className="page-section">
        <SectionHeader>Advanced</SectionHeader>
        <SettingsSection>
          <SettingsRow
            title="Enable debug logging"
            description="Writes additional diagnostic detail to the local log."
            control={
              <ToggleSwitch
                label="Enable debug logging"
                checked={settings.debugLogging}
                onChange={(value) => update("debugLogging", value)}
              />
            }
          />
          <SettingsRow
            title="Data folder"
            description={dataFolder || "Resolving…"}
            control={
              <Button
                variant="subtle"
                disabled={!dataFolder}
                onClick={() => void api.revealInExplorer(dataFolder).catch(reportError)}
              >
                Open folder
              </Button>
            }
          />
        </SettingsSection>
      </section>

      <ContentDialog
        open={confirmClear}
        title="Clear stored scan and cleanup history?"
        primaryText="Clear local data"
        cancelText="Cancel"
        destructive
        onPrimary={() => {
          setConfirmClear(false);
          api
            .clearLocalData()
            .then(() =>
              notify({ tone: "success", title: "Local data cleared." }),
            )
            .catch(reportError);
        }}
        onCancel={() => setConfirmClear(false)}
      >
        <p style={{ margin: 0 }}>
          This removes Stora's record of past scans and cleanups. No files on your
          drives are deleted, and your settings and exclusions are kept.
        </p>
      </ContentDialog>

      <ContentDialog
        open={editingAdvisorKey}
        title={advisorKeySaved ? "Replace Advisor API key" : "Add Advisor API key"}
        primaryText="Save securely"
        cancelText="Cancel"
        onPrimary={() => {
          void api
            .saveAdvisorApiKey(advisorKey)
            .then((status) => {
              setAdvisorKeySaved(status.saved);
              setAdvisorKey("");
              setEditingAdvisorKey(false);
              notify({
                tone: "success",
                title: "Advisor key saved in Windows Credential Manager.",
              });
            })
            .catch(reportError);
        }}
        onCancel={() => {
          setAdvisorKey("");
          setEditingAdvisorKey(false);
        }}
      >
        <div className="stack">
          <p className="text-secondary" style={{ margin: 0 }}>
            The key is used only for the optional cloud Advisor. It is not saved in
            Stora settings, scan history, logs, or reports.
          </p>
          <label className="stack" htmlFor="advisor-api-key">
            <span>OpenAI API key</span>
            <input
              id="advisor-api-key"
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={advisorKey}
              onChange={(event) => setAdvisorKey(event.target.value)}
              style={{ width: "100%", boxSizing: "border-box" }}
            />
          </label>
        </div>
      </ContentDialog>
    </>
  );
}
