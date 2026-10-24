# Retained capability and accounting inventory

This inventory maps the retained C++ compatibility reference to implementation
tickets. A row describes reachable behavior; it is not a claim that its Rust
implementation or six-target acceptance is complete. Test results and unresolved
differences must accompany any completion claim. Normal syntax and default
matching/replacement are the required entry configuration, as recorded in the
[ADRs](adr/0001-rust-migration-scope.md).

## Entry points and syntax

The source of the entry-point inventory is
[`regex_algorithms.hpp`](../boost/regex/v5/regex_algorithms.hpp); the parser is
[`basic_regex_parser.hpp`](../boost/regex/v5/basic_regex_parser.hpp), with
post-parse validation in
[`basic_regex_creator.hpp`](../boost/regex/v5/basic_regex_creator.hpp).

| Capability | Retained behavior and branches | Ticket |
| --- | --- | --- |
| Reference | Byte protocol, phase-separated errors, six targets, source/conditions provenance | #1 |
| Compilation | Explicit byte range, empty expression, embedded NUL, high bytes, reusable compiled expression | #2 |
| Search/full match | Ordered search, whole-input matching, capture 0, no-match outcome | #2 |
| Literal escapes | Escaped punctuation/unknown escapes, bell, escape, form feed, newline, carriage return, tab, vertical tab, control byte, two-digit/braced hex, leading-zero octal, named character | #3 |
| Quoting | `\Q...\E`, empty/unclosed quoted regions, literal NUL/high bytes | #3 |
| Dot and separators | `.`, `\C`, `\X`, `\R`; retained byte separator and combining behavior, CRLF boundaries | #3, #8 |
| Anchors | `^`, `$`, `\A`, `\z`, `\Z`, backtick/apostrophe buffer anchors, `\G` continuation | #3, #17 |
| Inline options | Global/scoped `i`, `m`, `s`, `x`; enabling/disabling, restoration, comments, skipped whitespace | #3, #7, #8 |
| Groups | Capturing/noncapturing, nesting, empty groups/alternatives, ordered `|`, branch-reset numbering `(?|...)` | #4 |
| Repetition | `*`, `+`, `?`, bounded `{min,max}`, exact/open bounds, greedy/lazy choice, null repeated bodies, capture restoration | #5 |
| Byte sets | Positive/negative sets, initial `]`, literal/escaped delimiters and hyphens, ranges, high bytes/NUL, malformed/reversed ranges | #6 |
| Classes and boundaries | `\w`, `\d`, `\s`, horizontal/vertical and negated forms, POSIX classes, property names, `\b`, `\B`, `\<`, `\>` | #7, #8 |
| Windows character rules | Single-byte `GetStringTypeExA` patch, zero-on-failure class policy, native case maps, LCID/code-page differences | #7 |
| Unix character rules | C++ ctype/collate facet semantics on Linux/glibc and macOS; effective locale and signed `char` | #8 |
| Extended sets | Collating names `[.name.]`, equivalence `[=name=]`, POSIX classes including negation, long/digraph sets, collation-sensitive membership/ranges | #9 |
| Capture names | Named capture syntaxes, duplicate names, name hashing, leftmost participating named replacement lookup, branch-reset interactions | #10 |
| Backreferences | Numbered, named, relative forms accepted by parser; unmatched references, case folding, forward/invalid references | #10 |
| Atomic behavior | `(?>...)`, possessive repeats, captures and choice restoration around atomic boundaries | #11 |
| Lookahead | Positive/negative assertions, captures, nested/conditional assertions | #12 |
| Lookbehind | Positive/negative assertions, creator-calculated fixed lengths and equal-length alternatives, rejected forms | #13 |
| Recursion/subroutines | Whole-pattern, numbered, relative and named calls; forward resolution, capture save/restore, no-progress/infinite recursion | #14 |
| Conditions | Capture participation, named/numbered conditions, assertion and recursion conditions, DEFINE, both branches | #15 |
| Control verbs | FAIL/F, ACCEPT, COMMIT, PRUNE, SKIP, THEN; failure/unwind/search restart semantics | #16 |
| Match-start reset | `\K` span/prefix changes, interaction with captures and assertions | #16 |
| Enumeration | `regex_iterator`, original-input offsets, retry nonempty at same position after empty match, ordered exhaustion | #17 |
| Replacement | Copy unmatched regions, default formatter, empty matches and empty templates, arbitrary byte results | #18 |
| Advanced formatter | All dispatch branches listed below, locale case conversion, incomplete/ambiguous templates | #19 |
| Diagnostics | Parser/context messages, creator validation messages, execution errors, exact codes/positions/raw bytes, separate display text | #20 |
| Limits | Original logical budgets plus independent caller limits; atomic failure for collections/output | #21 |
| Sharing/recovery | Independent per-call state, concurrent compiled-expression use, reuse following failures | #22 |
| Functional evidence | Full six-target matrix, character scenarios, deterministic combinations, seeds and minimized regressions | #23 |
| Consumers | Standalone examples and dependencies, actual functional results, deferred program integration | #24 |
| Measurement tooling | Identical workloads/build conditions, compilation/matching/replacement/memory reports, designated-machine-only actual measurements | #25 |

Not all modern Perl/PCRE constructs are retained. Accepted forms and diagnostics
are determined by this parser, not an external regex implementation. Removed
interfaces such as wide-character/ICU, POSIX basic parsing, token iterators, and
alternate formatting modes do not enlarge the committed API.

## All 34 matcher dispatch states

The dispatch table in
[`perl_matcher_non_recursive.hpp`](../boost/regex/v5/perl_matcher_non_recursive.hpp)
and identifiers in [`states.hpp`](../boost/regex/v5/states.hpp) have these groups:

| States | Tickets |
| --- | --- |
| `startmark`, `endmark`, `literal`, `match` | #2, #4, #10 |
| `start_line`, `end_line`, `wild`, `buffer_start`, `buffer_end`, `soft_buffer_end`, `restart_continue`, `combining` | #3, #8, #17 |
| `word_boundary`, `within_word`, `word_start`, `word_end` | #7, #8 |
| `set`, `long_set` | #6–#9 |
| `jump`, `alt`, `rep`, `dot_rep`, `char_rep`, `short_set_rep`, `long_set_rep` | #4–#6, #9, #11 |
| `backref`, `assert_backref`, `toggle_case` | #7, #8, #10, #15 |
| `backstep`, `recurse` | #12–#15 |
| `fail`, `accept`, `commit`, `then` | #16 |

The 19 unwind entries are: end, paren, recursion stopper, assertion,
alternative, repeater counter, extra block, greedy single repeat, slow dot
repeat, fast dot repeat, character repeat, short-set repeat, long-set repeat,
non-greedy repeat, recursion, recursion pop, commit, then, and case restoration.
Every feature using these entries also owns its corresponding success/failure
accounting. Assertions and atomic groups share the nested matcher machinery;
subroutine call frames are a separate mechanism.

Search has seven restart choices: any position, word start, line start, input
start, continuation, literal prefix, and fixed literal. Optimizations may avoid
state dispatches and therefore participate in the observable budget contract.

## All default formatter branches

The source is [`regex_format.hpp`](../boost/regex/v5/regex_format.hpp).

| Dispatch | Retained forms | Ticket |
| --- | --- | --- |
| Literal | Any byte, including NUL/high bytes, through current case state | #18, #19 |
| Dollar | Trailing `$`, `$$`, `$&`, prefix ``$` ``, suffix `$'`, decimal `$n` and `${n}` | #18 |
| Named/last capture | `$+{name}`, `$+`, unmatched or missing groups, incomplete name delimiters | #18 |
| Perl names | `MATCH`, `PREMATCH`, `POSTMATCH`, `LAST_PAREN_MATCH`, `LAST_SUBMATCH_RESULT`, `^N`; braced/unbraced handling and optional caret in braces | #19 |
| Escapes | `\a`, `\f`, `\n`, `\r`, `\t`, `\v`, `\e`, `\cX`, trailing backslash, unknown escaped byte | #19 |
| Hex/octal | `\xHH`, `\x{...}`, `\0...`, invalid/incomplete rewind/literal behavior, native numeric conversion | #19 |
| Backslash capture | Single digit `\1`–`\9` | #18 |
| Case state | `\l`, `\u`, `\L`, `\U`, `\E`, next-byte restoration state, transitions while no byte is emitted | #19 |

Prefix expansions use the match result's current search prefix, including during
global replacement. They must not be approximated as the entire original input
prefix for every match. `LAST_PAREN_MATCH` and `LAST_SUBMATCH_RESULT` have different
capture-selection rules.

## Original engine computation accounting

These values are properties of the retained source, not caller defaults.
[`config.hpp`](../boost/regex/config.hpp),
[`perl_matcher_common.hpp`](../boost/regex/v5/perl_matcher_common.hpp), and
[`perl_matcher_non_recursive.hpp`](../boost/regex/v5/perl_matcher_non_recursive.hpp)
are authoritative.

| Accounting path | Exact condition / consequence |
| --- | --- |
| Parser nesting | `parse_all` preincrements its count; values greater than 400 raise code 18 with `Exceeded nested brace limit.` and parser context |
| Creator start maps | A `create_startmap` call rejects `recursion_count > 100` with code 18, position 0, `Expression complexity exceeded.`; recursive call sites preincrement the shared local count |
| Lookbehind analysis | Alternate-stack size is checked with `> 1024` before pushing; invalid length analysis returns the retained invalid-lookbehind diagnostic |
| Nested matcher calls | `match_all_states` preincrements `m_recursions`; values greater than 80 raise execution complexity, independently of explicit pattern recursion frames |
| State budget | Let `N=max(input search-range length,1)` and `S=max(original pattern byte length,1)`. The threshold is `max(N*S*S+100000, min(N*N+100000,100000000))`, subject to signed `ptrdiff_t` overflow checks. Only the second expression is normally capped. On a checked-overflow path use `min(100000000,PTRDIFF_MAX-2)` |
| State increments | Each dispatched state increments once; selected greedy/single-repeat unwind paths increment additionally. The threshold test is `state_count > max_state_count` after a state handler fails, not an unconditional pre-dispatch fuel decrement |
| Search restart | A `find_imp` call initializes one state count across attempted positions; each iterator search constructs a new matcher with the original base retained for context and the new search range for budgeting |
| Backtracking blocks | One initial 4096-byte block contains an end sentinel. `used_block_count=1024` permits that many additional blocks; extending with zero remaining raises code 19. Each extra block stores its predecessor metadata; unwind returns the block and increments the remaining count |
| Record sizes | Pushes consume the original saved-state record sizes, including alignment and extra-block overhead; runtime metadata reports the active platform's sizes. Rust allocation size cannot replace these logical sizes |
| No-progress repeat | Repeater tracking sets the repeat count to its maximum when the same position is revisited after a repetition; this changes both the next choice and accounting |
| No-progress recursion | Creator start-map recursion checks reject reachable infinite recursion; matcher call-site/position checks prevent loops in accepted recursive expressions |

All initial targets are 64-bit, but native `char` signedness and standard-library
locale behavior still differ. Hex braced and octal escape validation uses the
native `char` maximum in retained branches; two-digit hexadecimal has a distinct
validation path. The environment record captures signedness rather than assuming
it from architecture alone.

Caller limits on pattern, input, template, output and match count are separate
from this table. Each feature must test its own budget behavior before ticket #21
audits cross-feature boundaries. A generic instruction cap is not evidence of
strict accounting compatibility.

## Error families

The numeric definitions are in
[`error_type.hpp`](../boost/regex/v5/error_type.hpp); messages are in
[`regex_traits_defaults.hpp`](../boost/regex/v5/regex_traits_defaults.hpp) and
explicit parser/creator failure sites.

| Code | Family | Principal owner |
| --- | --- | --- |
| 2 | Bad pattern, forward recursion/capture condition, invalid lookbehind, infinite recursion | #13–#15, #20 |
| 3, 4 | Collation and character class | #6–#9 |
| 5, 6 | Escape and backreference | #3, #10 |
| 7, 8 | Set and parenthesis | #4, #6 |
| 9, 10, 11 | Brace, repetition bound, range | #3, #5, #6 |
| 12, 15 | Space/size enumerators; retain inventory even where the required entry configuration has no direct throw site | #20, #21 |
| 13 | Invalid repetition | #5, #11 |
| 17 | Empty-expression enumerator; normal syntax accepts empty expressions | #2, #20 |
| 18, 19 | Computation complexity and backtracking stack | Feature owners and #21 |
| 20, 21 | Perl extension and unknown/internal failure | Advanced syntax owners and #20 |

Codes 0, 1, 14 and 16 are marked unused by the retained enum. `std::exception`
failures without a `regex_error` identity are recorded separately by the driver.
Parser messages generally insert a bounded pattern fragment and `>>>HERE>>>`;
creator/execution messages may omit parser context. Raw bytes and display
escaping remain separate representations.
