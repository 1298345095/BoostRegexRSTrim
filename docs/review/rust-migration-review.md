# Trimmed Boost.Regex Rust Migration Plan

Status: scope approved; implementation has not started. Other agents will implement the library on GitHub. This document defines scope and acceptance conditions that can be fulfilled within this repository.

The current work publishes planning documents and prepares an issue breakdown, not implementation or CI workflows. Formal specifications and implementation tasks belong in GitHub Issues under the repository conventions. The ticket breakdown requires separate approval before publication.

## 1. Goal and deliverables

Build an independent Rust crate in this repository for the user's own programs. Port the required engine logic from the trimmed Boost.Regex source so Rust programs can call it directly without Boost C++ compilation, linking, or bridging.

The first version includes the library and usage documentation, the repository's C++ reference source and test driver, functional validation on six GitHub Actions targets, compatibility tests and reports, and performance baselines and acceptance results from designated environments.

Scope follows the observable behavior required by the agreed interfaces. Further trimming is allowed for unnecessary paths. Callers supply patterns and replacement templates at runtime, so existing samples are not a syntax whitelist. Inventory every behavior reachable through retained entry points and inputs. Removing further functionality requires evidence that it is unreachable or outside the agreed interfaces.

Do not restore removed wide-character or ICU/Unicode interfaces, other syntax modes, or convenience overloads outside the agreed scope. Do not substitute an existing Rust regex engine for this port.

The library is for the user's own programs. Development and functional acceptance must depend only on this repository. Once the library is substantially complete, the user will arrange integration into the actual program separately; that integration is not an execution task for agents working here.

## 2. Compatibility reference

- The primary reference is the current trimmed implementation in this repository, including its Windows byte-classification patch, rather than unmodified upstream Boost 1.89.
- Retain the C++ source as an independent reference. Do not change its behavior to accommodate the Rust port.
- Compare both engines under the same platform and runtime conditions. Preserve existing platform and locale differences rather than forcing identical results across operating systems.
- Full upstream Boost 1.89 is an additional comparison only; it cannot override the trimmed reference.

Create an independent reference driver using the retained C++ entry points, and keep test cases in this repository. Collect compilation and execution results separately and distinguish errors from those two phases.

## 3. Rust interfaces and behavior

The required capabilities are below. Concrete Rust types and method signatures remain implementation decisions; reproducing C++ templates or class hierarchies is not required.

| Capability | Initial contract |
| --- | --- |
| Compilation | Accept byte patterns and match the retained normal/Perl syntax and inline options reachable from patterns. |
| Input | Accept arbitrary bytes, including embedded NUL and invalid UTF-8; do not require conversion to `&str`. |
| Searching | Support both overall-span-only results and results with captures. |
| Captures | Use byte ranges; distinguish unmatched captures from empty captures and retain supported named-capture behavior. |
| Full matching | Support matching the entire input. |
| Match enumeration | Preserve ordering, advancement after empty matches, and caller-selected match-count limits. |
| Replacement | Accept byte templates, return complete output bytes, enforce caller output limits, and retain formatter behavior. |
| Sharing and reuse | Compiled expressions can be shared across threads; executions have independent state; an execution failure does not invalidate normal reuse. |
| Errors | Expose structured categories, original positions, raw message bytes, and separate error display text. |

Reachable features such as backreferences, lookarounds, recursion, and conditionals cannot be omitted because sample configurations do not use them. Build the full capability inventory from the retained entry points and source; these examples are not an exhaustive list.

### Error representation

Preserve the original error category, position, wording, and complete raw message bytes, including embedded pattern fragments. Escape invalid UTF-8 bytes for display without lossy replacement of the original message. Differential tests compare raw error information; test the display representation separately. The exact escaping format remains an interface detail.

### Two kinds of resource limits

- **Caller limits:** accept caller-selected limits for pattern, replacement template, input, and output sizes, and match counts. Do not turn one program's business limits into universal defaults.
- **Engine computation limits:** strictly preserve the reference's logical accounting, thresholds, and resulting success and failure boundaries. Inventory parser depth, matching complexity, and backtracking-storage accounting instead of copying a single constant. Rust memory layout changes must not change budget semantics.

Exceeding a caller limit returns the corresponding error. Failed replacement and match collection return no partial results. Validate caller-limit errors separately from engine-computation errors.

## 4. Dependencies and implementation constraints

Implement core parsing, matching, captures, and replacement in Rust. Necessary pure Rust helper crates and operating-system APIs are allowed. Do not add C/C++ implementation libraries requiring separate builds or retain a production Boost C++ bridge.

C++ is a test reference only, and test tooling may compile it. Normal use of the Rust library must not require building C++ Boost or the reference driver.

Initially preserve the reference's parsing order, match priorities, and budget accounting. Refactor and optimize internals under differential validation afterward. Avoid replacing algorithms early and enlarging the set of possible causes for behavioral differences. This implementation strategy does not require mechanically translating every C++ class.

## 5. Platforms and toolchains

The initial functional matrix follows. Candidate runner labels were checked against GitHub's official documentation during planning. Recheck availability during implementation and record actual image versions; this table is not evidence that tests have already passed.

| System and runtime | Architecture | Candidate GitHub-hosted runner |
| --- | --- | --- |
| Windows / MSVC | x86-64 | `windows-2025` |
| Windows / MSVC | ARM64 | `windows-11-vs2026-arm` |
| Linux / glibc | x86-64 | `ubuntu-24.04` |
| Linux / glibc | ARM64 | `ubuntu-24.04-arm` |
| macOS | x86-64 | `macos-15-intel` |
| macOS | ARM64 | `macos-15` |

Windows GNU, Linux musl, and 32-bit targets are outside the initial commitment. Follow Rust stable without a historical MSRV commitment. There is no old-OS compatibility requirement; explicitly selected validation environments form the support baseline. Record Rust and C++ compiler versions, operating systems and runtimes, runner images, and locale conditions for acceptance runs.

Sources: [GitHub-hosted runner documentation](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) and [Windows ARM64 VS2026 general availability announcement](https://github.com/actions/runner-images/issues/14592).

CI must exercise character classification, case mapping, collation, and the Windows DBCS patch. Passing six jobs under default locale settings does not replace checking those semantics. A selected test corpus also does not narrow the character-rule behavior that the library must preserve.

## 6. Acceptance conditions

The following checks turn the agreed requirements into verifiable evidence.

| Area | Required outcome and evidence |
| --- | --- |
| Scope coverage | Inventory retained entry points, reachable syntax and states, formatter behavior, and error paths. Do not omit reachable capabilities merely because existing samples do not use them. |
| Valid reference | Compile the repository's trimmed reference with an independent driver. Separate compilation from execution and use byte-preserving encodings for inputs, outputs, and errors. |
| Functional comparison | On all six targets under matching conditions, compare success, spans, captures, enumeration order, replacement bytes, and error categories, positions, and raw messages. Leave no known unresolved differences. |
| Boundaries and combinations | Cover NUL, invalid UTF-8, empty patterns/inputs/templates, empty matches, capture participation, advanced syntax combinations, and failure paths. Add reproducible randomized comparisons and retain minimized failures. |
| Resource behavior | Test below, at, and above limits. Preserve engine accounting and independently test caller limits for pattern/template/input/output sizes and match counts. |
| Sharing and reuse | Concurrent executions sharing a compiled expression agree with independent executions. A failed execution does not contaminate later executions. |
| Error display | Keep complete raw bytes accessible, escape invalid UTF-8 for display, and leave raw messages used in comparisons unchanged. |
| Caller dependencies | Provide a minimal caller example in this repository demonstrating that library use does not require Boost C++ or a bridge. Reference tooling remains test-only. |
| Performance | Establish and review baselines on designated machines, then apply user-approved regression thresholds. Performance acceptance remains pending until the environments and thresholds are defined and measured. |

Differential tests provide acceptance evidence, not a mathematical proof for every possible input. Investigate and fix differences instead of silently changing the reference or skipping failing cases. Requirement changes need an explicit user decision and updated records.

### Performance measurements

Initially cover the user's designated primary runtime environments; the performance matrix need not match all six functional targets. Measure compilation, matching, replacement, and memory use, distinguishing typical workloads from complex expressions.

Measure both engines on the same designated machine, with identical inputs and comparable build conditions. Record methods, build options, toolchains, machine conditions, and repeated measurements before agreeing on regression thresholds. Do not compare absolute timings across different machines or substitute GitHub Actions functional job timings for formal performance acceptance.

Other agents will develop the repository on GitHub. The user will designate performance machines and approve regression thresholds later. Until formal measurements are complete, performance acceptance remains pending.

## 7. Implementation sequence

These steps are for subsequent implementation by agents on GitHub. The current task does not start implementation.

1. **Inventory capabilities.** Use this repository's C++ source to enumerate retained entry points, reachable syntax, formatter rules, errors, and resource accounting. Establish test cases in this repository.
2. **Build the reference driver and six-target CI.** Use retained entry points and report compilation and execution separately. Verify the driver's byte preservation, error classification, captures, and replacement output before introducing differential comparisons. Record platform and locale conditions with results.
3. **Establish Rust interfaces and character-rule adapters.** Define byte inputs, ranges, raw errors, display text, and both kinds of limits. Independently compare platform classification, case mapping, and collation behavior.
4. **Port compilation through matching and captures in vertical slices.** Start with complete basic-pattern behavior, then add retained advanced syntax and backtracking states. Preserve budget accounting from the start and compare each increment against the same reference.
5. **Complete enumeration, replacement, and interfaces.** Validate empty-match advancement, template formatting, reuse after errors, sharing, and caller limits.
6. **Resolve six-target compatibility differences.** Expand boundary, combination, and randomized comparisons. Validate locale-sensitive behavior and strict engine limits, and verify library use through a minimal caller example.
7. **Measure and optimize in designated environments.** Once environments are specified, compare both engines and agree on regression thresholds. Optimizations must preserve behavior and budget contracts and rerun affected differential tests.
8. **Complete acceptance and handoff.** Collect six-target results, coverage and difference records, performance reports, build conditions, and usage documentation. Mark unfinished acceptance areas as pending rather than equating completed coding with completed delivery.

## 8. Deliberately deferred details

- The user will designate performance test machines later.
- The user will arrange integration into the actual program once the library is substantially complete.
- Agree on performance regression thresholds after establishing baselines in designated environments.
- Select and record actual CI image versions, system conditions, and character-rule scenarios during implementation. The runner table contains viable candidates.
- Rust module structure, helper crates, API type names, and error escaping format are implementation details. They must satisfy the contracts above without introducing extra scope or weakening compatibility.

## 9. Decision records

- [ADR-0001: Migration scope and compatibility reference](../adr/0001-rust-migration-scope.md)
- [ADR-0002: Porting direction, deliverables, and platforms](../adr/0002-port-trimmed-boost-engine-to-rust.md)
- [ADR-0003: Interfaces and resource limits](../adr/0003-api-and-resource-limits.md)
- [ADR-0004: Performance baselines and development environments](../adr/0004-performance-baseline-environment.md)
- [ADR-0005: Library and reference-test dependencies](../adr/0005-runtime-and-reference-dependencies.md)
- [Glossary](../../GLOSSARY.md)
