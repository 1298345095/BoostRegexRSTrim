# Functional compatibility evidence

This document records the implemented scope, repeatable checks, observed results,
and acceptance results for the standalone Rust port. The reference is
the retained `boost/` implementation in this repository, including its existing
Windows byte-classification patch. Results are compared on the same native target
and under the same character rules; platform differences are preserved.

Passing these finite checks is evidence for the exercised behavior. It is not a
proof over every possible pattern, input, or replacement template. The retained
[capability inventory](capabilities.md) and [ADRs](adr/0001-rust-migration-scope.md)
define the scope beyond the sampled cases.

## Acceptance record

The implementation is published in [PR #26](https://github.com/1298345095/BoostRegexRSTrim/pull/26).
The final code snapshot passed all six native targets on 2026-10-24. The
[machine-readable evidence](native-functional-results.json) retains the native
results, environments, compiler/build details, executable and source hashes,
artifact identities, and common budget-boundary observations.

| Final acceptance field | Value |
| --- | --- |
| Final tested implementation commit | [`078a299fac1ef2993deedcba649aef74acd424c9`](https://github.com/1298345095/BoostRegexRSTrim/commit/078a299fac1ef2993deedcba649aef74acd424c9) |
| Successful six-target workflow run | [40066443631](https://github.com/1298345095/BoostRegexRSTrim/actions/runs/40066443631) |
| Final native matrix conclusion | **Passed: all six native targets** |
| Performance baseline and thresholds | Pending a user-designated measurement machine |
| Integration into the user's external application | Deferred under ADR-0002 |

This acceptance covers the recorded code snapshot and the scenarios below.
Subsequent documentation-only commits preserve that tested implementation.

## Native matrix

The [functional workflow](../.github/workflows/functional.yml) runs each executable
natively on its target. Cross-compilation alone is not native compatibility
evidence. The earlier
[reference-only run](https://github.com/1298345095/BoostRegexRSTrim/actions/runs/40065012916)
passed on all six targets.

The final native run passed every workflow gate. Each target also passed the
standalone consumer, Rust tests, documentation example, strict Clippy, 20
reference selftests and nine benchmark checksum checks.

| Target | Runner | Byte/error comparisons | Character comparisons | Accounting comparisons | Result |
| --- | --- | --- | --- | --- | --- |
| `x86_64-unknown-linux-gnu` | `ubuntu-24.04` | 17,772 | 8,790 | 1,601 | Passed |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` | 17,772 | 8,790 | 1,601 | Passed |
| `x86_64-pc-windows-msvc` | `windows-2025` | 17,772 | 8,790 | 1,601 | Passed |
| `aarch64-pc-windows-msvc` | `windows-11-vs2026-arm` | 17,772 | 8,790 | 1,601 | Passed |
| `x86_64-apple-darwin` | `macos-15-intel` | 17,772 | 5,860 | 1,601 | Passed |
| `aarch64-apple-darwin` | `macos-15` | 17,772 | 5,860 | 1,601 | Passed |

Across the six targets, this is **106,632 byte/error comparisons**, **46,880
character/formatter comparisons**, **9,606 accounting comparisons**, and **54
measurement-free benchmark checks**, with zero differences. Linux and Windows
each exercise three character scenarios; macOS exercises two. All jobs used
Rust 1.99.1; the evidence file records each target's native C++ compiler and
runtime conditions, including unsigned native `char` on Linux ARM64.

The [initial Rust native run](https://github.com/1298345095/BoostRegexRSTrim/actions/runs/40065979601)
exposed 68 named-locale class/boundary differences on each macOS architecture.
The correction preserves libc++'s classic byte-classification table while
retaining native named-locale case conversion and collation. The final run
passes the unchanged differential scenarios and two additional native
regressions covering all 256 bytes, 21 classes, case conversion and word
boundaries. No failing scenario was removed.

Each workflow run uploads an artifact named `functional-<target>`. Download it
from the run's **Artifacts** section. It contains reference build metadata and
JSON reports for functional comparisons, character scenarios, accounting, and
measurement-free benchmark checks. Metadata records the actual compiler,
runtime/locale, runner image and source/observer conditions. The functional and
accounting reports identify the executable hashes; mismatch records retain exact
pattern, input and template bytes in hexadecimal.

## Development evidence

The following Linux x64 results were obtained during integration of the complete
implementation. They supplement the native matrix and do not replace it.

| Check | Observed result | Durable entry point |
| --- | --- | --- |
| Main byte/error differential corpus | 25,772 comparisons, zero differences | `tools/differential.py --random-cases 3000` |
| Character and formatter scenarios | 8,790 comparisons, zero differences across default, `C.UTF-8` and `en_US.UTF-8` | `tools/character_differential.py` |
| Exact computation accounting | 1,601 comparisons, zero differences; 17 boundary/threshold probes | `tools/accounting_differential.py` |
| Rust unit and integration suite | 50 tests passed, plus the public API documentation example | `cargo test --all-targets --all-features`; `cargo test --doc` |
| Observed/uninstrumented C++ reference checks | 20 checks passed | `tools/reference_selftest.py` |
| Benchmark protocol and result checksums | Nine workloads passed; zero measurement samples | `tools/benchmark.py --check` |
| Focused development replay of `ACCEPT` interactions | 10,240 comparisons, zero differences after fixing raw capture endpoints and last-closed state | Minimized regression cases are retained in `tests/api.rs` and the main differential corpus |

The focused replay was a development diagnostic. Its representative regressions
are committed; the full exploratory replay is not a separate required CI suite.
Source-count assertions are functional checks, not timing or memory measurements.

## Ticket-to-test mapping

The table maps every implementation ticket to committed checks. `D` denotes the
[main differential corpus](../tools/differential.py), `C` the
[character/formatter scenarios](../tools/character_differential.py), and `A` the
[accounting comparison](../tools/accounting_differential.py). Additional public
API, parser and accounting regressions are in [`tests/`](../tests).

| Ticket | Implemented behavior | Evidence and focused checks |
| --- | --- | --- |
| [#1](https://github.com/1298345095/BoostRegexRSTrim/issues/1) | Byte-preserving C++ reference and six-target setup | `reference_selftest.py`, observed versus uninstrumented comparison, native reference run |
| [#2](https://github.com/1298345095/BoostRegexRSTrim/issues/2) | Literal compilation, search and full matching | D; API tests for empty input, NUL, invalid UTF-8, spans and no-match outcomes |
| [#3](https://github.com/1298345095/BoostRegexRSTrim/issues/3) | Escapes, anchors and inline options | D; numeric-boundary cases; parser `\R` expansion/depth tests; A restart modes |
| [#4](https://github.com/1298345095/BoostRegexRSTrim/issues/4) | Groups, captures and ordered alternatives | D; branch-reset numbering, empty/unmatched captures; A top-level start-marker placement |
| [#5](https://github.com/1298345095/BoostRegexRSTrim/issues/5) | Greedy/lazy repetition and capture restoration | D; nullable repeated groups and partial-map regressions; A repeat paths |
| [#6](https://github.com/1298345095/BoostRegexRSTrim/issues/6) | Byte sets and ranges | D/C; NUL/high bytes, negation, malformed endpoints and reversed ranges |
| [#7](https://github.com/1298345095/BoostRegexRSTrim/issues/7) | Windows classes and case mapping | C under default LCID, Japanese 1041 and Simplified Chinese 2052; Windows numeric regression |
| [#8](https://github.com/1298345095/BoostRegexRSTrim/issues/8) | Linux/macOS classes and case mapping | C under classic/default and named locales; macOS native regressions preserve classic classification and named case/collation behavior |
| [#9](https://github.com/1298345095/BoostRegexRSTrim/issues/9) | Collation, equivalence and extended/digraph sets | D/C; A verifies digraph sets keep the generic repeat path |
| [#10](https://github.com/1298345095/BoostRegexRSTrim/issues/10) | Named captures and backreferences | D; duplicate names, relative references, participation and case behavior |
| [#11](https://github.com/1298345095/BoostRegexRSTrim/issues/11) | Atomic groups and possessive repeats | D/A; atomic boundaries, scoped control verbs and nested matcher limit |
| [#12](https://github.com/1298345095/BoostRegexRSTrim/issues/12) | Positive/negative lookahead | D/A; capture visibility, failed assertions and `ACCEPT` interaction |
| [#13](https://github.com/1298345095/BoostRegexRSTrim/issues/13) | Positive/negative lookbehind | D; parser width validation, conditional paths and pending-alternative limit |
| [#14](https://github.com/1298345095/BoostRegexRSTrim/issues/14) | Subroutines and recursion | D/A; source return dispatch, saved captures, no-progress checks and depth-100 execution |
| [#15](https://github.com/1298345095/BoostRegexRSTrim/issues/15) | Conditional patterns and DEFINE | D/A; explicit empty else, assertion polarity, recursion conditions and retained possessive-condition syntax |
| [#16](https://github.com/1298345095/BoostRegexRSTrim/issues/16) | Control verbs and `\K` | D/A; scoped COMMIT/PRUNE/SKIP/THEN, ACCEPT capture ranges, bounded reversed-span test |
| [#17](https://github.com/1298345095/BoostRegexRSTrim/issues/17) | Ordered enumeration and empty-match retry | D/API; nonempty retry at the same position and endpoint-equality handling |
| [#18](https://github.com/1298345095/BoostRegexRSTrim/issues/18) | Global byte replacement | D/API; unmatched copying, prefix/suffix, empty templates, caller limits and atomic failure |
| [#19](https://github.com/1298345095/BoostRegexRSTrim/issues/19) | Complete retained formatter behavior | C/API; byte escapes, ambiguous/incomplete forms, case-state restoration, raw numbered endpoints and participating named lookup |
| [#20](https://github.com/1298345095/BoostRegexRSTrim/issues/20) | Malformed patterns and raw errors | D; parser/error tests; raw-error observer checks preserve bytes after NUL |
| [#21](https://github.com/1298345095/BoostRegexRSTrim/issues/21) | Computation and caller limits | A/accounting tests; five caller-limit boundary tests and recovery checks |
| [#22](https://github.com/1298345095/BoostRegexRSTrim/issues/22) | Shared compiled expressions and recovery | API concurrent reuse, recursion/assertion/replacement interactions, caller/computation failure recovery |
| [#23](https://github.com/1298345095/BoostRegexRSTrim/issues/23) | Reproducible six-target coverage | D/C/A, committed minimized regressions and native artifacts; all six native jobs passed with zero differences |
| [#24](https://github.com/1298345095/BoostRegexRSTrim/issues/24) | Standalone examples and acceptance documentation | README, examples, independent consumer and this record; native results and environment evidence recorded above |
| [#25](https://github.com/1298345095/BoostRegexRSTrim/issues/25) | Comparative measurement tooling | Nine checksum workloads, dry-run plan, metadata and designated-machine gate; actual baselines remain deferred |

## Reproducible inputs and seeds

The main corpus uses seed **1298345095**. CI selects **1,000** generated pattern
combinations; the expanded local run selects **3,000**. Each generated combination
has one to four bounded syntax pieces and an input of fewer than 16 bytes, and is
exercised through search, full match, enumeration and replacement. The corpus also
contains fixed feature cases, minimized failures, 512 arbitrary-byte compilation
cases, and 16 integer-boundary compilation/replacement requests. Fixed inputs
include empty bytes, NUL, `FF`, case variants, CRLF, newline and form feed.

The character/formatter corpus uses seed **189**, **2,000** generated templates,
and a corpus containing every byte from `00` through `FF`. Each scenario contains
2,930 requests. Platform defaults and explicit locales/LCIDs are separate
scenarios; unavailable environments and protocol failures fail the report.

The accounting corpus is deterministic rather than randomized. It discovers
the C++ complexity/storage transition within a declared bracket, then compares
both sides of the transition and the next input. Large inputs use the streaming
protocol to avoid Windows command-line limits. Executable copies are pinned for
the duration of the comparison, and their hashes are stored in the report.

From a native checkout with Rust stable, Python 3 and a C++17 compiler:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
python tools/build_reference.py
python tools/build_reference.py --baseline
python tools/reference_selftest.py --baseline target/reference/reference-baseline
cargo build --bin regex_driver --features diagnostics
python tools/differential.py --seed 1298345095 --random-cases 3000 --report reports/differential.json
python tools/character_differential.py --reference target/reference/reference --rust target/debug/regex_driver --report reports/characters.json
python tools/accounting_differential.py --reference target/reference/reference --rust target/debug/regex_driver --report reports/accounting.json
python tools/benchmark.py --check --reference target/reference/reference-baseline --rust target/debug/regex_driver --report reports/benchmark-plumbing.json
```

On Windows, supply `.exe` suffixes for executable paths. With an explicit Cargo
target, the Rust executable is under `target/<triple>/debug/`; pass that path
with `--rust` to each comparison tool. The native workflow contains the exact
platform-specific commands and MSVC environment selection.

## Exact logical budget boundaries

The source dispatch threshold for search-range length `N` and original pattern
byte length `S`, with each clamped to at least one, is:

```text
max(N * S * S + 100000, min(N * N + 100000, 100000000))
```

Checked signed-pointer overflow takes the source fallback. Only the input-square
term is normally capped. The threshold is tested after a failed handler;
successful dispatches and selected repeat unwind steps can make the final count
exceed the threshold before an error is raised. Rust allocation sizes do not
replace the original virtual stack-record sizes.

| Boundary exercised | Exact expected observation |
| --- | --- |
| Ordinary parser nesting | 399 nested groups compile; 400 fail with code 18 at position 400. Accepted forms also compile on a one-MiB thread stack. |
| Conditional assertion nesting | 199 levels compile; 200 fail with code 18 at position 1000. |
| Creator map recursion | 100 reachable leading word-start assertions compile; 101 fail with code 18 at position zero. |
| Lookbehind pending alternatives | The source checks `> 1024` before pushing; 1,026 sequential `(?:a|a)` groups inside lookbehind fail with the retained lookbehind error. |
| Nested matcher calls | Atomic depth 79 succeeds with 161 dispatched states; depths 80/81 fail with code 18 after 81 states. |
| Explicit pattern recursion | `(?<x>a(?&x)?b)` on 100 `a` bytes followed by 100 `b` bytes succeeds with 997 states and threshold 140,000. |
| Complexity transition | `(a+)+$` on `a^14 + b` returns no match after 73,726 states; `a^15 + b` fails with code 18 at 100,582 states against threshold 100,576; `a^16 + b` fails at 100,618 against 100,612. |
| Backtracking storage transition | `(a)*` on 36,900 `a` bytes succeeds after 184,503 states; 36,901 and 36,902 fail with code 19 after 184,507 states. |
| Cleanup and reuse | Successful and failed diagnostic calls restore the remaining-extra-block counter to 1,024; later calls on the same compiled expression succeed. |
| Threshold cap | Input length 9,995 reaches the 100,000,000 input-square cap; pattern length 101 with input length 10,000 has threshold 102,110,000. |

Caller limits for pattern, input, template, output and match count are tested
below, at and above the required size. Limit errors have caller-policy identity
and return no partial output or collection. They do not alter the source engine's
computation thresholds.

## Defined behavior at backwards replacement ranges

The source can expose a reversed whole-match span through `\K` in lookahead.
For `(?=a\K)` on `ab`, search returns endpoints `(1, 0)`. The retained iterator
uses endpoint equality to detect an empty match, so this pattern can repeat
without advancing. A caller match-count limit makes that behavior bounded;
the regression uses a limit of two and receives a match-limit error on result
three.

Overlapping match starts can also make replacement's unmatched-prefix range
backwards. For `a|(?<=\Ka)b` on `ab`, search/enumeration preserve spans `0..1`
and `0..2`. The C++ replacement then requests an invalid forward `std::copy`
range. Rust returns a structured `Execute` error with no reference error code
or position instead of panicking or pretending that undefined C++ copying has a
portable result. Numbered formatter ranges are checked on the same basis.

This boundary does not discard valid unmatched capture endpoints. `ACCEPT` can
leave a capture publicly unmatched while retaining nonempty stored endpoints;
the numbered and last-capture formatter forms read those endpoints. Named
expansion continues to select the first participating duplicate.

## Consumer isolation and deferred work

[`tests/consumer`](../tests/consumer) is a separate Cargo project with a path
dependency on the Rust library. The workflow builds and runs it offline after
dependency resolution with `CXX=missing-cxx-compiler`. It demonstrates compilation,
captures, named lookup, enumeration, replacement, full match and raw/display
errors. The [standalone examples](../examples) also demonstrate caller limits and
recovery. Normal caller builds do not invoke the reference scripts or build a
Boost bridge; the reference remains a separate test executable.

The benchmark check compares identical byte workloads and result checksums. Its
reports contain no elapsed-time or peak-memory samples in check mode. Actual
compilation, matching, replacement and memory baselines, acceptable regression
thresholds and performance acceptance remain pending a user-designated machine
under [ADR-0004](adr/0004-performance-baseline-environment.md). The
[measurement procedure](performance.md) records the later release-build and
reporting requirements. Integration into an external application remains the
user's later task under [ADR-0002](adr/0002-port-trimmed-boost-engine-to-rust.md).
