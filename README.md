# CrashLens

CrashLens is a native Rust desktop application for opening and exploring Windows and Breakpad minidump (`.dmp`/`.mdmp`) files.

## Run

```sh
cargo run --release
```

Open a dump from the welcome screen, use **File → Open dump**, or drag a dump onto the window.

## Current analysis

- dump validity, format and stream inventory
- crash exception and faulting thread
- thread and module lists
- memory-region map
- filterable raw stream directory
- malformed or missing stream diagnostics

Minidumps are parsed locally; no crash data leaves the machine. Full stack unwinding and symbol-server integration are the next layer planned for the analysis service.

