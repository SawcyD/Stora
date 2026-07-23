/** Formatting helpers shared across pages. */

const UNITS = ["bytes", "KB", "MB", "GB", "TB", "PB"] as const;

/** Formats a byte count the way Windows does: binary units, one decimal. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} bytes`;

  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(1)} ${UNITS[unit]}`;
}

export function formatCount(value: number): string {
  return value.toLocaleString();
}

export function formatPercent(value: number): string {
  return `${Math.round(value)}%`;
}

/** `48 seconds`, `2 minutes 5 seconds` — never a bare millisecond count. */
export function formatDuration(milliseconds: number): string {
  if (milliseconds < 1000) return "under a second";

  const totalSeconds = Math.round(milliseconds / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;

  if (minutes === 0) return `${seconds} second${seconds === 1 ? "" : "s"}`;
  if (seconds === 0) return `${minutes} minute${minutes === 1 ? "" : "s"}`;
  return `${minutes} min ${seconds} sec`;
}

/** `00:31` — the elapsed display used during scans and cleanups. */
export function formatElapsed(milliseconds: number): string {
  const totalSeconds = Math.floor(milliseconds / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

/** `Today, 4:32 PM` for recent times, an explicit date otherwise. */
export function formatTimestamp(unixSeconds: number | null | undefined): string {
  if (!unixSeconds) return "Never";

  const date = new Date(unixSeconds * 1000);
  const now = new Date();

  const time = date.toLocaleTimeString(undefined, {
    hour: "numeric",
    minute: "2-digit",
  });

  const sameDay = date.toDateString() === now.toDateString();
  if (sameDay) return `Today, ${time}`;

  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (date.toDateString() === yesterday.toDateString()) {
    return `Yesterday, ${time}`;
  }

  return `${date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
  })}, ${time}`;
}

/** Shortens a long path for display while keeping both ends readable. */
export function shortenPath(path: string, maxLength = 64): string {
  if (path.length <= maxLength) return path;

  const parts = path.split("\\");
  if (parts.length <= 3) return path;

  const head = parts.slice(0, 2).join("\\");
  const tail = parts.slice(-2).join("\\");
  return `${head}\\...\\${tail}`;
}

const CATEGORY_LABELS: Record<string, string> = {
  applications: "Applications",
  system: "System",
  development: "Development",
  documents: "Documents",
  games: "Games",
  temporaryFiles: "Temporary files",
  other: "Other",
};

export function categoryLabel(id: string): string {
  return CATEGORY_LABELS[id] ?? id;
}

const RISK_LABELS: Record<string, string> = {
  low: "Low",
  moderate: "Moderate",
  advanced: "Advanced",
  userReviewRequired: "User review required",
};

export function riskLabel(risk: string): string {
  return RISK_LABELS[risk] ?? risk;
}

const METHOD_LABELS: Record<string, string> = {
  recycleBin: "Move to Recycle Bin",
  permanent: "Delete permanently",
  quarantine: "Move to Stora quarantine",
  windowsCleanup: "Use Windows cleanup",
  applicationCleanup: "Use application cleanup",
};

export function methodLabel(method: string): string {
  return METHOD_LABELS[method] ?? method;
}
