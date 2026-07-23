# Next Features Development Plan

*Proposed: 2026-07-22*

## Product principles

Stora remains a transparent, local-first storage utility rather than an
aggressive "PC cleaner".

- AI may explain evidence and make recommendations; deterministic Rust policy
  decides whether an operation can be offered.
- Core Windows, boot, recovery, credential, and protected locations remain
  non-removable through Stora. Confirmation dialogs never override a policy
  block.
- Unattended cleanup is limited to an allow-list of regeneratable cache
  categories. It never includes Downloads, Recycle Bin contents, personal
  files, installers, app data, or developer source.
- All destructive work uses the existing plan, expiry, revalidation, and
  quarantine/Recycle Bin pipeline. The frontend and tray are never allowed to
  send arbitrary paths to delete.
- Activity, deletion history, and AI processing are opt-in, local by default,
  and labelled with their evidence and limitations.

## Milestone 1: Reliable background maintenance

### Background scheduler

Turn the existing on-demand automation rules into unattended rules while
Stora is running in the tray.

- Evaluate weekly, low-free-space, and folder-growth rules on a bounded timer.
- Record growth snapshots automatically after a successful scan and once per
  day at most; deduplicate snapshots by path and timestamp.
- Do not run an automatic cleanup while a scan, manual cleanup, or uninstall
  is active.
- Pause/defer automatic cleanup on battery power, during quiet hours, or when
  the machine is active; configurable settings default to conservative values.
- Log every check and action, including a no-op reason and recovered bytes.
- Notify only on state changes or successful cleanup, avoiding repeat alerts.

### Scheduled safe cleanup presets

Add simple preset rules before exposing detailed rule construction:

- Weekly temporary-file cleanup (default: disabled).
- Clear shader and thumbnail caches when C: is below a chosen free-space
  target (default: disabled).
- Notify, never delete, when a watched folder grows sharply.

Preset cleanup uses only the existing allow-list: user/system temporary files,
thumbnail and shader caches, crash dumps, error reports, and Delivery
Optimization files. It honours a minimum file age and uses the user-selected
reversible removal method where supported.

**Acceptance:** a rule fires no more than once for its eligible period,
survives app restart, stops after repeated errors, and cannot clean a category
outside the backend allow-list.

## Milestone 2: Tray quick actions and everyday quality of life

Stora already has a tray icon, tooltip, menu navigation, and double-click to
open. Extend it with one configurable primary action and a compact status
section.

### Configurable double-click action

Settings offers one of:

1. Open Stora (default).
2. Start a storage scan.
3. Open Cleanup review.
4. Run the named, enabled safe-cleanup preset.
5. Show the Drive Pressure panel.

The quick-clean option runs only a previously enabled rule whose categories
are policy-approved. It does not bypass the existing automation safeguards.
The menu also exposes Scan now, Review cleanup, the latest C: free-space
summary, and the last successful cleanup result.

### Quality-of-life work

- Add a global in-app command palette and keyboard shortcuts for scan, cleanup
  review, large files, and Drive Pressure.
- Remember filters, sort order, and last expanded folder per drive.
- Add a concise post-cleanup summary with reclaimed, skipped, quarantined, and
  Recycle Bin figures.
- Add retry-after-restart guidance for locked files; no forced process killing.
- Surface scan freshness ("last scanned 3 days ago") and offer a refresh only
  when data is stale.

**Acceptance:** tray actions remain responsive with the window hidden, respect
the selected action after restart, and never execute arbitrary deletion paths.

## Milestone 3: Storage timeline and app recommendations

### Storage timeline

Create a simple, searchable timeline retaining 30 days by default (user
configurable):

- Stora cleanup runs and their item-level results.
- Quarantine moves, restores, expirations, and explicit purges.
- Recycle Bin inventory when available, clearly separated from Stora history.
- Scan snapshots and meaningful folder-growth events.
- Installation/uninstallation observations when Windows provides them.

It must not claim to reconstruct files deleted outside Stora. Windows cannot
reliably provide a complete historical deletion log, so the UI says exactly
what source each record came from.

### Application review

Use the existing opt-in launch observation and Windows estimate to create a
review queue, never an automatic uninstall list:

> Installed in 2023 · 18.4 GB · no launch observed by Stora in 90 days.

Each recommendation shows its evidence source and confidence. Users can mark
an app "keep", snooze it, or start its registered uninstaller after a
footprint review. Missing activity data always means unknown, not unused.

**Acceptance:** the user can clear this local history; activity does not start
without consent; uninstall recommendations never include runtimes, drivers,
or system components.

## Milestone 4: Drive Pressure

Build a C:-focused, actionable view when its free-space target is missed.

- Compare pressure on each local drive and explain the most impactful,
  user-actionable categories on the pressured drive.
- Classify opportunities as: move personal files, clean safe caches, use an
  application's supported move flow, review an unused application, or
  informational only.
- Recognize known game-library and development/virtual-disk storage and direct
  users to the owning tool's supported relocation process.
- Offer move planning for user-owned folders (Pictures, Videos, Downloads,
  project folders) with destination capacity checks, collision checks,
  progress, cancellation, and a move log.
- Do not silently move AppData, Windows folders, program folders, virtual
  disks, or game files. Unsupported app-data relocation is guidance only.

Directory junctions are a separate advanced feature: only offer them for a
validated, allow-listed app migration with a backup/rollback record and a
clear compatibility warning. They are not part of the initial Drive Pressure
release.

**Acceptance:** recommendations account for destination capacity and preserve
file integrity; system/app-data paths cannot be selected for a direct move.

## Milestone 5: Evidence-backed "Can I delete this?" advisor

### UX

Add a right-click **Ask Stora** action for a folder, file, cleanup category,
or app footprint. It returns one of four conclusions:

- **Safe to remove** — only where policy and evidence support it.
- **Review first** — removal may affect user or application data.
- **Do not remove** — policy-protected or known to damage Windows/app repair.
- **Unknown** — Stora lacks sufficient evidence; no deletion call-to-action.

Every answer includes observed path/type/size, owner or classification when
known, consequences, safer alternatives, and linked primary sources.

### Architecture

1. Rust collects a narrow evidence packet from indexed metadata and the
   curated knowledge base; it does not send arbitrary file contents.
2. Protected-path and safety classification run first and cannot be overridden
   by the advisor response.
3. A source resolver selects official Windows/vendor/tool documentation. Cache
   citations locally with title, URL, retrieval date, and applicability.
4. An optional AI provider turns the structured evidence into plain language.
   No provider receives paths or metadata until the user explicitly enables
   cloud advice; a local-only explanation remains available for known entries.
   The initial cloud model is the pinned `gpt-4.1-mini-2025-04-14`: it provides
   structured outputs and tool calling at an appropriate cost for short,
   evidence-constrained explanations. It is not the source of safety policy.
5. The result never carries deletion authority. Any later deletion begins a
   new normal cleanup plan and its existing revalidation checks.

### Protected locations

Core Windows, boot/recovery, credential/key, and current-running-app paths
are hard blocks. They show explanation and supported cleanup guidance only.
Do not use repeated confirmation dialogs to override these blocks. For
dangerous but allowed user-owned data, use a typed confirmation plus final
plan review; five generic "are you sure" screens create fatigue without
meaningfully improving safety.

**Acceptance:** adversarial AI output cannot make a protected or arbitrary
path removable; every recommendation has visible evidence/source status; an
unknown result is conservative.

## Delivery order and test gates

1. Scheduler + automatic snapshots + safe cleanup presets.
2. Configurable tray action and cleanup/status quality-of-life work.
3. Timeline, activity review, and Drive Pressure recommendations.
4. Guided user-folder moves; validate supported migration guidance.
5. Advisor with local evidence first, then optional cloud AI.

Before each release: Rust policy unit tests, migration tests, scheduler tests
with a controllable clock, tray integration tests, crash/restart recovery
tests, accessibility checks, and live verification on a non-production Windows
profile. Test every destructive flow with protected paths, symlinks/junctions,
low disk space, locked files, cancellation, and rollback.
