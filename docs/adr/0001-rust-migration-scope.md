# Define the Rust migration scope through required interface behavior

The primary goal is to remove the Rust program's dependency on Boost C++ compilation, linking, and bridging. Platform system API calls remain allowed under [ADR-0005](0005-runtime-and-reference-dependencies.md). Compatibility covers the behavior required by the agreed interfaces. Further trimming is allowed for unnecessary paths; reproducing every remaining implementation detail is not a goal.

The compatibility reference is the current trimmed implementation in this repository on each target environment, including its existing patches. Full upstream Boost.Regex 1.89 is only an additional comparison and cannot override this reference.

Compare both engines on the same platform under the same agreed runtime conditions. Preserve existing platform and locale differences rather than requiring identical results across all platforms.

Preserve error categories, positions, wording, and raw message bytes. Human-readable output escapes invalid UTF-8 bytes instead of replacing or discarding them. Raw error messages and error display text are separate representations.

The required interfaces are described in [ADR-0003](0003-api-and-resource-limits.md). Callers supply patterns and replacement templates at runtime, so an existing sample corpus cannot justify removing reachable syntax. See [ADR-0002](0002-port-trimmed-boost-engine-to-rust.md) for the implementation direction.
