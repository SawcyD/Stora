# Stora

**Understand your storage.**

A transparent storage analysis and cleanup utility for Windows 11, built with
Tauri v2, Rust, React, and SQLite.

Stora is not a "PC cleaner". It has no health score, no scare messaging, and no
inflated cleanup totals. Every deletion is previewed, every figure is derived
from something observed on disk, and all analysis stays on the device.

## Current scope

All six phases are implemented. See [docs/ROADMAP.md](docs/ROADMAP.md) for the
detail and the remaining limitations.

| Phase | Area | Status |
| --- | --- | --- |
| 1 | Native shell, navigation, tray, settings, SQLite migrations | Complete |
| 2 | Drive discovery, scanner, folder tree, large files | Complete |
| 3 | Safe cleanup, preview, Recycle Bin, results, history | Complete |
| 4 | Developer storage, package caches, virtual disks | Complete |
| 5 | Application discovery, footprints, activity tracking | Complete |
| 6 | Duplicates, growth, alerts, automation, quarantine | Complete |

## Repository layout

```text
stora/
├── apps/desktop/          React frontend and the Tauri shell
│   ├── src/               Pages, components, typed IPC layer
│   └── src-tauri/         Commands, app state, tray
├── crates/
│   ├── stora-core/        Shared models, typed errors, task coordination
│   ├── stora-security/    Path validation, protected paths, authorization
│   ├── stora-winapi/      Volumes, Recycle Bin, file locks, accent color
│   ├── stora-index/       SQLite persistence and queries
│   ├── stora-scanner/     Directory traversal and aggregation
│   ├── stora-cleaner/     Cleanup categories, plans, execution
│   ├── stora-developer/   Project detection and artifact classification
│   ├── stora-apps/        Installed apps, footprints, unused filtering
│   ├── stora-activity/    Local launch observation
│   ├── stora-duplicates/  Staged duplicate detection
│   ├── stora-rules/       Automation, growth history, alerts
│   └── stora-knowledge/   Curated, offline location explanations
└── migrations/            SQL schema migrations, applied in order
```

## Development

```bash
# Frontend
cd apps/desktop
npm install
npm run typecheck
npm run build

# Backend
cargo test --workspace
cargo clippy --workspace --all-targets

# Run the app
cd apps/desktop && npm run tauri dev
```

Requires the MSVC toolchain, WebView2, and the Tauri v2 prerequisites.

## How deletion is kept safe

The frontend is treated as untrusted input.

1. The backend enumerates cleanup categories and produces a **plan** — the only
   source of deletable paths. Plans expire after 15 minutes.
2. The frontend approves **indices into that plan**, never paths.
3. `stora-security` re-normalizes each resolved path, rejects traversal, and
   re-checks it against the protected-location rules, so a bug in plan
   generation cannot escalate into deleting a system file.
4. Each item is **revalidated immediately before removal**. A file whose size or
   type changed since the preview, or that became a link, is skipped.
5. Only files that were actually removed count toward "space recovered".

There is deliberately no `delete_any_path`, `run_any_command`, or
`read_any_file` command.

## Storage footprint

Stora only stores an individual database row for files at or above 1 MB.
Everything the interface shows below that comes from folder aggregates and
per-category totals accumulated during the scan.

Measured on `C:\Program Files` (136,050 files, 42.7 GB): 4,154 rows and an
8.7 MB database — about 67 bytes per file seen. To re-check after a schema
change:

```bash
cargo run -p stora-index --example scan_size -- "C:\Program Files"
```

## Why is this here?

Right-click any folder on the Storage page for a curated explanation of what
writes there and what happens if it is removed, each one citing primary
documentation.

The entries live in
[`crates/stora-knowledge/data/locations.json`](crates/stora-knowledge/data/locations.json),
are embedded in the binary, and are mirrored into SQLite at startup. There is
deliberately no language model, no web request, and no confidence score: a
location either has a hand-written entry or it does not, and "No information
available" is a better answer than a fabricated percentage.

## Design system

The interface uses [`@sawcy/memora-ui`](https://www.npmjs.com/package/@sawcy/memora-ui)
as an installed dependency. Its stylesheet is imported exactly once, in
`src/main.tsx`, and none of it is copied into this repository.

Memora's surfaces are translucent by design, so Stora paints the opaque window
base itself using the Windows 11 Mica base tones, which flip with the active
theme.
