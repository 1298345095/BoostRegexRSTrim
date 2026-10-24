# Provide byte interfaces and distinguish two kinds of limits

The independent Rust crate provides byte-pattern compilation, searching, captures, full-input matching, match enumeration, and replacement. Positions use byte offsets, and compiled expressions can be shared across threads. Expose structured errors and raw error message bytes without reproducing C++ templates or class hierarchies. Error-message requirements are defined in [ADR-0001](0001-rust-migration-scope.md).

Pattern, replacement template, input, and output size limits, along with match-count limits, are caller policy. The library accepts caller-selected limits instead of hardcoding one program's business limits as universal defaults. Exceeding a caller limit returns the corresponding error. Failed replacement and match collection do not return partial results.

Engine computation limits must strictly match the trimmed compatibility reference. Preserve its logical accounting, thresholds, and resulting success and failure boundaries. Internal representation and memory layout may change, but these changes must not alter observable budget behavior.
