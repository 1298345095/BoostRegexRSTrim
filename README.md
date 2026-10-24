# Pinned Boost.Regex 1.89

The retained entry points compile `char` byte patterns with normal syntax,
search with captures or an overall span, match complete inputs, iterate matches,
and replace through an output iterator and the default string formatter.
Their parser, matcher, formatter, error handling and transitive dependencies
remain, including syntax selected by user-supplied patterns and replacements.

The configuration is fixed to C++17, standalone headers, exceptions and threaded
caches. Win32 byte traits remain the default on Windows; other targets retain
the C++ locale traits (also selected by GCC on Windows, as upstream does).
The original cache mutexes and platform-dependent memory block cache remain.

Removed: ICU/Unicode and wide-character regex interfaces, MFC, concepts checks,
POSIX and deprecated traits, C locale traits, token iterators, grep/merge/split,
unused convenience overloads, user configuration, module/no-exception/non-threaded
branches and unused helper functions. The bridge and its Rust API are unchanged.

The function-level subset also omits legacy `reg_expression`, unused regex
constructors/assignment overloads, comparison and stream operators, and capture
string conversion APIs. Replacement formatting accepts the bridge's
`std::string` with the compiled regex traits directly; callback/C-string
dispatch, standalone formatting wrappers, fallback formatting traits and the
string output adapter are removed. Embedded NULs still use explicit ranges.

Parsing enters the normal (Perl) parser directly. POSIX basic/Emacs parsing and
its dedicated helpers are removed. Normal syntax still uses the original
literal parser, inline options, named captures, recursion and matching states.

Custom message catalogs are not exposed by the bridge. Their setters, loaders,
name locks, custom error/class/collation tables and Win32 module-loading imports
are removed. Default error strings, character classes, locale case mapping and
collation remain. The C++ locale cache key retains the ctype and collate facets
used by these operations; object-cache synchronization is unchanged.

Replacement uses the default Perl formatter only, so sed/conditional/literal
format modes and first-only/no-copy options are removed. Normal-syntax matching
uses Perl rules directly, without POSIX temporary results or longest-match
selection. POSIX character classes remain supported. The unused matcher flag
setters and non-random-access state-budget fallback are also removed.
`match_dot_repeat_slow()` and its runtime fallbacks remain intact, including
inline options that make dot repetitions stop at line separators.

The existing source patch in `boost/regex/v5/w32_regex_traits.hpp` remains: the ANSI
class-table initializer calls `GetStringTypeExA` for each individual byte and
stores zero on failure. Bulk classification of concatenated DBCS bytes can
leave the table tail unwritten. This carries the project's accepted initialized
byte-class policy and the existing native single-byte classification failure
policy. The native lowercase map is unchanged; the unused `BOOST_NO_ANSI_APIS`
fallback has been removed.

Header layout is consolidated for this single implementation. The public
`boost/regex.hpp`, `boost/regex_fwd.hpp`, `boost/regex/pattern_except.hpp` and
`boost/regex/regex_traits.hpp` contain their former v5 contents directly.
`boost/regex/v5/regex_algorithms.hpp` groups matching, searching, iteration and
replacement in dependency order. Iterator-category helpers live with the matcher,
and copying uses `std::copy` directly instead of a workaround header. Parser,
creator, matcher implementation, platform traits and cache headers remain split
by responsibility; this reorganization does not change their algorithms.

Unused configuration macros and empty export/no-exception markers are removed.
Constants, fallthrough attributes, throws and string comparisons use C++17 syntax
directly, and the internal namespace remains `re_detail_600`. Memory-block caching
keeps both original implementations, selected directly by
`ATOMIC_POINTER_LOCK_FREE`; the fixed cache size is 16 blocks. Platform locale
selection, assertions, matching limits and Win32 declarations remain unchanged.

The retained platform traits provide all required Boost extensions directly.
Custom-trait detection, fallback adapters and their private syntax/case helpers
are removed. Unused length helpers, width tags, non-char helper overloads and
match-result convenience overloads are also omitted. Named replacement captures
still use the original char-range lookup. Core parser/matcher templates,
long character sets, storage alignment and both
dot-repetition paths remain.

The replacement formatter reads the string's contiguous data range directly.
Numeric and named-capture expansion no longer adapts generic iterators through
temporary vectors. Explicit end pointers preserve empty replacements and embedded
NUL bytes; output still uses the bridge's bounded iterator.

The normal-syntax parser retains inline i/m/s/x options directly. Basic/Emacs
mode tests, disabled-capture handling, subexpression source-position recording,
non-throwing errors, newline-as-alternation and disabled character-class/escape
branches are removed. Empty alternatives and incomplete repetition literals keep
the upstream normal-syntax behavior. Long sets, named captures and recursion
remain supported.

Validate with `cargo test --workspace`,
`scripts/test-boost-span-native-contract.ps1` (MSVC and clang-cl), and
`scripts/test-regex-boost-oracle.ps1` against the full Boost 1.89 distribution
above. Use `-BuildDirectory` for a fresh native build and `-TargetDirectory`
for an independent Cargo/oracle directory. Keep the full oracle distribution
outside this subset so it remains an independent behavior reference.
