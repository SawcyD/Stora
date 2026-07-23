# Stora — status

A running record of what is built and what is deliberately absent. Nothing
below is aspirational: if a feature is listed as shipped, it exists in the
product and has tests behind it.

Last updated: 2026-07-22 · Version 0.1.0

---

## Summary

| Phase | Area | Status |
| --- | --- | --- |
| 1 | Native shell, navigation, tray, settings, migrations | **Shipped** |
| 2 | Drive discovery, scanner, folder tree, large files | **Shipped** |
| 3 | Safe cleanup, preview, Recycle Bin, results, history | **Shipped** |
| 4 | Developer storage, package caches, virtual disks | **Shipped** |
| 5 | Applications, footprints, activity tracking | **Shipped** |
| 6 | Duplicates, growth, alerts, automation, quarantine | **Shipped** |
| 7 | Treemap, shell activity, uninstall leftovers, knowledge base | **Shipped** |

356 Rust tests and 14 frontend tests pass; `cargo clippy --all-targets` is
clean; the frontend typechecks and builds with no warnings.

---

## Phase 1 — Application shell

- Eleven-page `NavigationView` with icons, labels, and a Windows-style leading
  selection indicator; collapses to icons below 820 px.
- Light, dark, and high-contrast support, following the Windows theme by
  default with an explicit override in Settings.
- Real Windows accent color read from DWM and handed to the component library.
- System tray with menu and free-space tooltip; close-to-tray keeps background
  monitoring alive.
- Window size, position, page, sidebar state, and drive restored between
  sessions.
- Keyboard-first: skip link, logical tab order, visible focus rings, accessible
  names on every control, reduced-motion respected.

## Phase 2 — Storage analysis

- Local fixed and removable volumes; optical and network drives are skipped
  deliberately.
- Scanner with pause, resume, and cancel, all responding in under a second.
- Reparse points are **not** followed by default; when the user opts in, loop
  detection prevents a junction pointing at its own ancestor from hanging the
  walk.
- Paths beyond `MAX_PATH`, Unicode names, and unreadable folders all handled —
  an access denial is recorded and the scan continues.
- Folder totals roll up in a single pass, so the tree is exact.
- Tree loads one level at a time; a multi-million-file drive never renders at
  once.
- Large-file grid with size and type filters. Deletion is deliberately absent
  here.

## Phase 3 — Cleanup

Twelve categories across three tiers — safe (preselected), review required
(never preselected), and advanced (hidden unless enabled, and deferring to the
supported DISM command rather than deleting servicing files).

Full preview with per-file toggles, three removal methods, live progress, and a
result listing exactly what was skipped and why. Every run is logged to
History with a copyable report.

### The safety model

The frontend is treated as untrusted input:

1. The backend produces a **plan** — the only source of deletable paths. Plans
   expire after 15 minutes.
2. The frontend approves **indices into that plan**, never paths.
3. Each resolved path is re-normalized, traversal is rejected, and protected
   locations are re-checked — so a bug in plan generation cannot escalate into
   deleting a system file.
4. Every item is **revalidated immediately before removal**. A file that
   changed size or type, or became a link, is skipped.
5. Only files actually removed count toward "space recovered".

There is no `delete_any_path`, `run_any_command`, or `read_any_file` command.

## Phase 4 — Developer storage

- Projects are recognized **only** by a marker file the toolchain itself
  creates: `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`,
  `build.gradle`, `*.sln`, `*.uproject`, `default.project.json`, or a Unity
  `ProjectSettings` directory.
- **A folder named `build`, `dist`, or `target` is never an artifact on its
  own.** Classification requires a project kind that actually produces it, so
  `target` inside a Node project is ignored.
- Every artifact carries a safety label: regeneratable, usually regeneratable,
  build output, dependency cache, project source, or user-created data. The
  last two are never removable — Unreal's `Saved` folder is shown with its size
  but can never be selected.
- Machine-wide package caches (npm, pnpm, Cargo, NuGet, pip, Gradle) with the
  official cleanup command shown rather than run.
- Virtual disks for WSL, Docker, Hyper-V, VMware, and VirtualBox. **Stora never
  deletes one** — each entry explains the supported way to compact it, and why
  a WSL disk stays large after files inside it are deleted.
- Monorepos are walked for nested projects, but `node_modules` is never
  searched: the thousands of `package.json` files inside are not the user's
  projects.

## Phase 5 — Applications

- Discovery from the 64-bit, 32-bit, and per-user uninstall registry views,
  de-duplicated. Entries Windows marks as system components, updates, or
  patches are hidden.
- **Runtimes, redistributables, and drivers are never suggested for removal.**
  Nothing launches them directly so they always look idle, yet other software
  depends on them. Vendor and runtime name fragments classify them out.
- **Footprint inspector** where each folder→application link carries a
  confidence level and a stated reason. Short or common names never produce a
  partial match — a folder called `Code` is not attributed to anything.
- **Activity with honest labelling.** Windows has no single reliable
  last-opened value, so Stora distinguishes *Last observed by Stora* (High),
  *Windows activity estimate* (Medium), *File activity only* (Low), and *No
  reliable activity data* (Unknown). No source is ever labelled plain "Last
  opened", and last-access time is never treated as proof.
- Launch observation is off until turned on, stays on the device, and can be
  cleared. It diffs periodic process snapshots, so a program that starts and
  exits between samples is missed — an honest under-count.
- Removal always launches the application's own registered uninstaller.

## Phase 6 — Advanced analysis

- **Duplicates** — staged: group by size, sample 8 KB from each end, then fully
  verify survivors with SHA-256. Hard links are detected via volume serial and
  file index, are never selected for removal, and do not count toward
  reclaimable space. Nothing is ever preselected.
- **Growth history** — snapshot differences over 24 hours, 7/30/90 days, and
  since install. A folder with no old enough baseline reports "not enough
  history" rather than claiming it grew by its entire size.
- **Alerts** — low space and sharp folder growth, worded informatively. A test
  asserts no scare vocabulary.
- **Automation** — rules are disabled when created. A rule may notify about
  anything, but may only *remove* categories on a fixed safe list of
  regeneratable caches; Downloads, the Recycle Bin, and old installers are
  rejected both at creation and at evaluation. Rules stop after three
  consecutive errors, and re-enabling clears the counter.
- **Quarantine** — files are recorded when moved, can be restored to their
  original location (refusing to overwrite anything now there), or purged
  explicitly. Credential and key paths are never quarantined.

---

## Phase 7 — Visualization, history, and leftovers

### Treemap

Rectangles sized by space on disk, sitting above the folder list on the Storage
page and toggleable off.

- Layout maths come from `d3-hierarchy` alone (~197 KB installed, no DOM, no
  charting library). Rectangles are drawn here so they match the rest of the
  interface.
- SVG below 2,000 rectangles, Canvas above — beyond that, SVG nodes cost more
  than they are worth.
- Siblings under 1% of the parent fold into a single "Other" block. A folder
  can hold thousands of children whose rectangles would be a pixel wide;
  collapsing them keeps the total honest while the list carries the detail.
- Clicking a block drills in and moves the breadcrumb. The "Other" block is not
  a real folder and cannot be opened.
- Arrow keys move between blocks, Enter opens, and the focused block is drawn
  with a dashed outline as well as a colour change so it survives high
  contrast. The list beside it remains the accessible reference.

### Windows activity estimates

UserAssist (`HKCU\...\Explorer\UserAssist\{GUID}\Count`) is read, ROT13-decoded,
and parsed for run count and last-executed `FILETIME`. Both the modern 72-byte
and legacy 16-byte value layouts are handled.

It is reported as *Windows activity estimate* at medium confidence and **never**
as a launch Stora observed. A launch Stora watched always wins; the estimate
only fills gaps.

The limitation is stated in Settings and on the Applications page: UserAssist
only sees programs launched through Explorer, so anything started from a
terminal or a game launcher is invisible to it. **Absence is unknown, never
unused.**

### Uninstall leftovers

1. **Preflight** measures the application's footprint and resolves the method
   that will actually be used, so the dialog states it rather than guessing.
2. **Restore point** is attempted and the result reported verbatim — created,
   System Restore disabled, needs elevation, or rate-limited. Stora never
   implies a restore point exists when it does not, and a failure does not
   block the uninstall.
3. **Execution** runs the registered `UninstallString`, falls back to
   `winget uninstall --id`, and otherwise points at Windows Settings. There is
   no path that deletes a program directory.
4. **Leftovers** re-measures the recorded folders once the uninstaller closes.
   Survivors are offered through the ordinary plan-and-revalidate pipeline,
   with nothing preselected — leftovers often include settings and documents.
5. **Registry keys** are reported and never removed. They occupy kilobytes and
   removing them risks breaking other software for no measurable gain.

### Local knowledge base

Right-click a folder for a curated explanation of what writes there and what
happens if it is removed, citing primary documentation. Seventeen entries cover
the locations people most often ask about, including the ones that are
routinely got wrong — `WinSxS` (whose size is inflated by hard links),
`C:\Windows\Installer` (whose removal silently breaks MSI uninstall), and WSL
virtual disks.

There is no language model, no web request, and no confidence score. A
generated number would imply a calibration that does not exist. Unknown
locations say "No information available", and the dialog states that this means
nothing is known — not that removal is safe.

## Known limitations

- **Elevation is not implemented.** Anything requiring administrator rights is
  surfaced as guidance rather than performed. This includes DISM component
  store cleanup and `Optimize-VHD`.
- **Automation does not execute on a schedule.** Rules can be created,
  enabled, evaluated on demand via "Check rules now", and their history is
  recorded — but there is no background scheduler firing them unattended yet.
- **Prefetch is deliberately not parsed.** `C:\Windows\Prefetch` is ACL'd and
  needs administrator rights, which conflicts with not running elevated. It is
  also capped at 1024 entries, so a program unused for a long time may simply
  have been evicted — absence there would be especially misleading. UserAssist
  covers the same question without elevation.
- **Growth snapshots are recorded manually** via the button on the Automation
  page, not automatically after each scan.
- **File-lock detection has no dedicated UI.** The Restart Manager integration
  identifies which processes hold a file and locked files are reported as
  skipped, but there is no Skip/Retry/Schedule-after-restart prompt.
- **Mica is not wired up.** The window paints opaque Mica base tones instead.
- **Application footprint sizes are not persisted** — they are measured on
  demand when the inspector is opened, and the pre-uninstall snapshot lives in
  memory only, so leftovers cannot be checked after a restart.
- **Leftover detection only covers folders Stora had already attributed** to the
  application. Anything the uninstaller left in a location the footprint
  inspector could not link is not found.
- **The knowledge base covers seventeen locations.** It is a curated set, not
  exhaustive; most paths will correctly return "No information available".
- **The treemap has not been seen rendering with real data.** Its layout logic
  has 14 unit tests and its styles are confirmed live in the browser, but the
  populated view was not visually verified, for the same reason as the native
  window below.
- The native window has not been visually inspected; verification was done
  against the identical bundle in the same Chromium engine via the dev server.
- DISM guidance, Delivery Optimization cleanup, and the uninstaller launch are
  implemented and unit-tested but have not been exercised end to end on a live
  machine.
