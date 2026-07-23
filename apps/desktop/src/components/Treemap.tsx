/**
 * Storage treemap.
 *
 * Only the layout maths come from `d3-hierarchy` — no DOM, no charting
 * library, no theming of its own. Rectangles are drawn here so the result
 * matches the rest of the interface and stays usable in high contrast.
 *
 * This is deliberately a *secondary* view. The folder list beside it carries
 * the same information, remains the accessible reference, and is what a
 * screen reader is pointed at.
 */

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { hierarchy, treemap, treemapSquarify } from "d3-hierarchy";

import { formatBytes } from "../lib/format";

/** Above this many rectangles, SVG nodes cost more than they are worth. */
const CANVAS_THRESHOLD = 2000;

/** Siblings below this share of the parent are folded into one bucket. */
const OTHER_SHARE = 0.01;

/** Never draw more individual rectangles than this, "Other" aside. */
const MAX_RECTS = 6000;

export interface TreemapNode {
  /** Empty for the aggregated "Other" bucket, which cannot be opened. */
  path: string;
  name: string;
  bytes: number;
  hasChildren: boolean;
}

interface LaidOutRect {
  node: TreemapNode;
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

interface TreemapProps {
  nodes: TreemapNode[];
  /** Highlighted to keep the treemap and the list in step. */
  selectedPath: string | null;
  onSelect: (node: TreemapNode) => void;
  /** Fired when a node with children is activated. */
  onDrillDown: (node: TreemapNode) => void;
  height?: number;
}

/**
 * Folds the long tail of small siblings into a single bucket.
 *
 * A folder can hold thousands of children whose rectangles would be a pixel
 * wide — unreadable, unclickable, and expensive. Collapsing them keeps the
 * total honest while the list remains available for the detail.
 */
export function foldSmallNodes(nodes: TreemapNode[]): TreemapNode[] {
  const total = nodes.reduce((sum, node) => sum + node.bytes, 0);
  if (total === 0) return [];

  const sorted = [...nodes].sort((a, b) => b.bytes - a.bytes);
  const kept: TreemapNode[] = [];
  let foldedBytes = 0;
  let foldedCount = 0;

  for (const node of sorted) {
    const tooSmall = node.bytes / total < OTHER_SHARE;
    const tooMany = kept.length >= MAX_RECTS;

    if ((tooSmall || tooMany) && node.bytes > 0) {
      foldedBytes += node.bytes;
      foldedCount += 1;
    } else if (node.bytes > 0) {
      kept.push(node);
    }
  }

  if (foldedCount > 0) {
    kept.push({
      path: "",
      name: `Other (${foldedCount.toLocaleString()})`,
      bytes: foldedBytes,
      hasChildren: false,
    });
  }

  return kept;
}

/** Runs the squarified layout over a flat set of siblings. */
export function layoutRects(
  nodes: TreemapNode[],
  width: number,
  height: number,
): LaidOutRect[] {
  if (nodes.length === 0 || width <= 0 || height <= 0) return [];

  const root = hierarchy<{ node?: TreemapNode; children?: unknown[] }>({
    children: nodes.map((node) => ({ node })),
  } as never)
    .sum((datum) => (datum as { node?: TreemapNode }).node?.bytes ?? 0)
    .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));

  treemap<{ node?: TreemapNode }>()
    .tile(treemapSquarify)
    .size([width, height])
    .paddingInner(2)
    .round(true)(root as never);

  return (root.leaves() as unknown as Array<{
    data: { node?: TreemapNode };
    x0: number;
    y0: number;
    x1: number;
    y1: number;
  }>)
    .filter((leaf) => leaf.data.node !== undefined)
    .map((leaf) => ({
      node: leaf.data.node as TreemapNode,
      x0: leaf.x0,
      y0: leaf.y0,
      x1: leaf.x1,
      y1: leaf.y1,
    }));
}

export default function Treemap({
  nodes,
  selectedPath,
  onSelect,
  onDrillDown,
  height = 320,
}: TreemapProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const [hovered, setHovered] = useState<TreemapNode | null>(null);
  const [focusIndex, setFocusIndex] = useState(0);

  const folded = useMemo(() => foldSmallNodes(nodes), [nodes]);
  const rects = useMemo(
    () => layoutRects(folded, width, height),
    [folded, width, height],
  );
  const useCanvas = rects.length > CANVAS_THRESHOLD;

  // Track the container width so the layout follows the window.
  useLayoutEffect(() => {
    const element = containerRef.current;
    if (!element) return;

    const observer = new ResizeObserver((entries) => {
      const measured = entries[0]?.contentRect.width ?? 0;
      setWidth(Math.floor(measured));
    });
    observer.observe(element);
    setWidth(Math.floor(element.getBoundingClientRect().width));

    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    setFocusIndex(0);
  }, [nodes]);

  const colorFor = useCallback((index: number, isSelected: boolean) => {
    const styles = getComputedStyle(document.documentElement);
    const accent =
      styles.getPropertyValue("--memora-accent-usable").trim() || "#0078d4";
    if (isSelected) return accent;

    // A single accent stepped through lightness, so the map reads as one
    // family rather than an arbitrary palette.
    const shade = 26 + ((index * 11) % 34);
    return `color-mix(in srgb, ${accent} ${shade}%, transparent)`;
  }, []);

  // Canvas path, used once there are too many rectangles for SVG.
  useEffect(() => {
    if (!useCanvas) return;
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ratio = window.devicePixelRatio || 1;
    canvas.width = width * ratio;
    canvas.height = height * ratio;

    const context = canvas.getContext("2d");
    if (!context) return;

    context.scale(ratio, ratio);
    context.clearRect(0, 0, width, height);

    const styles = getComputedStyle(document.documentElement);
    const stroke = styles.getPropertyValue("--memora-stroke").trim() || "#8884";
    const accent =
      styles.getPropertyValue("--memora-accent-usable").trim() || "#0078d4";

    rects.forEach((rect, index) => {
      const isSelected = rect.node.path !== "" && rect.node.path === selectedPath;
      context.fillStyle = isSelected
        ? accent
        : `rgba(0, 120, 212, ${0.22 + ((index * 7) % 30) / 100})`;
      context.fillRect(rect.x0, rect.y0, rect.x1 - rect.x0, rect.y1 - rect.y0);
      context.strokeStyle = stroke;
      context.lineWidth = 1;
      context.strokeRect(rect.x0, rect.y0, rect.x1 - rect.x0, rect.y1 - rect.y0);
    });
  }, [useCanvas, rects, width, height, selectedPath]);

  const activate = (node: TreemapNode) => {
    // The aggregated bucket is not a real folder and cannot be opened.
    if (node.path === "") return;
    onSelect(node);
    if (node.hasChildren) onDrillDown(node);
  };

  const onKeyDown = (event: React.KeyboardEvent) => {
    if (rects.length === 0) return;

    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      event.preventDefault();
      setFocusIndex((index) => Math.min(index + 1, rects.length - 1));
    } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      event.preventDefault();
      setFocusIndex((index) => Math.max(index - 1, 0));
    } else if (event.key === "Home") {
      event.preventDefault();
      setFocusIndex(0);
    } else if (event.key === "End") {
      event.preventDefault();
      setFocusIndex(rects.length - 1);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      const rect = rects[focusIndex];
      if (rect) activate(rect.node);
    }
  };

  const focused = rects[focusIndex]?.node ?? null;

  if (nodes.length === 0) {
    return null;
  }

  return (
    <div className="treemap" ref={containerRef}>
      <div
        className="treemap__surface"
        style={{ height }}
        role="application"
        aria-label="Storage treemap. The folder list below carries the same information."
        tabIndex={0}
        onKeyDown={onKeyDown}
        onMouseLeave={() => setHovered(null)}
      >
        {useCanvas ? (
          <canvas
            ref={canvasRef}
            style={{ width, height }}
            onClick={(event) => {
              const bounds = event.currentTarget.getBoundingClientRect();
              const x = event.clientX - bounds.left;
              const y = event.clientY - bounds.top;
              const hit = rects.find(
                (rect) => x >= rect.x0 && x <= rect.x1 && y >= rect.y0 && y <= rect.y1,
              );
              if (hit) activate(hit.node);
            }}
            onMouseMove={(event) => {
              const bounds = event.currentTarget.getBoundingClientRect();
              const x = event.clientX - bounds.left;
              const y = event.clientY - bounds.top;
              const hit = rects.find(
                (rect) => x >= rect.x0 && x <= rect.x1 && y >= rect.y0 && y <= rect.y1,
              );
              setHovered(hit?.node ?? null);
            }}
          />
        ) : (
          <svg width={width} height={height} aria-hidden="true" focusable="false">
            {rects.map((rect, index) => {
              const isSelected =
                rect.node.path !== "" && rect.node.path === selectedPath;
              const isFocused = index === focusIndex;
              const rectWidth = rect.x1 - rect.x0;
              const rectHeight = rect.y1 - rect.y0;

              return (
                <g key={rect.node.path || `other-${index}`}>
                  <rect
                    x={rect.x0}
                    y={rect.y0}
                    width={rectWidth}
                    height={rectHeight}
                    rx={2}
                    className={`treemap__rect${
                      isSelected ? " treemap__rect--selected" : ""
                    }${isFocused ? " treemap__rect--focused" : ""}`}
                    fill={colorFor(index, isSelected)}
                    onClick={() => activate(rect.node)}
                    onMouseEnter={() => setHovered(rect.node)}
                  />
                  {/* Only label a rectangle big enough to read. */}
                  {rectWidth > 54 && rectHeight > 22 ? (
                    <text
                      x={rect.x0 + 6}
                      y={rect.y0 + 15}
                      className="treemap__label"
                      pointerEvents="none"
                    >
                      {rect.node.name.length > Math.floor(rectWidth / 7)
                        ? `${rect.node.name.slice(0, Math.floor(rectWidth / 7))}…`
                        : rect.node.name}
                    </text>
                  ) : null}
                </g>
              );
            })}
          </svg>
        )}
      </div>

      <div className="treemap__readout" aria-live="polite">
        {hovered ?? focused ? (
          <>
            <span className="treemap__readout-name">
              {(hovered ?? focused)?.name}
            </span>
            <span className="numeric">
              {formatBytes((hovered ?? focused)?.bytes ?? 0)}
            </span>
            {(hovered ?? focused)?.path ? (
              <span className="path-text">{(hovered ?? focused)?.path}</span>
            ) : (
              <span className="text-secondary">
                Grouped small folders. Use the list below to see them.
              </span>
            )}
          </>
        ) : (
          <span className="text-secondary">
            {rects.length} block{rects.length === 1 ? "" : "s"} · arrow keys to move,
            Enter to open
          </span>
        )}
      </div>
    </div>
  );
}
