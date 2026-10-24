# Compatibility reference

The compatibility reference is the retained `boost/` source in this repository,
including the Windows individual-byte classification patch. It is compiled only
when explicitly requested by test tooling. Cargo builds and downstream library
users do not build, link, or execute C++.

## Build and validate

Use Python 3 and a C++17 compiler. On Windows, run from a Visual Studio developer
command prompt for the intended target architecture.

```sh
python tools/build_reference.py
python tools/build_reference.py --baseline
python tools/reference_selftest.py --baseline target/reference/reference-baseline
```

Windows executables have the `.exe` suffix; pass that suffix to `--baseline`.
`CXX` or `--cxx` selects the compiler. The build script uses the repository's
include directory explicitly and never downloads upstream Boost.

The build records the exact compiler command, compiler identification, operating
system, runtime, architecture, runner image identifiers, locale environment,
effective traits locale, `char` signedness, engine constants, original saved-state
sizes, and SHA-256 hashes of the executable, driver, and every retained header in
`target/reference/build-info.json`. The uninstrumented build has a separate
`build-info-baseline.json`. These reports are functional evidence, not performance
measurements. An actual six-target workflow run is required before claiming
six-target acceptance.

## Byte protocol, version 1

The driver reads one request per line. A request consists of exactly four fields
separated by literal tab bytes:

```text
operation<TAB>pattern_hex<TAB>input_hex<TAB>template_hex<NEWLINE>
```

The supported operations are `compile`, `search`, `full`, `find_all`, and
`replace`. All three byte fields use hexadecimal, including patterns, inputs,
and templates that would otherwise contain tabs, newlines, NULs, or invalid
UTF-8. Empty byte strings use empty fields. Both upper- and lowercase hexadecimal
digits are accepted. Each request creates an independently compiled expression;
errors in one request do not contaminate the next request. Carriage-return/line-feed
and line-feed request terminators are accepted.

Each reply is one JSON object on one line. The exact response shapes are:

| Operation | Response |
| --- | --- |
| `compile` | `{"ok":true,"mark_count":2}`; count excludes capture 0 |
| `search`, `full` | `{"ok":true,"match":null}` when absent, or a match object |
| `find_all` | `{"ok":true,"matches":[...]}` with ordered match objects |
| `replace` | `{"ok":true,"output_hex":"..."}` |
| Any failure | `{"ok":false,"phase":"compile","code":8,"position":2,"message_hex":"..."}` |

A match object is
`{"span":[start,end],"captures":[[start,end],null,[start,end],...]}`.
Offsets are byte offsets from the beginning of the original input; endpoints
are exclusive. Capture 0 is included. An unmatched capture is `null`; an empty
participating capture is `[offset,offset]`.

The retained `\K` operation inside assertions can produce a whole-match span
whose start exceeds its end. Only equal endpoints count as an empty match for
iterator advancement. Some such patterns repeat without progress; regression
tests observe their first matches through a caller-selected match-count limit
instead of performing unbounded collection.

Error phases are `compile`, `execute`, and `protocol`. A `boost::regex_error`
has its numeric retained error code and position. Non-regex exceptions and
protocol errors have `null` for both fields. `message_hex` is the raw error
message, with no Unicode conversion. The driver buffers entire results so an
execution failure cannot emit a partial JSON collection or replacement.

Example from Python, avoiding shell quoting of literal tabs:

```python
import json
import subprocess

query = "\t".join(["search", b"a\0b".hex(), b"xa\0b\xff".hex(), ""]) + "\n"
result = subprocess.run(
    ["target/reference/reference"], input=query, text=True,
    capture_output=True, check=True,
)
assert json.loads(result.stdout)["match"]["span"] == [1, 4]
```

## Complete raw error observation

The retained parser constructs a `std::string` error message containing arbitrary
pattern bytes. `boost::regex_error` originally keeps it in `std::runtime_error`,
whose `what()` accessor exposes a pointer without a length. Reading it with
`strlen` loses bytes after an embedded NUL. Reading past the terminator without
a known bound is not a portable way to recover the message.

The default reference build therefore generates a test-only header overlay under
`target/reference/raw-error-observer/`. It changes exactly four anchored portions
of `boost/regex/pattern_except.hpp`: both constructors initialize a copied
`std::string`, the exception contains that additional member, and it exposes a
`const std::string& raw_message() const noexcept` accessor. The original retained
headers are never written. Every replacement must match exactly once, otherwise
the build fails so a changed source cannot silently invalidate the observer.

The `std::runtime_error` constructor, `what()`, code, position, throw/copy path,
parser, matcher execution, formatter, and engine computation limits are unchanged. The
observer adds error-message storage only; it is not suitable for allocation or
memory-performance comparisons. `--baseline` builds the identical driver against
the original headers without the observer. The selftest compares both builds,
including NUL-containing diagnostics: existing fields and observable `what()`
prefixes must match, while the observed message additionally preserves its full
length. Performance tooling must use an uninstrumented reference.

## Compiled states and computation accounting

Additional diagnostic modes expose the retained implementation for structural
and boundary comparisons:

```sh
target/reference/reference --program 2861292b
target/reference/reference --accounting full 6162 6162
```

`--program PATTERN_HEX` returns the pattern byte length, capture count, initial
restart strategy, nullability, 256-byte initial start map, and every reachable
compiled state. State `kind` is the original numeric syntax-element enum.
`id`, `next`, and `alt` identify byte offsets from the first state, with `null`
for absent links. They are addresses relative to that compiled allocation,
not stable identifiers across compiler or architecture changes. Every non-null
link points to a state included in the dump.

State-specific fields retain capture/backstep indices, literal bytes, case
flags, set maps or long-set members, repeat bounds and IDs, leading/greedy flags,
branch nullability masks, and control-verb actions. Maps and strings use hex;
the original branch-map bits and numeric masks are preserved. Compilation
failure returns the same complete raw error as the ordinary `compile` request.
This mode works with both observed and uninstrumented executables.

`--accounting search|full PATTERN_HEX INPUT_HEX` returns the ordinary match or
execution error object plus:

```json
{
  "accounting": {
    "state_count": 3,
    "max_state_count": 100008,
    "blocks_remaining_after_cleanup": 1024
  }
}
```

These values belong to the single completed matcher call, including failed
calls. The block field is the original remaining-extra-block counter after
unwinding and cleanup; it is not peak block consumption or allocated memory.
Compile errors occur before a matcher exists and have no `accounting` field.
For an unrelated exception before counter initialization, `accounting` is null.

The observed build adds three const accessors to the generated
`boost/regex/v5/perl_matcher.hpp` overlay for the existing `state_count`,
`max_state_count`, and `used_block_count` members. One exact source anchor must
match. No data members, writes, execution hooks, or layout changes are added to
the matcher. Original and overlay hashes are recorded in
`accounting_observer` build metadata. The uninstrumented executable rejects
`--accounting`. The Rust test driver provides the same mode when built with
`--features diagnostics`; the ordinary public API does not need that feature.

For large inputs, use `accounting_search` or `accounting_full` as the operation
in the four-field TSV protocol. Pattern and input remain hex, and the template
field is decoded but unused. Each request produces exactly the same object as
the equivalent `--accounting` command. This permits storage-limit tests larger
than a platform's command-line length limit and multiple diagnostic requests in
one process. These two operations require the same observed/diagnostics builds
as the one-shot mode.

See [capabilities.md](capabilities.md) for source locations and original budget
rules. Measurement-free benchmark checks and designated-machine performance
commands are documented separately in [performance.md](performance.md).

## Runtime conditions

`reference --environment` prints the effective traits locale and low-level
conditions used by the engine. A process normally preserves the reference's
default: classic C++ locale on Unix and the default user LCID on Windows.

For explicitly selected character-rule scenarios, run:

```sh
target/reference/reference --locale C --environment
target/reference/reference --locale en_US.UTF-8
```

On Windows use `--lcid 0x0409`, `--lcid 0x0411`, or `--lcid 0x0804` for the
English, Japanese, and Simplified Chinese scenarios. The retained `basic_regex`
has no public `imbue`; the test driver uses a traits subclass whose constructor
calls the inherited traits `imbue` only when an LCID is requested. All
classification, case mapping, collation, and matching still use the retained
implementation. Without an explicit LCID the base traits constructor is used
unchanged. Setting a C++ locale does not override Windows LCID rules.

The six workflow labels were rechecked against [GitHub's runner
reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
and the [Windows ARM64 Visual Studio 2026
announcement](https://github.com/actions/runner-images/issues/14592):

| Runtime | Architecture | Runner |
| --- | --- | --- |
| Windows MSVC | x86-64 | `windows-2025` |
| Windows MSVC | ARM64 | `windows-11-vs2026-arm` |
| Linux glibc | x86-64 | `ubuntu-24.04` |
| Linux glibc | ARM64 | `ubuntu-24.04-arm` |
| macOS | x86-64 | `macos-15-intel` |
| macOS | ARM64 | `macos-15` |

Runner names describe requested environments. Uploaded build metadata identifies
the image and tools actually used by each acceptance run.
