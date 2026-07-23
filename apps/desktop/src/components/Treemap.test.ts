import { describe, expect, it } from "vitest";

import { foldSmallNodes, layoutRects, type TreemapNode } from "./Treemap";

function node(name: string, bytes: number, hasChildren = false): TreemapNode {
  return { path: `C:\\${name}`, name, bytes, hasChildren };
}

describe("foldSmallNodes", () => {
  it("keeps every node when they are all a meaningful share", () => {
    const folded = foldSmallNodes([
      node("a", 500),
      node("b", 300),
      node("c", 200),
    ]);

    expect(folded).toHaveLength(3);
    expect(folded.some((entry) => entry.path === "")).toBe(false);
  });

  it("folds the long tail into a single Other bucket", () => {
    // One dominant folder plus fifty tiny ones: the tiny rectangles would be
    // a pixel wide and impossible to click.
    const nodes = [node("big", 1_000_000)];
    for (let i = 0; i < 50; i += 1) nodes.push(node(`tiny${i}`, 100));

    const folded = foldSmallNodes(nodes);

    expect(folded).toHaveLength(2);
    expect(folded[0].name).toBe("big");

    const other = folded[1];
    expect(other.path).toBe("");
    expect(other.name).toContain("Other");
    expect(other.name).toContain("50");
    expect(other.bytes).toBe(50 * 100);
  });

  it("preserves the total size across the fold", () => {
    const nodes = [node("big", 900_000), node("small", 500), node("tiny", 300)];
    const before = nodes.reduce((sum, entry) => sum + entry.bytes, 0);
    const after = foldSmallNodes(nodes).reduce((sum, entry) => sum + entry.bytes, 0);

    expect(after).toBe(before);
  });

  it("drops zero-byte folders rather than drawing invisible blocks", () => {
    const folded = foldSmallNodes([node("real", 1000), node("empty", 0)]);
    expect(folded).toHaveLength(1);
    expect(folded[0].name).toBe("real");
  });

  it("returns nothing when every folder is empty", () => {
    expect(foldSmallNodes([node("a", 0), node("b", 0)])).toEqual([]);
  });

  it("returns nothing for an empty input", () => {
    expect(foldSmallNodes([])).toEqual([]);
  });

  it("orders the result largest first", () => {
    const folded = foldSmallNodes([node("small", 200_000), node("large", 800_000)]);
    expect(folded[0].name).toBe("large");
  });

  it("marks the Other bucket as not drillable", () => {
    const nodes = [node("big", 1_000_000)];
    for (let i = 0; i < 20; i += 1) nodes.push(node(`tiny${i}`, 50));

    const other = foldSmallNodes(nodes).find((entry) => entry.path === "");
    expect(other?.hasChildren).toBe(false);
  });
});

describe("layoutRects", () => {
  it("lays every node inside the given bounds", () => {
    const rects = layoutRects(
      [node("a", 500), node("b", 300), node("c", 200)],
      400,
      200,
    );

    expect(rects).toHaveLength(3);
    for (const rect of rects) {
      expect(rect.x0).toBeGreaterThanOrEqual(0);
      expect(rect.y0).toBeGreaterThanOrEqual(0);
      expect(rect.x1).toBeLessThanOrEqual(400);
      expect(rect.y1).toBeLessThanOrEqual(200);
      expect(rect.x1).toBeGreaterThanOrEqual(rect.x0);
      expect(rect.y1).toBeGreaterThanOrEqual(rect.y0);
    }
  });

  it("gives a larger folder a larger rectangle", () => {
    const rects = layoutRects([node("big", 900), node("small", 100)], 400, 200);

    const area = (r: (typeof rects)[number]) => (r.x1 - r.x0) * (r.y1 - r.y0);
    const big = rects.find((r) => r.node.name === "big")!;
    const small = rects.find((r) => r.node.name === "small")!;

    expect(area(big)).toBeGreaterThan(area(small));
  });

  it("returns nothing when there is no space to draw in", () => {
    const nodes = [node("a", 500)];
    expect(layoutRects(nodes, 0, 200)).toEqual([]);
    expect(layoutRects(nodes, 400, 0)).toEqual([]);
  });

  it("returns nothing for an empty node set", () => {
    expect(layoutRects([], 400, 200)).toEqual([]);
  });

  it("survives a folder that vanished between scan and render", () => {
    // A stale node still carries a size from the index; the layout must place
    // it rather than throwing, and the click handler is what refuses it.
    const stale: TreemapNode = {
      path: "C:\\Deleted\\Since\\Scan",
      name: "Since",
      bytes: 4096,
      hasChildren: true,
    };

    const rects = layoutRects([stale, node("still-here", 8192)], 400, 200);

    expect(rects).toHaveLength(2);
    expect(rects.some((rect) => rect.node.path === stale.path)).toBe(true);
  });

  it("carries the original node through to the caller", () => {
    const original = node("project", 1234, true);
    const [rect] = layoutRects([original], 400, 200);

    expect(rect.node.path).toBe(original.path);
    expect(rect.node.hasChildren).toBe(true);
    expect(rect.node.bytes).toBe(1234);
  });
});
