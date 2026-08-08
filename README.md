# CrashLens

<p align="center">
  <img src="assets/crashlens-logo.png" alt="CrashLens logo" width="256">
</p>

CrashLens is a desktop tool for investigating `.dmp` and `.mdmp` crash dumps. It is written in Rust and uses Mozilla’s minidump libraries for parsing, stack unwinding, and symbol resolution.

The goal is simple: open a dump and get from “the application crashed” to the part of the process worth investigating, without working through raw `minidump-stackwalk` output by hand.

Everything is analysed locally. CrashLens does not upload dumps or captured memory. It only uses the network if you configure an HTTP symbol server.

## Running it

You need a current stable Rust toolchain. From the project directory, run:

```sh
cargo run
```

The first build takes longer because Cargo has to compile the dependencies. A release build is optional:

```sh
cargo run --release
```

## Opening a dump

You can choose a dump with the native file picker, enter a file or directory path, or drag a file onto the window.

CrashLens also has a system scan for finding `.dmp` and `.mdmp` files. The scanner works in parallel, skips directories it cannot read, and shows results as they are found. A full scan can still take a while on a large disk or mounted network volume.

The library allows two dumps to be selected and compared directly. There is no need to browse for them again on the comparison screen.

## Investigating a crash

The Overview is the starting point. It shows the recorded exception, fault address, crashed thread, likely failure location, and problems that could limit the analysis. From there you can move into the evidence that matters.

The Stack trace view shows the captured threads and recovered frames. Selecting a frame reveals its module, instruction address, source location, unwind confidence, and registers. If matching source is available locally, CrashLens can show the relevant source lines.

Code & modules answers two practical questions: which executable code was loaded, and which modules are connected to recovered stack frames? It separates operating-system code from application or third-party code, shows symbol status, and marks the module containing the fault address.

Memory references connects captured memory to the crash. It can identify the crashed thread’s stack, regions containing the fault address, register targets, pointers into known stacks or modules, pointer chains, and readable ASCII or UTF-16 text. The memory map shows the composition of the memory saved in the dump; it is not pretending to show the process’s complete address space.

Search evidence looks across threads, frames, functions, modules, source paths, addresses, memory regions, and dump streams.

Dump internals is the low-level view. It lists the streams physically present in the minidump and is mainly useful when a dump is incomplete, malformed, or contains producer-specific data.

## Comparing crashes

Select two files in the crash library and click **Compare selected**. CrashLens keeps file A and file B clearly separated and compares:

- the exception and fault address;
- the recovered crashing stacks and crash signatures;
- thread and module counts;
- modules that changed path or version;
- application or runtime modules found in only one dump;
- operating-system module differences, shown separately because they are often background noise.

Comparison is only as reliable as the evidence in both dumps. If one file has no exception stream or no usable crashing stack, CrashLens says that the result is incomplete instead of treating missing data as a meaningful difference.

## Symbols and local source

Raw dumps often contain addresses without useful function names. To get a good stack trace, CrashLens needs symbol files from the exact build that produced the crashing module.

The Debug symbols screen accepts local directories containing Breakpad `.sym` files and optional HTTP symbol-server URLs. Downloaded symbols are cached locally. If symbols refer to paths from a build machine, a recorded source root can be mapped to a local checkout.

Applying new symbol settings re-runs the analysis. It does not modify the dump.

## Notes and exports

The Notes & export screen is for saving or sharing an investigation. Status, tags, and notes are annotations; they do not change the analysis or the original dump.

CrashLens can create:

- a readable Markdown report;
- a privacy-reduced Markdown report with parent paths removed;
- a plain-text stack trace;
- the raw processor JSON;
- an investigation folder containing a report, stack trace, JSON, and a short README.

Each export asks where it should be saved.

## A few important limitations

A minidump only contains what the crashing application chose to save. CrashLens cannot recover missing threads, registers, memory, or streams.

The memory view is not a heap profiler. Ordinary minidumps usually do not contain enough allocator or runtime metadata to identify every object and its owner. Pointers and readable strings are useful clues, but they are not proof of the crash cause.

Good function names and source locations require matching symbols. Using symbols from a different build can be worse than having no symbols because the resulting stack may look convincing while being wrong.

Crash dumps and exported reports may contain private data, including paths, usernames, document text, URLs, tokens, and fragments of process memory. Review anything you export before sharing it. The privacy-reduced report removes parent paths, but it cannot guarantee that all captured content is safe.

## Development

Run the complete local check with:

```sh
cargo fmt --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build
```

The project is split into a typed domain model, parsing and investigation services, application state, view models, and egui screens. [ARCHITECTURE.md](ARCHITECTURE.md) explains the boundaries in more detail.

## Main dependencies

- `eframe` and `egui` for the desktop interface
- `minidump`, `minidump-processor`, and `minidump-unwind` for crash analysis
- `rfd` for native file and directory dialogs
- `tokio` for the symbol-processing runtime

## License

The crate is licensed under MIT or Apache-2.0.
