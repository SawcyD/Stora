import { useCallback, useEffect, useState } from "react";
import {
  Button,
  ComboBox,
  CommandBar,
  CommandGroup,
  ContentDialog,
  DataGrid,
  InfoBar,
  InfoRow,
  NumberBox,
  SearchBox,
  SectionHeader,
  SettingsRow,
  SettingsSection,
  ToggleSwitch,
} from "@sawcy/memora-ui";

import { EmptyState, PageHeader } from "../components/common";
import { RefreshIcon } from "../components/icons";
import * as api from "../lib/api";
import { formatBytes, formatCount, formatTimestamp } from "../lib/format";
import type { AutomationRule, GrowthRow, RuleRunRow } from "../lib/types";
import { useApp } from "../state/AppContext";

const GIGABYTE = 1024 * 1024 * 1024;

/** The only categories automation is permitted to remove. */
const SAFE_CATEGORIES = [
  { id: "userTemp", label: "User temporary files" },
  { id: "thumbnailCache", label: "Thumbnail cache" },
  { id: "shaderCache", label: "DirectX shader cache" },
  { id: "crashDumps", label: "Application crash dumps" },
  { id: "errorReports", label: "Old error reports" },
  { id: "windowsTemp", label: "System temporary files" },
  { id: "deliveryOptimization", label: "Delivery Optimization files" },
];

const WEEKDAYS = [
  { value: 0, label: "Sunday" },
  { value: 1, label: "Monday" },
  { value: 2, label: "Tuesday" },
  { value: 3, label: "Wednesday" },
  { value: 4, label: "Thursday" },
  { value: 5, label: "Friday" },
  { value: 6, label: "Saturday" },
];

const RANGES = [
  { value: "day", label: "24 hours" },
  { value: "week", label: "7 days" },
  { value: "month", label: "30 days" },
  { value: "quarter", label: "90 days" },
  { value: "sinceInstall", label: "Since Stora was installed" },
];

export default function AutomationPage() {
  const { selectedDrive, notify, reportError } = useApp();

  const [rules, setRules] = useState<AutomationRule[]>([]);
  const [growth, setGrowth] = useState<GrowthRow[]>([]);
  const [range, setRange] = useState("week");
  const [creating, setCreating] = useState(false);
  const [history, setHistory] = useState<{ rule: AutomationRule; runs: RuleRunRow[] } | null>(
    null,
  );

  // Draft rule state.
  const [name, setName] = useState("");
  const [trigger, setTrigger] = useState("weekly");
  const [action, setAction] = useState("notify");
  const [weekday, setWeekday] = useState(0);
  const [freeSpaceGb, setFreeSpaceGb] = useState(20);
  const [growthGb, setGrowthGb] = useState(8);
  const [minimumAge, setMinimumAge] = useState(14);
  const [categories, setCategories] = useState<Set<string>>(new Set(["userTemp"]));

  const load = useCallback(async () => {
    try {
      setRules(await api.getAutomationRules());
    } catch (error) {
      reportError(error);
    }
  }, [reportError]);

  const loadGrowth = useCallback(async () => {
    try {
      setGrowth(await api.getGrowthHistory(range));
    } catch (error) {
      reportError(error);
    }
  }, [range, reportError]);

  useEffect(() => {
    void load();
  }, [load]);

  useEffect(() => {
    void loadGrowth();
  }, [loadGrowth]);

  const create = async () => {
    setCreating(false);
    try {
      setRules(
        await api.createAutomationRule({
          name,
          trigger,
          action,
          weekday,
          freeSpaceThreshold: freeSpaceGb * GIGABYTE,
          growthThreshold: growthGb * GIGABYTE,
          watchedPath: null,
          categories: [...categories],
          minimumAgeDays: minimumAge,
        }),
      );
      notify({
        tone: "success",
        title: `“${name}” was created, switched off.`,
        detail: "Review what it affects, then turn it on when you are satisfied.",
      });
      setName("");
    } catch (error) {
      reportError(error);
    }
  };

  const showHistory = async (rule: AutomationRule) => {
    try {
      setHistory({ rule, runs: await api.getRuleHistory(rule.id) });
    } catch (error) {
      reportError(error);
    }
  };

  const deleting = action === "cleanSafeCategories";

  return (
    <>
      <PageHeader
        title="Automation"
        description="Local rules. Disabled when created, and never able to remove anything you made."
      />

      <div className="page-section">
        <InfoBar
          tone="info"
          title="What automation is allowed to do"
          message="A rule can notify you about anything, but it may only remove regeneratable caches. Downloads, the Recycle Bin, and old installers can never be cleaned automatically — those always need you to look first."
        />
      </div>

      <CommandBar>
        <CommandGroup>
          <Button variant="primary" onClick={() => setCreating(true)}>
            Create rule
          </Button>
          <Button
            variant="subtle"
            onClick={() =>
              void api
                .evaluateAutomationRules()
                .then((messages) =>
                  notify({
                    tone: "info",
                    title:
                      messages.length === 0
                        ? "No rules would run right now."
                        : `${messages.length} rule(s) would run now.`,
                    detail: messages.join(" "),
                  }),
                )
                .catch(reportError)
            }
          >
            Check rules now
          </Button>
        </CommandGroup>
      </CommandBar>

      <section className="page-section">
        <SectionHeader>Rules</SectionHeader>
        {rules.length === 0 ? (
          <SettingsSection>
            <EmptyState
              title="No rules yet"
              detail="A rule can tell you when storage runs low, or clear regeneratable caches on a schedule."
            />
          </SettingsSection>
        ) : (
          <SettingsSection>
            {rules.map((rule) => (
              <SettingsRow
                key={rule.id}
                title={rule.name}
                description={describeRule(rule)}
                note={
                  <span className="cleanup-row__meta">
                    {rule.categories.length > 0 ? (
                      <span className="text-secondary">
                        Affects: {rule.categories.join(", ")}
                      </span>
                    ) : null}
                    {rule.lastRun ? (
                      <span className="text-secondary">
                        Last run {formatTimestamp(rule.lastRun)}
                      </span>
                    ) : (
                      <span className="text-secondary">Has never run</span>
                    )}
                    {rule.consecutiveErrors > 0 ? (
                      <span className="badge badge--moderate">
                        {rule.consecutiveErrors} consecutive error
                        {rule.consecutiveErrors === 1 ? "" : "s"}
                      </span>
                    ) : null}
                  </span>
                }
                control={
                  <span className="cleanup-row__control">
                    <Button variant="subtle" onClick={() => void showHistory(rule)}>
                      History
                    </Button>
                    <Button
                      variant="subtle"
                      onClick={() =>
                        void api
                          .deleteAutomationRule(rule.id)
                          .then(setRules)
                          .catch(reportError)
                      }
                    >
                      Delete
                    </Button>
                    <ToggleSwitch
                      label={`Enable ${rule.name}`}
                      checked={rule.enabled}
                      onChange={(on) =>
                        void api
                          .setRuleEnabled(rule.id, on)
                          .then(setRules)
                          .catch(reportError)
                      }
                    />
                  </span>
                }
              />
            ))}
          </SettingsSection>
        )}
      </section>

      <section className="page-section">
        <SectionHeader>What recently consumed storage</SectionHeader>
        <CommandBar>
          <CommandGroup>
            <ComboBox
              label="Time range"
              value={range}
              options={RANGES}
              onChange={(value) => setRange(String(value))}
            />
            <Button variant="subtle" onClick={() => void loadGrowth()}>
              <RefreshIcon /> Refresh
            </Button>
            <Button
              variant="subtle"
              disabled={!selectedDrive}
              onClick={() =>
                selectedDrive &&
                void api
                  .recordGrowthSnapshot(selectedDrive.root)
                  .then((count) =>
                    notify({
                      tone: "success",
                      title: `Recorded sizes for ${formatCount(count)} folders.`,
                      detail: "Growth appears once there are two snapshots to compare.",
                    }),
                  )
                  .then(() => loadGrowth())
                  .catch(reportError)
              }
            >
              Record snapshot
            </Button>
          </CommandGroup>
        </CommandBar>

        {growth.length === 0 ? (
          <EmptyState
            title="No snapshots recorded yet"
            detail="Growth is worked out by comparing folder sizes over time, rather than watching every file change. Record one snapshot now and another later."
          />
        ) : (
          <DataGrid
            ariaLabel="Folder growth"
            rows={growth}
            rowKey={(row) => row.path}
            columns={[
              { id: "name", header: "Folder", render: (row) => row.name },
              {
                id: "current",
                header: "Current",
                width: 120,
                align: "end",
                render: (row) => (
                  <span className="numeric">{formatBytes(row.currentBytes)}</span>
                ),
              },
              {
                id: "change",
                header: "Change",
                width: 140,
                align: "end",
                render: (row) =>
                  row.hasBaseline ? (
                    <span className="numeric">
                      {row.changeBytes >= 0 ? "+" : "−"}
                      {formatBytes(Math.abs(row.changeBytes))}
                    </span>
                  ) : (
                    <span className="text-secondary">Not enough history</span>
                  ),
              },
              {
                id: "compared",
                header: "Compared with",
                width: 160,
                render: (row) =>
                  row.hasBaseline ? (
                    <span className="numeric">{formatTimestamp(row.comparedAt)}</span>
                  ) : (
                    "—"
                  ),
              },
            ]}
          />
        )}
      </section>

      <ContentDialog
        open={creating}
        title="Create an automation rule"
        primaryText="Create rule"
        cancelText="Cancel"
        onPrimary={create}
        onCancel={() => setCreating(false)}
      >
        <div className="stack">
          <SearchBox
            label="Rule name"
            placeholder="Weekly temporary file cleanup"
            value={name}
            onChange={setName}
          />
          <ComboBox
            label="When"
            value={trigger}
            options={[
              { value: "weekly", label: "Every week" },
              { value: "lowFreeSpace", label: "When available storage runs low" },
              { value: "folderGrowth", label: "When a watched folder grows sharply" },
            ]}
            onChange={(value) => setTrigger(String(value))}
          />
          {trigger === "weekly" ? (
            <ComboBox
              label="Day"
              value={weekday}
              options={WEEKDAYS}
              onChange={(value) => setWeekday(Number(value))}
            />
          ) : null}
          {trigger === "lowFreeSpace" ? (
            <NumberBox
              label="Free space threshold"
              value={freeSpaceGb}
              min={1}
              max={500}
              suffix="GB"
              onChange={setFreeSpaceGb}
            />
          ) : null}
          {trigger === "folderGrowth" ? (
            <NumberBox
              label="Growth threshold"
              value={growthGb}
              min={1}
              max={500}
              suffix="GB"
              onChange={setGrowthGb}
            />
          ) : null}

          <ComboBox
            label="Then"
            value={action}
            options={[
              { value: "notify", label: "Notify me" },
              { value: "openCleanupReview", label: "Notify me and open cleanup review" },
              {
                value: "cleanSafeCategories",
                label: "Remove the selected regeneratable caches",
              },
            ]}
            onChange={(value) => setAction(String(value))}
          />

          {deleting ? (
            <>
              <InfoBar
                tone="warning"
                title="This rule will remove files without asking each time"
                message="Only the categories below are available, and every one is data the owning program recreates on demand."
              />
              <NumberBox
                label="Only files older than"
                value={minimumAge}
                min={1}
                max={365}
                suffix="days"
                onChange={setMinimumAge}
              />
            </>
          ) : null}

          <SectionHeader>Categories this rule affects</SectionHeader>
          <SettingsSection>
            {SAFE_CATEGORIES.map((category) => (
              <SettingsRow
                key={category.id}
                title={category.label}
                control={
                  <ToggleSwitch
                    label={`Include ${category.label}`}
                    checked={categories.has(category.id)}
                    onChange={(on) =>
                      setCategories((current) => {
                        const next = new Set(current);
                        if (on) next.add(category.id);
                        else next.delete(category.id);
                        return next;
                      })
                    }
                  />
                }
              />
            ))}
          </SettingsSection>

          <p className="text-secondary" style={{ margin: 0 }}>
            The rule is created switched off. Turn it on yourself once you are happy with
            what it affects.
          </p>
        </div>
      </ContentDialog>

      <ContentDialog
        open={history !== null}
        title={history ? `${history.rule.name} — run history` : ""}
        cancelText="Close"
        onCancel={() => setHistory(null)}
      >
        {history === null || history.runs.length === 0 ? (
          <EmptyState title="This rule has not run yet." />
        ) : (
          <div className="stack">
            {history.runs.map((run) => (
              <InfoRow
                key={`${run.ranAt}-${run.outcome}`}
                label={formatTimestamp(run.ranAt)}
                value={`${run.outcome}${
                  run.recoveredBytes > 0
                    ? ` · ${formatBytes(run.recoveredBytes)} recovered`
                    : ""
                }`}
                help={run.detail || undefined}
              />
            ))}
          </div>
        )}
      </ContentDialog>
    </>
  );
}

function describeRule(rule: AutomationRule): string {
  const when =
    rule.trigger === "weekly"
      ? `Every ${WEEKDAYS[rule.weekday]?.label ?? "week"}`
      : rule.trigger === "lowFreeSpace"
        ? `When available storage falls below ${formatBytes(rule.freeSpaceThreshold)}`
        : `When a watched folder grows by ${formatBytes(rule.growthThreshold)} in a week`;

  const then =
    rule.action === "notify"
      ? "notify me"
      : rule.action === "openCleanupReview"
        ? "notify me and open cleanup review"
        : `remove the selected caches (files older than ${rule.minimumAgeDays} days)`;

  return `${when}, ${then}.`;
}
