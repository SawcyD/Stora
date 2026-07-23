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
  ProgressBar,
  SectionHeader,
  SettingsRow,
  SettingsSection,
  ToggleSwitch,
  Tooltip,
} from "@sawcy/memora-ui";

import { EmptyState, PageHeader, RiskBadge } from "../components/common";
import * as api from "../lib/api";
import {
  formatBytes,
  formatCount,
  formatDuration,
  formatElapsed,
  formatTimestamp,
  methodLabel,
  shortenPath,
} from "../lib/format";
import type {
  CleanupCategory,
  CleanupCategoryResult,
  CleanupItem,
  CleanupPlan,
  CleanupProgress,
  CleanupResult,
} from "../lib/types";
import { useApp } from "../state/AppContext";

type Stage = "select" | "preview" | "running" | "result";

const METHODS = [
  { value: "recycleBin", label: "Move to Recycle Bin" },
  { value: "permanent", label: "Delete permanently" },
  { value: "quarantine", label: "Move to Stora quarantine" },
];

export default function CleanupPage() {
  const { settings, notify, reportError, refreshDrives } = useApp();

  const [categories, setCategories] = useState<CleanupCategory[]>([]);
  const [enabled, setEnabled] = useState<Record<string, boolean>>({});
  const [stage, setStage] = useState<Stage>("select");
  const [plan, setPlan] = useState<CleanupPlan | null>(null);
  const [selection, setSelection] = useState<Set<number>>(new Set());
  const [method, setMethod] = useState("recycleBin");
  const [busy, setBusy] = useState(false);
  const [progress, setProgress] = useState<CleanupProgress | null>(null);
  const [result, setResult] = useState<CleanupResult | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [expandedItems, setExpandedItems] = useState<CleanupItem[]>([]);

  useEffect(() => {
    api
      .getCleanupCategories()
      .then((all) => {
        setCategories(all);
        // Only safe, low-risk categories start switched on. Downloads, the
        // Recycle Bin, and anything advanced stay off until asked for.
        setEnabled(
          Object.fromEntries(
            all.map((category) => [
              category.id,
              category.tier === "safe" && category.risk === "low",
            ]),
          ),
        );
      })
      .catch(reportError);
  }, [reportError]);

  useEffect(() => {
    if (settings) setMethod(settings.defaultDeletionMethod);
  }, [settings]);

  useEffect(() => {
    const unlisten = api.onCleanupProgress(setProgress);
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, []);

  const visibleCategories = useMemo(
    () =>
      categories.filter(
        (category) =>
          category.tier !== "advanced" || settings?.showAdvancedCategories,
      ),
    [categories, settings?.showAdvancedCategories],
  );

  const buildPreview = useCallback(async () => {
    const chosen = Object.entries(enabled)
      .filter(([, on]) => on)
      .map(([id]) => id);

    if (chosen.length === 0) {
      notify({ tone: "info", title: "Select at least one category to review." });
      return;
    }

    setBusy(true);
    try {
      const response = await api.buildCleanupPlan(chosen);
      setPlan(response.plan);
      setSelection(new Set(response.defaultSelection));
      setStage("preview");
    } catch (error) {
      reportError(error);
    } finally {
      setBusy(false);
    }
  }, [enabled, notify, reportError]);

  const loadItems = useCallback(
    async (categoryId: string) => {
      if (!plan) return;
      if (expanded === categoryId) {
        setExpanded(null);
        setExpandedItems([]);
        return;
      }
      try {
        setExpandedItems(await api.getPlanItems(plan.planId, categoryId, 500));
        setExpanded(categoryId);
      } catch (error) {
        reportError(error);
      }
    },
    [plan, expanded, reportError],
  );

  const selectedItems = useMemo(
    () => (plan ? [...selection].map((index) => plan.items[index]).filter(Boolean) : []),
    [plan, selection],
  );

  const selectedBytes = selectedItems.reduce((total, item) => total + item.size, 0);

  const toggleCategorySelection = (categoryId: string, on: boolean) => {
    if (!plan) return;
    setSelection((current) => {
      const next = new Set(current);
      plan.items.forEach((item, index) => {
        if (item.categoryId !== categoryId) return;
        if (on) next.add(index);
        else next.delete(index);
      });
      return next;
    });
  };

  const runCleanup = async () => {
    if (!plan) return;
    setConfirmOpen(false);
    setStage("running");
    setBusy(true);
    setProgress(null);

    try {
      const outcome = await api.executeCleanupPlan(
        plan.planId,
        [...selection],
        method,
      );
      setResult(outcome);
      setStage("result");
      void refreshDrives();
    } catch (error) {
      reportError(error);
      setStage("preview");
    } finally {
      setBusy(false);
    }
  };

  const restart = () => {
    setStage("select");
    setPlan(null);
    setSelection(new Set());
    setResult(null);
    setProgress(null);
    setExpanded(null);
    setExpandedItems([]);
  };

  // ------------------------------------------------------------- selection

  if (stage === "select") {
    return (
      <>
        <PageHeader
          title="Cleanup"
          description="Choose what to review. Nothing is removed until you approve a preview."
        />

        {settings?.showAdvancedCategories ? (
          <div className="page-section">
            <InfoBar
              tone="warning"
              title="Advanced categories are shown"
              message="These affect Windows servicing state. Where Windows provides a supported tool, Stora shows you the command instead of deleting files itself."
            />
          </div>
        ) : null}

        {(["safe", "reviewRequired", "advanced"] as const).map((tier) => {
          const inTier = visibleCategories.filter((c) => c.tier === tier);
          if (inTier.length === 0) return null;

          return (
            <section className="page-section" key={tier}>
              <SectionHeader>
                {tier === "safe"
                  ? "Safe to remove"
                  : tier === "reviewRequired"
                    ? "Review required"
                    : "Advanced"}
              </SectionHeader>
              <SettingsSection>
                {inTier.map((category) => (
                  <SettingsRow
                    key={category.id}
                    title={category.name}
                    description={category.explanation}
                    note={<RiskBadge risk={category.risk} />}
                    control={
                      <ToggleSwitch
                        label={`Include ${category.name}`}
                        checked={enabled[category.id] ?? false}
                        onChange={(on) =>
                          setEnabled((current) => ({ ...current, [category.id]: on }))
                        }
                      />
                    }
                  />
                ))}
              </SettingsSection>
            </section>
          );
        })}

        <CommandBar>
          <CommandGroup>
            <Button variant="primary" onClick={buildPreview} disabled={busy}>
              {busy ? "Inspecting…" : "Review selected categories"}
            </Button>
          </CommandGroup>
        </CommandBar>
      </>
    );
  }

  // --------------------------------------------------------------- preview

  if (stage === "preview" && plan) {
    const withContent = plan.categories.filter(
      (category) => category.fileCount > 0 || category.unavailableReason,
    );

    return (
      <>
        <PageHeader
          title="Cleanup preview"
          description="Inspect every item before anything is removed."
          actions={
            <Button variant="subtle" onClick={restart}>
              Back to categories
            </Button>
          }
        />

        <div className="selection-summary">
          <div className="selection-summary__figures">
            <span>
              Selected size
              <strong>{formatBytes(selectedBytes)}</strong>
            </span>
            <span>
              Files
              <strong>{formatCount(selectedItems.length)}</strong>
            </span>
            <span>
              Found in total
              <strong>{formatBytes(plan.totalBytes)}</strong>
            </span>
          </div>
          <div className="selection-summary__actions">
            <ComboBox
              label="Removal method"
              value={method}
              options={METHODS.map((option) => ({
                ...option,
                disabled:
                  option.value === "quarantine" && !settings?.enableQuarantine,
              }))}
              onChange={(value) => setMethod(String(value))}
            />
            <Button
              variant={method === "permanent" ? "danger" : "primary"}
              disabled={selectedItems.length === 0}
              onClick={() => setConfirmOpen(true)}
            >
              Clean selected items
            </Button>
          </div>
        </div>

        {method === "permanent" ? (
          <div className="page-section">
            <InfoBar
              tone="warning"
              title="Permanent deletion cannot be undone"
              message="These files will not go to the Recycle Bin. Consider moving them to the Recycle Bin instead."
            />
          </div>
        ) : null}

        {withContent.length === 0 ? (
          <EmptyState
            title="Nothing was found in the selected categories"
            detail="There is no recoverable data here right now."
          />
        ) : (
          withContent.map((category) => (
            <CategoryPreview
              key={category.id}
              category={category}
              plan={plan}
              selection={selection}
              expanded={expanded === category.id}
              items={expanded === category.id ? expandedItems : []}
              onToggleAll={(on) => toggleCategorySelection(category.id, on)}
              onToggleExpand={() => void loadItems(category.id)}
              onToggleItem={(path, on) => {
                const index = plan.items.findIndex((item) => item.path === path);
                if (index < 0) return;
                setSelection((current) => {
                  const next = new Set(current);
                  if (on) next.add(index);
                  else next.delete(index);
                  return next;
                });
              }}
              onReveal={(path) => void api.revealInExplorer(path).catch(reportError)}
            />
          ))
        )}

        <ContentDialog
          open={confirmOpen}
          title={
            method === "permanent"
              ? "Delete these items permanently?"
              : "Clean the selected items?"
          }
          primaryText={method === "permanent" ? "Delete permanently" : "Continue"}
          cancelText="Cancel"
          destructive={method === "permanent"}
          onPrimary={runCleanup}
          onCancel={() => setConfirmOpen(false)}
        >
          <div className="stack">
            <InfoRow label="Method" value={methodLabel(method)} />
            <InfoRow label="Files" value={formatCount(selectedItems.length)} />
            <InfoRow label="Selected size" value={formatBytes(selectedBytes)} />
            <p className="text-secondary" style={{ margin: 0 }}>
              {method === "permanent"
                ? "These files will be removed without going to the Recycle Bin. This cannot be reversed."
                : method === "quarantine"
                  ? "Files move to Stora quarantine and can be restored until the retention period ends."
                  : "Files move to the Recycle Bin and can be restored from there."}{" "}
              Items in use by another program are skipped and reported.
            </p>
          </div>
        </ContentDialog>
      </>
    );
  }

  // --------------------------------------------------------------- running

  if (stage === "running") {
    const completed = progress?.completed ?? 0;
    const total = progress?.total ?? selectedItems.length;

    return (
      <>
        <PageHeader title="Cleaning selected items" />
        <SettingsSection>
          <div style={{ padding: "12px 14px" }}>
            <ProgressBar
              value={completed}
              max={Math.max(total, 1)}
              label={`${formatCount(completed)} of ${formatCount(total)}`}
            />
            <div className="progress-readout">
              <InfoRow
                label="Completed"
                value={`${formatCount(completed)} of ${formatCount(total)}`}
              />
              <InfoRow
                label="Recovered"
                value={formatBytes(progress?.recoveredBytes ?? 0)}
              />
              <InfoRow label="Skipped" value={formatCount(progress?.errors ?? 0)} />
              <InfoRow
                label="Elapsed"
                value={formatElapsed(progress?.elapsedMs ?? 0)}
              />
            </div>
            <p className="path-text" style={{ marginTop: 8 }}>
              {shortenPath(progress?.currentPath ?? "")}
            </p>
          </div>
        </SettingsSection>
      </>
    );
  }

  // ---------------------------------------------------------------- result

  if (stage === "result" && result) {
    return (
      <>
        <PageHeader
          title={
            result.state === "completedWithErrors"
              ? "Cleanup completed with skipped items"
              : "Cleanup completed"
          }
          actions={
            <Button variant="primary" onClick={restart}>
              Done
            </Button>
          }
        />

        <SettingsSection>
          <InfoRow
            label="Space recovered"
            value={formatBytes(result.recoveredBytes)}
            help="Counts only files that were actually removed."
          />
          <InfoRow label="Files removed" value={formatCount(result.filesRemoved)} />
          <InfoRow label="Files skipped" value={formatCount(result.filesSkipped)} />
          <InfoRow label="Method" value={methodLabel(result.method)} />
          <InfoRow label="Duration" value={formatDuration(result.durationMs)} />
        </SettingsSection>

        {result.errors.length > 0 ? (
          <section className="page-section">
            <SectionHeader>Skipped items ({result.errors.length})</SectionHeader>
            <DataGrid
              ariaLabel="Items that were not removed"
              rows={result.errors}
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
                  width: 300,
                  render: (row) =>
                    api.describeError({
                      code: row.code,
                      message: row.message,
                      path: null,
                    }).title,
                },
              ]}
            />
          </section>
        ) : null}
      </>
    );
  }

  return null;
}

interface CategoryPreviewProps {
  category: CleanupCategoryResult;
  plan: CleanupPlan;
  selection: Set<number>;
  expanded: boolean;
  items: CleanupItem[];
  onToggleAll: (on: boolean) => void;
  onToggleExpand: () => void;
  onToggleItem: (path: string, on: boolean) => void;
  onReveal: (path: string) => void;
}

function CategoryPreview({
  category,
  plan,
  selection,
  expanded,
  items,
  onToggleAll,
  onToggleExpand,
  onToggleItem,
  onReveal,
}: CategoryPreviewProps) {
  const indices = useMemo(
    () =>
      plan.items
        .map((item, index) => (item.categoryId === category.id ? index : -1))
        .filter((index) => index >= 0),
    [plan, category.id],
  );

  const selectedCount = indices.filter((index) => selection.has(index)).length;
  const allSelected = indices.length > 0 && selectedCount === indices.length;

  const selectedPaths = useMemo(
    () => new Set(indices.filter((i) => selection.has(i)).map((i) => plan.items[i].path)),
    [indices, selection, plan],
  );

  return (
    <section className="page-section">
      <SettingsSection>
        <SettingsRow
          title={category.name}
          description={category.explanation}
          note={
            <span className="cleanup-row__meta">
              <RiskBadge risk={category.risk} />
              {category.unavailableReason ? (
                <span className="text-secondary">{category.unavailableReason}</span>
              ) : (
                <span className="text-secondary numeric">
                  {formatCount(category.fileCount)} files ·{" "}
                  {formatCount(selectedCount)} selected
                </span>
              )}
            </span>
          }
          control={
            <span className="cleanup-row__control">
              <span className="cleanup-row__size">{formatBytes(category.bytes)}</span>
              {indices.length > 0 ? (
                <>
                  <ToggleSwitch
                    label={`Select all in ${category.name}`}
                    checked={allSelected}
                    onChange={onToggleAll}
                  />
                  <Button variant="subtle" onClick={onToggleExpand}>
                    {expanded ? "Hide files" : "View files"}
                  </Button>
                </>
              ) : null}
            </span>
          }
        />
      </SettingsSection>

      {expanded && items.length > 0 ? (
        <div style={{ marginTop: 8 }}>
          <DataGrid
            ariaLabel={`Files in ${category.name}`}
            rows={items}
            rowKey={(row) => row.path}
            emptyMessage="No files in this category."
            columns={[
              {
                id: "selected",
                header: "Include",
                width: 90,
                render: (row) => (
                  <ToggleSwitch
                    label={`Include ${row.path}`}
                    checked={selectedPaths.has(row.path)}
                    onChange={(on) => onToggleItem(row.path, on)}
                  />
                ),
              },
              {
                id: "path",
                header: "File",
                render: (row) => (
                  <Tooltip content={row.path}>
                    <span>{shortenPath(row.path, 54)}</span>
                  </Tooltip>
                ),
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
                id: "modified",
                header: "Modified",
                width: 160,
                render: (row) => (
                  <span className="numeric">{formatTimestamp(row.modified)}</span>
                ),
              },
              {
                id: "actions",
                header: "",
                width: 110,
                render: (row) => (
                  <Button variant="subtle" onClick={() => onReveal(row.path)}>
                    Open location
                  </Button>
                ),
              },
            ]}
          />
          {items.length >= 500 ? (
            <p className="text-secondary" style={{ marginTop: 6 }}>
              Showing the first 500 files in this category. Selecting the category
              includes all {formatCount(category.fileCount)} of them.
            </p>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
