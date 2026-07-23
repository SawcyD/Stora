import { useCallback, useEffect, useState } from "react";
import {
  Button,
  CommandBar,
  CommandGroup,
  ContextMenu,
  SearchBox,
  ToggleSwitch,
  type MenuAction,
} from "@sawcy/memora-ui";

import { EmptyState, PageHeader } from "../components/common";
import Treemap, { type TreemapNode } from "../components/Treemap";
import LocationExplanation from "../components/LocationExplanation";
import AdvisorDialog from "../components/AdvisorDialog";
import { ChevronRightIcon, FolderIcon, RefreshIcon } from "../components/icons";
import * as api from "../lib/api";
import { formatBytes, formatCount, formatTimestamp } from "../lib/format";
import type { FolderAggregate } from "../lib/types";
import { useApp } from "../state/AppContext";

interface MenuState {
  x: number;
  y: number;
  folder: FolderAggregate;
}

const ROW_ACTIONS: MenuAction[] = [
  { id: "advisor", label: "Ask Stora" },
  { id: "explain", label: "Why is this here?" },
  { id: "open", label: "Open in File Explorer", dividerBefore: true },
  { id: "copy", label: "Copy path" },
  { id: "exclude", label: "Exclude from future scans", dividerBefore: true },
];

export default function StoragePage() {
  const { selectedDrive, scanSummary, scanRevision, notify, reportError } = useApp();

  const [path, setPath] = useState<string | null>(null);
  const [children, setChildren] = useState<FolderAggregate[]>([]);
  const [loading, setLoading] = useState(false);
  const [search, setSearch] = useState("");
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [showTreemap, setShowTreemap] = useState(true);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [explaining, setExplaining] = useState<string | null>(null);
  const [advising, setAdvising] = useState<string | null>(null);

  const root = selectedDrive?.root ?? null;
  const currentPath = path ?? root;

  const load = useCallback(
    async (target: string) => {
      if (!root) return;
      setLoading(true);
      try {
        // The tree loads one level at a time; a full drive is far too large
        // to render in a single pass.
        setChildren(await api.getFolderChildren(root, target));
      } catch (error) {
        setChildren([]);
        reportError(error);
      } finally {
        setLoading(false);
      }
    },
    [root, reportError],
  );

  useEffect(() => {
    setPath(null);
  }, [root]);

  useEffect(() => {
    if (currentPath) void load(currentPath);
  }, [currentPath, load, scanRevision]);

  const onAction = async (id: string, folder: FolderAggregate) => {
    setMenu(null);
    try {
      if (id === "advisor") {
        setAdvising(folder.path);
      } else if (id === "explain") {
        setExplaining(folder.path);
      } else if (id === "open") {
        await api.revealInExplorer(folder.path);
      } else if (id === "copy") {
        await navigator.clipboard.writeText(folder.path);
        notify({ tone: "info", title: "Path copied to the clipboard." });
      } else if (id === "exclude") {
        await api.createExclusion(folder.path, "folder");
        notify({
          tone: "success",
          title: "Folder excluded from future scans.",
          detail: folder.path,
        });
      }
    } catch (error) {
      reportError(error);
    }
  };

  if (!selectedDrive) {
    return (
      <>
        <PageHeader title="Storage" />
        <EmptyState title="Select a drive on the Home page to browse its folders." />
      </>
    );
  }

  if (!scanSummary) {
    return (
      <>
        <PageHeader title="Storage" description="Browse folders by actual storage use." />
        <EmptyState
          title="No scan results for this drive yet"
          detail="Run a scan from the Home page to build the folder tree."
        />
      </>
    );
  }

  const crumbs = buildCrumbs(currentPath ?? "", selectedDrive.root);
  const visible = search.trim()
    ? children.filter((child) =>
        child.name.toLowerCase().includes(search.trim().toLowerCase()),
      )
    : children;

  const treemapNodes: TreemapNode[] = visible.map((child) => ({
    path: child.path,
    name: child.name,
    bytes: child.allocatedSize,
    hasChildren: child.hasChildren,
  }));

  const largest = children.reduce(
    (max, child) => Math.max(max, child.allocatedSize),
    0,
  );

  return (
    <>
      <PageHeader
        title="Storage"
        description="Folders are ordered by the space they occupy on disk."
      />

      <CommandBar>
        <CommandGroup>
          <SearchBox
            label="Filter folders"
            placeholder="Filter this folder"
            value={search}
            onChange={setSearch}
          />
        </CommandGroup>
        <CommandGroup>
          <ToggleSwitch
            label="Show treemap"
            checked={showTreemap}
            onChange={setShowTreemap}
          />
          <Button
            variant="subtle"
            onClick={() => currentPath && void load(currentPath)}
          >
            <RefreshIcon /> Refresh
          </Button>
        </CommandGroup>
      </CommandBar>

      <nav className="tree__breadcrumb" aria-label="Folder path">
        {crumbs.map((crumb, index) => (
          <span key={crumb.path} className="row" style={{ gap: 2 }}>
            {index > 0 ? (
              <span className="tree__separator" aria-hidden="true">
                <ChevronRightIcon size={12} />
              </span>
            ) : null}
            <button
              type="button"
              className="tree__crumb"
              aria-current={index === crumbs.length - 1 ? "page" : undefined}
              onClick={() => setPath(crumb.path)}
            >
              {crumb.label}
            </button>
          </span>
        ))}
      </nav>

      {showTreemap && treemapNodes.length > 0 ? (
        <Treemap
          nodes={treemapNodes}
          selectedPath={selectedPath}
          onSelect={(node) => setSelectedPath(node.path)}
          onDrillDown={(node) => setPath(node.path)}
        />
      ) : null}

      <div className="tree">
        <div className="tree__row tree__row--header" role="row">
          <span>Name</span>
          <span className="tree__numeric">Size on disk</span>
          <span className="tree__numeric">Files</span>
          <span className="tree__numeric">Folders</span>
          <span className="tree__numeric">Last modified</span>
        </div>

        {loading ? (
          <EmptyState title="Loading folders…" />
        ) : visible.length === 0 ? (
          <EmptyState
            title={
              search.trim()
                ? "No folders match this filter."
                : "This folder contains no subfolders."
            }
            detail={
              search.trim()
                ? undefined
                : "Individual files are listed on the Large files page."
            }
          />
        ) : (
          visible.map((child) => (
            <button
              key={child.path}
              type="button"
              className="tree__row"
              onClick={() => child.hasChildren && setPath(child.path)}
              onContextMenu={(event) => {
                event.preventDefault();
                setMenu({ x: event.clientX, y: event.clientY, folder: child });
              }}
              aria-label={`${child.name}, ${formatBytes(child.allocatedSize)}`}
            >
              <span className="tree__name">
                <span className="tree__chevron">
                  {child.hasChildren ? <ChevronRightIcon size={12} /> : null}
                </span>
                <FolderIcon />
                <span title={child.path}>{child.name}</span>
              </span>
              <span className="tree__numeric">
                {formatBytes(child.allocatedSize)}
                <span className="visually-hidden">
                  {largest > 0
                    ? `, ${Math.round((child.allocatedSize / largest) * 100)}% of the largest folder here`
                    : ""}
                </span>
              </span>
              <span className="tree__numeric">{formatCount(child.fileCount)}</span>
              <span className="tree__numeric">{formatCount(child.folderCount)}</span>
              <span className="tree__numeric">{formatTimestamp(child.modified)}</span>
            </button>
          ))
        )}
      </div>

      {explaining ? (
        <LocationExplanation
          path={explaining}
          onClose={() => setExplaining(null)}
        />
      ) : null}

      {advising ? <AdvisorDialog path={advising} onClose={() => setAdvising(null)} /> : null}

      {menu ? (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          actions={ROW_ACTIONS}
          onSelect={(id) => void onAction(id, menu.folder)}
          onDismiss={() => setMenu(null)}
        />
      ) : null}
    </>
  );
}

function buildCrumbs(path: string, root: string) {
  if (!path) return [];

  const normalizedRoot = root.replace(/\\$/, "");
  const crumbs = [{ label: root.replace("\\", ""), path: root }];

  if (path.replace(/\\$/, "").toLowerCase() === normalizedRoot.toLowerCase()) {
    return crumbs;
  }

  const remainder = path.slice(root.length).split("\\").filter(Boolean);
  let accumulated = normalizedRoot;

  for (const part of remainder) {
    accumulated = `${accumulated}\\${part}`;
    crumbs.push({ label: part, path: accumulated });
  }

  return crumbs;
}
