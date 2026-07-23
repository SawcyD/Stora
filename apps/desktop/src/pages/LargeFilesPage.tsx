import { useCallback, useEffect, useState } from "react";
import {
  Button,
  ComboBox,
  CommandBar,
  CommandGroup,
  ContextMenu,
  DataGrid,
  SearchBox,
  Tooltip,
  type DataGridSort,
  type MenuAction,
} from "@sawcy/memora-ui";

import { EmptyState, PageHeader } from "../components/common";
import AdvisorDialog from "../components/AdvisorDialog";
import * as api from "../lib/api";
import { formatBytes, formatTimestamp, shortenPath } from "../lib/format";
import type { LargeFile } from "../lib/types";
import { useApp } from "../state/AppContext";

const MEGABYTE = 1024 * 1024;

const THRESHOLDS = [
  { value: 100 * MEGABYTE, label: "Larger than 100 MB" },
  { value: 500 * MEGABYTE, label: "Larger than 500 MB" },
  { value: 1024 * MEGABYTE, label: "Larger than 1 GB" },
  { value: 5 * 1024 * MEGABYTE, label: "Larger than 5 GB" },
];

const TYPE_FILTERS: Record<string, string[]> = {
  installers: ["exe", "msi", "msix", "appx", "msu"],
  archives: ["zip", "7z", "rar", "tar", "gz", "cab"],
  videos: ["mp4", "mkv", "mov", "avi", "wmv"],
  diskImages: ["iso", "img", "dmg"],
  virtualMachines: ["vhd", "vhdx", "vmdk", "vdi", "qcow2"],
  databases: ["db", "sqlite", "mdf", "bak"],
};

// Deletion is deliberately not on this list: this page is for understanding
// what is large, and removal happens through the reviewed cleanup flow.
const ROW_ACTIONS: MenuAction[] = [
  { id: "advisor", label: "Ask Stora" },
  { id: "open", label: "Open location" },
  { id: "copy", label: "Copy path" },
  { id: "exclude", label: "Exclude from future scans", dividerBefore: true },
];

export default function LargeFilesPage() {
  const { selectedDrive, scanSummary, scanRevision, notify, reportError } = useApp();

  const [files, setFiles] = useState<LargeFile[]>([]);
  const [threshold, setThreshold] = useState(THRESHOLDS[0].value);
  const [typeFilter, setTypeFilter] = useState("all");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(false);
  const [sort, setSort] = useState<DataGridSort>({
    columnId: "size",
    direction: "descending",
  });
  const [menu, setMenu] = useState<{ x: number; y: number; file: LargeFile } | null>(
    null,
  );
  const [advising, setAdvising] = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!selectedDrive || !scanSummary) {
      setFiles([]);
      return;
    }
    setLoading(true);
    try {
      setFiles(await api.getLargeFiles(selectedDrive.root, threshold, 1000));
    } catch (error) {
      setFiles([]);
      reportError(error);
    } finally {
      setLoading(false);
    }
  }, [selectedDrive, scanSummary, threshold, reportError]);

  useEffect(() => {
    void load();
  }, [load, scanRevision]);

  const onAction = async (id: string, file: LargeFile) => {
    setMenu(null);
    try {
      if (id === "advisor") {
        setAdvising(file.path);
      } else if (id === "open") {
        await api.revealInExplorer(file.path);
      } else if (id === "copy") {
        await navigator.clipboard.writeText(file.path);
        notify({ tone: "info", title: "Path copied to the clipboard." });
      } else if (id === "exclude") {
        await api.createExclusion(file.path, "file");
        notify({ tone: "success", title: "File excluded from future scans." });
      }
    } catch (error) {
      reportError(error);
    }
  };

  if (!scanSummary) {
    return (
      <>
        <PageHeader title="Large files" />
        <EmptyState
          title="No scan results for this drive yet"
          detail="Run a scan from the Home page to list the largest files."
        />
      </>
    );
  }

  const extensions = TYPE_FILTERS[typeFilter];
  const term = search.trim().toLowerCase();

  const visible = files
    .filter((file) => !extensions || (file.extension && extensions.includes(file.extension)))
    .filter((file) => !term || file.path.toLowerCase().includes(term))
    .sort((a, b) => {
      const direction = sort.direction === "ascending" ? 1 : -1;
      switch (sort.columnId) {
        case "name":
          return a.name.localeCompare(b.name) * direction;
        case "modified":
          return ((a.modified ?? 0) - (b.modified ?? 0)) * direction;
        case "location":
          return a.path.localeCompare(b.path) * direction;
        default:
          return (a.logicalSize - b.logicalSize) * direction;
      }
    });

  return (
    <>
      <PageHeader
        title="Large files"
        description="The biggest individual files on this drive. Review each one before acting."
      />

      <CommandBar>
        <CommandGroup>
          <ComboBox
            label="Minimum size"
            value={threshold}
            options={THRESHOLDS}
            onChange={(value) => setThreshold(Number(value))}
          />
          <ComboBox
            label="File type"
            value={typeFilter}
            options={[
              { value: "all", label: "All types" },
              { value: "installers", label: "Installers" },
              { value: "archives", label: "Archives" },
              { value: "videos", label: "Videos" },
              { value: "diskImages", label: "Disk images" },
              { value: "virtualMachines", label: "Virtual machines" },
              { value: "databases", label: "Databases" },
            ]}
            onChange={(value) => setTypeFilter(String(value))}
          />
        </CommandGroup>
        <CommandGroup>
          <SearchBox
            label="Search files"
            placeholder="Search by name or path"
            value={search}
            onChange={setSearch}
          />
        </CommandGroup>
      </CommandBar>

      {loading ? (
        <EmptyState title="Loading files…" />
      ) : (
        <DataGrid
          ariaLabel="Large files"
          rows={visible}
          rowKey={(row) => row.path}
          sort={sort}
          onSortChange={setSort}
          onRowContextMenu={(row, event) => {
            event.preventDefault();
            setMenu({ x: event.clientX, y: event.clientY, file: row });
          }}
          emptyMessage={
            files.length === 0
              ? "No files on this drive are larger than the selected size."
              : "No files match these filters."
          }
          columns={[
            {
              id: "name",
              header: "Name",
              sortable: true,
              render: (row) => (
                <Tooltip content={row.path}>
                  <span>{row.name}</span>
                </Tooltip>
              ),
            },
            {
              id: "size",
              header: "Size",
              sortable: true,
              width: 110,
              align: "end",
              render: (row) => (
                <span className="numeric">{formatBytes(row.logicalSize)}</span>
              ),
            },
            {
              id: "allocated",
              header: "Size on disk",
              width: 120,
              align: "end",
              render: (row) => (
                <span className="numeric">{formatBytes(row.allocatedSize)}</span>
              ),
            },
            {
              id: "location",
              header: "Location",
              sortable: true,
              render: (row) => (
                <span className="path-text">{shortenPath(row.path, 46)}</span>
              ),
            },
            {
              id: "modified",
              header: "Modified",
              sortable: true,
              width: 150,
              render: (row) => (
                <span className="numeric">{formatTimestamp(row.modified)}</span>
              ),
            },
            {
              id: "actions",
              header: "",
              width: 120,
              render: (row) => (
                <Button
                  variant="subtle"
                  onClick={() => void api.revealInExplorer(row.path).catch(reportError)}
                >
                  Open location
                </Button>
              ),
            },
          ]}
        />
      )}

      {menu ? (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          actions={ROW_ACTIONS}
          onSelect={(id) => void onAction(id, menu.file)}
          onDismiss={() => setMenu(null)}
        />
      ) : null}
      {advising ? <AdvisorDialog path={advising} onClose={() => setAdvising(null)} /> : null}
    </>
  );
}
