# CrashLens architecture

CrashLens uses explicit domain, service, application, and UI layers. Dependencies point inward: services and UI consume the domain model, while the application layer coordinates them.

- `domain/report.rs`: UI- and infrastructure-independent crash report model.
- `services/analyzer.rs`: minidump parsing, stack walking, and symbol resolution.
- `services/scanner.rs`: recursive filesystem discovery and typed scan events.
- `services/investigation.rs`: signatures, likely causes, source mappings, and search.
- `services/export.rs`: artifact rendering and filesystem writes with typed results.
- `app/state.rs`: background analysis jobs, library scan state, and symbol settings.
- `app/screens/`: independently maintained high-volume application screens.
- `ui/view_model.rs`: presentation-ready relationships derived from domain reports.
- `ui/widgets.rs`: reusable egui presentation primitives.
- `app/mod.rs`: composition root and navigation controller.

## Design rules

The domain layer must not depend on egui, minidump, or filesystem services. Views may request actions, but filesystem access, parsing, and long-running work belong in services. Background workers communicate through typed channels and never mutate UI state directly. Large collections must use virtualized rendering. Screen-specific rendering belongs under `app/screens`, not in reusable UI modules.

## Testing strategy

Pure parsing, investigation, view-model, configuration, and rendering behavior is unit tested. Workflow tests exercise channels, scan state transitions, real temporary-file exports, and failure propagation. `cargo clippy --all-targets --all-features -- -D warnings` is the minimum static-quality gate.
