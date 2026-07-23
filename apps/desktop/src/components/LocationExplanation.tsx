/**
 * "Why is this here?" — the curated explanation for a location.
 *
 * Everything shown comes from a checked-in file that ships with Stora. There
 * is no model, no network call, and no score: a location either has a
 * hand-written entry with a citation, or it does not, and saying so is a
 * better answer than a guess dressed up as a percentage.
 */

import { useEffect, useState } from "react";
import { ContentDialog, InfoBar, InfoRow } from "@sawcy/memora-ui";

import { EmptyState } from "./common";
import * as api from "../lib/api";
import type { Explanation } from "../lib/types";

interface LocationExplanationProps {
  path: string;
  onClose: () => void;
}

export default function LocationExplanation({
  path,
  onClose,
}: LocationExplanationProps) {
  const [explanation, setExplanation] = useState<Explanation | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;

    api
      .explainLocation(path)
      .then((result) => {
        if (!cancelled) setExplanation(result);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });

    return () => {
      cancelled = true;
    };
  }, [path]);

  const entry = explanation?.entry ?? null;

  return (
    <ContentDialog open title="Why is this here?" cancelText="Close" onCancel={onClose}>
      <div className="stack">
        <p className="path-text" style={{ margin: 0 }}>
          {explanation?.path ?? path}
        </p>

        {failed ? (
          <EmptyState title="The explanation could not be loaded." />
        ) : explanation === null ? (
          <EmptyState title="Looking this up…" />
        ) : entry === null ? (
          <>
            <EmptyState
              title="No information available"
              detail="Stora has no curated entry for this location. That means nothing is known about it here — not that it is safe or unsafe to remove."
            />
            <p className="text-secondary" style={{ margin: 0 }}>
              Stora will not guess. Explanations are hand-written and cited; if a
              location is not covered, the honest answer is that it is not covered.
            </p>
          </>
        ) : (
          <>
            {!entry.removable ? (
              <InfoBar
                tone="warning"
                title="Not safe to remove"
                message="Stora will not modify this location, and neither should anything else."
              />
            ) : null}

            <InfoRow label="Location" value={entry.title} />
            <InfoRow label="What writes here" value={entry.writtenBy} />
            <InfoRow label="If it is removed" value={entry.ifRemoved} />
            <InfoRow
              label="Removable"
              value={entry.removable ? "Yes, with review" : "No"}
            />

            <div
              style={{
                paddingTop: 8,
                borderTop: "1px solid var(--memora-stroke-surface)",
              }}
            >
              <div className="text-secondary">Source</div>
              <a
                href={entry.sourceUrl}
                target="_blank"
                rel="noreferrer"
                style={{ fontSize: 12, color: "var(--memora-accent-usable)" }}
              >
                {entry.sourceTitle}
              </a>
            </div>
          </>
        )}
      </div>
    </ContentDialog>
  );
}
