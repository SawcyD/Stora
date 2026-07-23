import { Button, ContentDialog, InfoBar, InfoRow } from "@sawcy/memora-ui";
import { useEffect, useState } from "react";

import * as api from "../lib/api";
import type { AdvisorAnswer, AdvisorResearchAnswer } from "../lib/types";
import { EmptyState } from "./common";

export default function AdvisorDialog({ path, onClose }: { path: string; onClose: () => void }) {
  const [answer, setAnswer] = useState<AdvisorAnswer | null>(null);
  const [failed, setFailed] = useState(false);
  const [keySaved, setKeySaved] = useState(false);
  const [research, setResearch] = useState<AdvisorResearchAnswer | null>(null);
  const [researching, setResearching] = useState(false);
  const [researchError, setResearchError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api.advisePath(path).then(
      (result) => {
        if (!cancelled) setAnswer(result);
      },
      () => {
        if (!cancelled) setFailed(true);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [path]);

  useEffect(() => {
    void api.getAdvisorKeyStatus().then((status) => setKeySaved(status.saved)).catch(() => undefined);
  }, []);

  const researchWithAdvisor = async () => {
    setResearching(true);
    setResearchError(null);
    try {
      setResearch(await api.researchAdvisorPath(path));
    } catch (error) {
      setResearchError(api.describeError(error).title);
    } finally {
      setResearching(false);
    }
  };

  const title =
    answer?.verdict === "doNotRemove"
      ? "Do not remove"
      : answer?.verdict === "reviewFirst"
        ? "Review first"
        : "Unknown";

  return (
    <ContentDialog open title="Ask Stora" cancelText="Close" onCancel={onClose}>
      <div className="stack">
        <p className="path-text" style={{ margin: 0 }}>{path}</p>
        {failed ? (
          <EmptyState title="Stora could not prepare advice for this location." />
        ) : answer === null ? (
          <EmptyState title="Checking local evidence…" />
        ) : (
          <>
            <InfoBar
              tone={answer.verdict === "doNotRemove" ? "warning" : "info"}
              title={title}
              message={answer.summary}
            />
            {answer.reasons.map((reason) => (
              <InfoRow key={reason} label="Evidence" value={reason} />
            ))}
            <p className="text-secondary" style={{ margin: 0 }}>
              This verdict uses local safety policy and curated knowledge. It cannot
              authorize deletion.
            </p>
            {answer.sourceUrl && answer.sourceTitle ? (
              <a href={answer.sourceUrl} target="_blank" rel="noreferrer">
                Source: {answer.sourceTitle}
              </a>
            ) : null}
            {answer.verdict === "unknown" ? (
              <div className="stack" style={{ paddingTop: 8 }}>
                <InfoBar
                  tone="info"
                  title="Optional cloud research"
                  message="Research sends this path to OpenAI for a cited explanation. No file contents are sent, and the answer still cannot authorize deletion."
                />
                {keySaved ? (
                  <Button onClick={() => void researchWithAdvisor()} disabled={researching}>
                    {researching ? "Researching…" : "Research with Advisor"}
                  </Button>
                ) : (
                  <p className="text-secondary" style={{ margin: 0 }}>
                    Add an OpenAI API key in Settings → Stora Advisor to enable research.
                  </p>
                )}
                {researchError ? <InfoBar tone="error" title="Research failed" message={researchError} /> : null}
                {research ? (
                  <div className="stack">
                    <InfoBar
                      tone={research.verdict === "doNotRemove" ? "warning" : "info"}
                      title="Cloud research"
                      message={research.summary}
                    />
                    {research.reasons.map((reason) => (
                      <InfoRow key={reason} label="Research finding" value={reason} />
                    ))}
                    {research.sources.map((source) => (
                      <a key={source.url} href={source.url} target="_blank" rel="noreferrer">
                        Source: {source.title}
                      </a>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : null}
          </>
        )}
      </div>
    </ContentDialog>
  );
}
