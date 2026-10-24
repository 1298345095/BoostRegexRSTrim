# Port the trimmed Boost.Regex engine to Rust

The user evaluated existing Rust regex crates and found that their compatibility gaps and patch maintenance did not meet the requirements. Port the required engine logic from the current trimmed Boost.Regex implementation instead of substituting another regex engine.

The project will maintain the port and its compatibility checks. Follow [ADR-0001](0001-rust-migration-scope.md) for scope and the compatibility reference, [ADR-0003](0003-api-and-resource-limits.md) for interfaces and resource limits, and [ADR-0004](0004-performance-baseline-environment.md) for performance and development environments. The detailed Rust module structure remains an implementation decision.

Build an independent Rust crate in this repository for the user's own programs. Independence describes code organization and dependency boundaries; uploading the crate to a package registry is not required. Keep the trimmed C++ source for compatibility comparisons.

Other agents will develop the library on GitHub. Development and functional acceptance must be possible using this repository alone. The user will arrange integration into their actual program after the library is substantially complete; that integration is not a task for agents working in this repository.

The first version covers six target combinations: Windows with MSVC, Linux with glibc, and macOS, each on x86-64 and ARM64. Windows GNU, Linux musl, and 32-bit targets are outside the initial support commitment. Preserve the reference behavior on each platform under the agreed runtime conditions, including existing platform differences. The implementation of character-rule adapters remains to be designed.

Follow Rust stable without promising a historical minimum supported Rust version. There is no requirement to support old operating system versions. Use explicitly selected validation environments as the initial support baseline and record the actual operating system, runtime, and toolchain versions.
