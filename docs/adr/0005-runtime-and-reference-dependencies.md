# Separate library dependencies from reference-test dependencies

Implement core parsing, matching, captures, and replacement in Rust. Necessary pure Rust helper crates are allowed, but another regex engine must not replace the port. Calls to operating-system-provided APIs are allowed to preserve platform character rules and related behavior.

The library used by callers must not introduce additional C/C++ implementation libraries that require separate builds, and must not retain a Boost C++ bridge. Compatibility tests may compile the trimmed C++ implementation as an independent reference executable. That test dependency must not become a build or runtime dependency for callers using the Rust library.
