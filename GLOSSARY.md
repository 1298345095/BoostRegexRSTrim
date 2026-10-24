# Regex Behavioral Compatibility

This project provides regex behavior compatible with the trimmed Boost.Regex implementation for the user's own programs.

## Language

**Behavioral compatibility**:
The new and reference engines expose the same observable behavior for the agreed inputs, options, and runtime conditions. This does not require identical internals or removal of existing platform differences.
_Avoid_: Syntax similarity alone, identical source structure

**Compatibility reference**:
The current trimmed Boost.Regex implementation in this repository, including its existing patches, together with the target platform and runtime conditions used to judge compatibility.
_Avoid_: Unqualified "Boost behavior"

**Character rules**:
The rules governing character classification, case mapping, and collation. They may vary with the platform and locale.

**Raw error message**:
The complete byte sequence of an error message produced by the compatibility reference. It may contain invalid UTF-8.
_Avoid_: Error display text

**Error display text**:
A human-readable representation of an error message that escapes invalid UTF-8 bytes while leaving the raw error message unchanged.
_Avoid_: Raw error message

**Caller limits**:
Caller-selected limits on pattern, replacement template, input, and output sizes, and on match counts. These limits express the calling program's usage policy.
_Avoid_: Engine computation limits

**Engine computation limits**:
Limits used by the compatibility reference to terminate excessively complex computation. Their success and failure boundaries are part of the required behavior.
_Avoid_: Caller limits
