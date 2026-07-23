/**
 * Small application-specific components.
 *
 * Each one is built from Memora UI primitives and design tokens rather than
 * introducing new visual language.
 */

import type { ReactNode } from "react";

import { formatBytes, formatPercent, riskLabel } from "../lib/format";

interface PageHeaderProps {
  title: string;
  description?: string;
  actions?: ReactNode;
}

/** A restrained page title with an optional trailing action area. */
export function PageHeader({ title, description, actions }: PageHeaderProps) {
  return (
    <header className="page-header">
      <div className="page-header__titles">
        <h1 className="page-header__title">{title}</h1>
        {description ? <p className="page-header__description">{description}</p> : null}
      </div>
      {actions ? <div className="row">{actions}</div> : null}
    </header>
  );
}

interface CapacityBarProps {
  usedBytes: number;
  totalBytes: number;
  label: string;
}

/**
 * A horizontal Windows-style capacity bar.
 *
 * Deliberately not a circular gauge: this is the shape File Explorer and
 * Storage settings use, and it compares well across several drives.
 */
export function CapacityBar({ usedBytes, totalBytes, label }: CapacityBarProps) {
  const percent = totalBytes > 0 ? (usedBytes / totalBytes) * 100 : 0;
  const freeBytes = Math.max(totalBytes - usedBytes, 0);
  // Windows itself warns below roughly 10% free; mirror that threshold.
  const isLow = totalBytes > 0 && freeBytes / totalBytes < 0.1;

  return (
    <div className="capacity">
      <div
        className="capacity__track"
        role="progressbar"
        aria-valuenow={Math.round(percent)}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-label={`${label}: ${formatPercent(percent)} in use`}
      >
        <div
          className={`capacity__fill${isLow ? " capacity__fill--low" : ""}`}
          style={{ width: `${Math.min(percent, 100)}%` }}
        />
      </div>
      <div className="capacity__legend">
        <span>
          <strong>{formatBytes(usedBytes)}</strong> used
        </span>
        <span>
          <strong>{formatBytes(freeBytes)}</strong> available of{" "}
          {formatBytes(totalBytes)}
        </span>
      </div>
    </div>
  );
}

interface EmptyStateProps {
  title: string;
  detail?: string;
  action?: ReactNode;
}

export function EmptyState({ title, detail, action }: EmptyStateProps) {
  return (
    <div className="empty-state">
      <span className="empty-state__title">{title}</span>
      {detail ? <span className="empty-state__detail">{detail}</span> : null}
      {action}
    </div>
  );
}

/**
 * Risk indicator for a cleanup category.
 *
 * The level is always spelled out in text — color is never the only signal,
 * which also keeps it readable in high-contrast mode.
 */
export function RiskBadge({ risk }: { risk: string }) {
  const modifier =
    risk === "userReviewRequired"
      ? "review"
      : risk === "advanced"
        ? "advanced"
        : risk === "moderate"
          ? "moderate"
          : "low";

  return <span className={`badge badge--${modifier}`}>Risk: {riskLabel(risk)}</span>;
}

interface BreakdownRowProps {
  label: string;
  bytes: number;
  maxBytes: number;
  onSelect?: () => void;
  trailing?: ReactNode;
}

/** One row of the storage breakdown, with an inline proportion meter. */
export function BreakdownRow({
  label,
  bytes,
  maxBytes,
  onSelect,
  trailing,
}: BreakdownRowProps) {
  const share = maxBytes > 0 ? (bytes / maxBytes) * 100 : 0;

  return (
    <button
      type="button"
      className="breakdown__row"
      onClick={onSelect}
      disabled={!onSelect}
    >
      <span className="breakdown__label">
        <span className="breakdown__name">{label}</span>
        <span className="breakdown__meter" aria-hidden="true">
          <span style={{ width: `${share}%` }} />
        </span>
      </span>
      <span className="breakdown__value">
        {trailing ?? formatBytes(bytes)}
      </span>
    </button>
  );
}
