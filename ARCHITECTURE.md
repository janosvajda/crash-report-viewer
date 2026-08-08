# CrashLens architecture

CrashLens is a native Rust desktop application built with `eframe`/`egui`. It reads minidumps on background threads, converts them into a typed report, and presents that report through investigation-focused screens.

This document describes the code as it exists today. CrashLens is layered, but it is not a strict clean-architecture implementation: application screens may call synchronous services such as export and “open in Finder”, while parsing and filesystem scanning remain outside the UI.

## Source layout

```text
src/
├── main.rs                    process entry point and native window setup
├── domain/
│   └── report.rs              typed crash-report model
├── services/
│   ├── analyzer.rs            minidump parsing, stack walking and symbols
│   ├── scanner.rs             concurrent dump discovery
│   ├── investigation.rs       signatures, likely causes, search and source links
│   ├── memory.rs              memory classification and pointer/string analysis
│   ├── export.rs              report and investigation-bundle generation
│   └── platform.rs            operating-system integration
├── app/
│   ├── mod.rs                 application coordinator and navigation
│   ├── state.rs               jobs, dump library and symbol settings
│   ├── session.rs             current-analysis, investigation and comparison state
│   └── screens/               feature-specific egui screens and actions
│       └── memory/            memory overview, evidence, browser, map and cache
└── ui/
    ├── assets.rs              embedded logo and native icon decoding
    ├── theme.rs               fonts, colours and widget styling
    ├── view_model.rs          presentation data derived from reports
    └── widgets.rs             reusable egui controls
```

## Dependency direction

```text
main
  └── app
      ├── app/screens ──┬── ui
      │                 ├── services
      │                 └── domain
      ├── services ────────── domain
      └── domain

ui/view_model ─────────────── domain + investigation helpers
ui/assets, theme, widgets ─── egui/image
domain ────────────────────── standard library only
```

The domain model is the stable centre of the application. It does not know about egui, minidump parser types, threads, channels or the filesystem. Services translate external data into domain values. The application owns state and coordinates services. Screens render that state and return typed actions to the coordinator.

## Domain model

[`src/domain/report.rs`](src/domain/report.rs) contains the report exchanged between analysis, investigation and presentation code. Important concepts are represented by types rather than repeatedly interpreted strings or integers:

- `CpuArchitecture`, `OperatingSystem` and `PointerSize`
- `VirtualAddress`
- `ModuleOwnership` and `SymbolStatus`
- `MemoryPermissions`
- `DumpReport` and its thread, frame, module, memory and stream rows

Unknown metadata remains unknown. For example, memory analysis does not guess a pointer width when the dump architecture cannot determine one.

## Services

Services do the work that is independent of the desktop interface.

- `analyzer` opens a dump, walks stacks, resolves available symbols and produces a `DumpReport`. It is run through an `AnalysisJob`, not on the egui thread.
- `scanner` walks directories with a bounded worker pool and reports progress through typed `ScanEvent` messages. Permission failures encountered during a system scan do not abort the entire scan.
- `investigation` derives crash signatures, likely causes, search matches and source-location hints from a report.
- `memory` classifies captured regions, decodes architecture-sized pointer values, follows bounded pointer chains, extracts readable strings and relates addresses to stacks, regions or modules.
- `export` renders Markdown, JSON, stack text and investigation bundles, then writes them to the destination chosen by the user.
- `platform` contains operating-system-specific actions such as revealing an exported path.

The original dump is treated as read-only. Export code creates new files and never modifies it.

## Application state and screens

[`src/app/mod.rs`](src/app/mod.rs) owns `CrashLens`, the application coordinator. It handles navigation, polls background work, installs the theme, displays the startup splash and applies actions returned by screens.

State is grouped by lifetime instead of being stored as unrelated fields in every screen:

- `AnalysisViewState` holds transient selections, filters and the memory-analysis cache for the open report.
- `InvestigationState` holds notes, tags, status, symbol settings and the last export result.
- `ComparisonState` holds the second report and the pair selected from the dump library.
- `LibraryState` holds discovered files and scan progress.
- `AnalysisJob` owns a background analysis receiver.

Most screens communicate with the coordinator through `ScreenAction`; the memory and module features use their own smaller action types where the action is local to that feature. Screens do not retain duplicate copies of a `DumpReport`; reports shared between views use `Arc<DumpReport>`.

The memory feature is split by user task:

- `overview.rs` gives a compact starting point.
- `evidence.rs` shows crash-linked evidence.
- `analysis.rs` contains the memory-analysis helpers and detail presentation.
- `map.rs` visualises the composition of memory captured by the dump.
- `state.rs` caches derived analysis so scrolling does not repeatedly decode the same memory.
- `mod.rs` coordinates the memory feature and its browser.

## Main workflows

### Open and analyse a dump

1. The coordinator starts an `AnalysisJob` with the selected path and symbol configuration.
2. A worker calls the analyzer and sends back `Result<DumpReport, String>`.
3. The egui update loop polls the receiver without blocking.
4. On success, the report is shared through an `Arc`, view state is reset, and the Summary screen opens.
5. On failure, the UI keeps running and shows the error.

### Scan for dumps

1. The UI starts `scanner::scan` and keeps the receiving side of the channel.
2. Workers emit discovered files, progress and completion events.
3. `LibraryState` batches and sorts results for display.
4. Selecting two library entries feeds the same comparison workflow as manually chosen files.

### Investigate memory

Memory analysis starts with the typed architecture and captured regions in `DumpReport`. Derived results are stored in `MemoryAnalysisCache`. Addresses are resolved to known crash data through `AddressTarget`, so the UI can distinguish a thread stack, captured region, loaded module and unresolved address without parsing display text.

### Export investigation data

The report screen asks for a destination, calls the appropriate export function and stores the typed success or failure result in `InvestigationState`. Platform integration can then reveal the created file or directory.

## UI and performance rules

egui rendering stays on the main thread. Dump parsing, stack walking and whole-system scanning must not run there. Screens should derive expensive data once, cache it by the report or selection that produced it, and use virtualised rows for large collections. Hover and selection styling must not change layout dimensions.

`ui/view_model.rs` is the boundary for presentation-specific comparisons and relationships that are shared by multiple widgets. `ui/widgets.rs` is only for reusable controls; complete feature layouts belong under `app/screens`.

## Assets and startup

The official logo is stored under `assets/`. [`src/ui/assets.rs`](src/ui/assets.rs) embeds and decodes it for the window icon and the startup splash, so `cargo run` does not depend on the current working directory to find runtime image files.

## Errors and boundaries

Expected failures are returned as `Result` values and surfaced in the application state. Background workers send owned results through channels and never mutate egui state. A worker panic or parser error must not terminate the window process.

Some minidumps simply do not contain memory permissions, heap allocation metadata, source files or usable symbols. The domain and UI preserve that distinction instead of presenting inferred data as fact.

## Tests and quality checks

Tests live beside the code they exercise. They cover typed domain parsing, scan state, investigation logic, memory decoding, view models, application state, asset decoding and real export writes to temporary directories. Channel-based workflow tests cover success and failure propagation.

Before merging a change, run:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

Changes to screen behaviour should add tests to the underlying state, service or view-model logic rather than attempting to test pixel layout.
