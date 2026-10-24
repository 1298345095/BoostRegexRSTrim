# BoostRegexRSTrim

A byte-oriented Rust port of the trimmed Boost.Regex 1.89 implementation retained
in this repository. The Rust crate implements parsing, compilation, matching,
captures, enumeration and replacement. The original C++ headers are the
compatibility reference and are built only by the reference-test tools.

Patterns, inputs, replacement templates and raw diagnostics are byte sequences.
NUL and invalid UTF-8 are preserved. Positions are byte offsets into the original
input, and an unmatched capture is distinct from a participating empty capture.

## Use the crate

The package is `boost-regex-rs-trim`; its Rust import is `boost_regex_rs_trim`.
For a checkout beside your application:

```toml
[dependencies]
boost-regex-rs-trim = { path = "../BoostRegexRSTrim" }
```

```rust
use boost_regex_rs_trim::{Limits, Regex};

fn main() -> Result<(), boost_regex_rs_trim::Error> {
    let regex = Regex::compile(b"(?<word>[a-z]+)", Limits::UNLIMITED)?;
    let input = b"\xff\0abc 123";
    let matched = regex.captures(input)?.expect("the word participates");

    assert_eq!(matched.span(), 2..5);
    assert_eq!(regex.named_capture(&matched, b"word"), Some(2..5));
    assert_eq!(regex.replace(input, b"<$+{word}>")?, b"\xff\0<abc> 123");
    Ok(())
}
```

Run the examples with `cargo run --example bytes` and
`cargo run --example limits`. The independent project in
[`tests/consumer`](tests/consumer) demonstrates a path dependency and can build
with `CXX=missing-cxx-compiler`: normal library use has no C++ build step.

| Operation | API | Result |
| --- | --- | --- |
| Compile | `Regex::new(pattern)` / `Regex::compile(pattern, limits)` | Immutable, cheaply cloned compiled expression |
| Find a span | `find(input)` | `Option<Range<usize>>` |
| Find captures | `captures(input)` | `Option<Match>`; capture zero is the whole match |
| Test a search | `is_match(input)` | `bool` |
| Match the whole input | `full_match(input)` | `Option<Match>` |
| Enumerate | `find_all(input)` | Ordered `Vec<Match>` with the reference's empty-match retry |
| Replace globally | `replace(input, template)` | `Vec<u8>` using the retained default Perl formatter |

Every fallible operation returns `Result<_, Error>`. A successful search with no
match returns `Ok(None)`; it is distinct from an engine or caller-limit error.
Compiled expressions can be cloned and shared across threads. Each invocation
keeps its own captures, backtracking records and computation counters.

## Syntax and replacement behavior

The target is the normal Perl parser retained in this source snapshot. It
includes inline `i/m/s/x` options; captures, branch-reset and named groups;
greedy, lazy and possessive repeats; byte sets, POSIX classes and collating
elements; backreferences; atomic groups; lookahead and accepted fixed-width
lookbehind; recursion and subroutines; conditionals and DEFINE; control verbs;
and `\K`. The [capability inventory](docs/capabilities.md) links these behaviors
to the implementation tickets and identifies source-level accounting rules.

Default replacement supports numbered captures such as `$1`, the complete
match `$&`, named captures `$+{name}`, prefix/suffix forms, the retained
last-capture forms, byte escapes and case conversion. The formatter follows this
snapshot's treatment of incomplete and ambiguous templates; it is not a UTF-8
string formatter. The accepted syntax is defined by the reference, including
its edge cases, rather than by the syntax of another Rust regex crate.

## Caller limits and errors

`Limits` independently bounds `pattern`, `input`, `template`, `output` and
`matches`. Its default is `Limits::UNLIMITED`, which imposes no application
policy. Set limits appropriate to your caller:

```rust
use boost_regex_rs_trim::{Limits, Regex};

fn main() -> Result<(), boost_regex_rs_trim::Error> {
    let limits = Limits {
        pattern: 4_096,
        input: 1_048_576,
        template: 4_096,
        output: 2_097_152,
        matches: 10_000,
    };
    let regex = Regex::compile(b"[a-z]+", limits)?;
    assert!(regex.is_match(b"word")?);
    Ok(())
}
```

Collection and replacement failures return no partial result. Expressions stay
usable after caller-limit and computation-limit errors.

The source engine's computation limits remain active even with unlimited caller
policy. Their dispatch counts, parser/creator nesting, nested matcher calls and
virtual backtracking-block accounting are separate from Rust allocation sizes.
See [ADR-0003](docs/adr/0003-api-and-resource-limits.md) and the inventory.

`Error` exposes its `phase`, optional reference `code` and `position`, and
caller-limit details. `raw_message()` returns the exact diagnostic bytes;
`display_text()` and `Display` provide readable escaping. Use raw bytes for
compatibility comparisons, since messages may contain NUL or invalid UTF-8.

Match-start resets inside assertions can expose backwards or overlapping ranges.
Search preserves those reference spans. When such ranges would require an
invalid forward copy during replacement, Rust returns an execution error with
no reference code; the C++ replacement operation has undefined behavior there.
The [reference notes](docs/reference.md) describe the corresponding bounded
enumeration case.

## Platform character rules

The six initial native targets are:

| OS and native implementation | x64 | ARM64 |
| --- | --- | --- |
| Windows / MSVC | `x86_64-pc-windows-msvc` | `aarch64-pc-windows-msvc` |
| Linux / glibc | `x86_64-unknown-linux-gnu` | `aarch64-unknown-linux-gnu` |
| macOS / system libc | `x86_64-apple-darwin` | `aarch64-apple-darwin` |

Unix defaults to the classic C++ locale, independently of `LANG` and `LC_ALL`.
`CharacterRules::with_locale(name)` selects an installed native locale explicitly
without changing process-global locale state. Windows defaults to the user's
LCID; `CharacterRules::with_lcid(lcid)` selects it explicitly. Pass a snapshot to
`Regex::with_character_rules`. Byte classification, case conversion and
collation follow the relevant target; native signed-`char` differences in the
retained numeric escape parser are also preserved.

OS APIs supply locale behavior. The default crate has no external Rust crate
dependencies. Optional `measurements` tooling uses the pure Rust `libc` bindings
on Unix to obtain the OS resource-usage structure; it adds no separately built
C/C++ implementation library. Optional `diagnostics` exposes `Regex::diagnose`
and the reference-compatible logical counters used by the accounting tests.

## Build and verify

Use the stable Rust toolchain selected by `rust-toolchain.toml`:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo test --doc
```

Functional comparison additionally needs Python 3 and a native C++17 compiler:

```sh
python tools/build_reference.py
python tools/build_reference.py --baseline
python tools/reference_selftest.py --baseline target/reference/reference-baseline
cargo build --bin regex_driver --features diagnostics
python tools/differential.py --random-cases 3000
python tools/character_differential.py --reference target/reference/reference --rust target/debug/regex_driver
python tools/accounting_differential.py
python tools/benchmark.py --check --reference target/reference/reference-baseline --rust target/debug/regex_driver
```

On Windows, append `.exe` to the executable paths passed to Python tools. When
using `CARGO_BUILD_TARGET`, pass the Rust driver from
`target/<triple>/debug/regex_driver[.exe]` with `--rust`.

The [functional workflow](.github/workflows/functional.yml) runs natively on all
six targets, builds an uninstrumented reference alongside the diagnostic
observer, compares results and errors, exercises character scenarios and limits,
and uploads JSON evidence. [Reference documentation](docs/reference.md) describes
the byte protocol and why the raw-error observer preserves bytes beyond NUL.
The tracked `boost/` headers remain unchanged.

The [functional acceptance record](docs/functional-acceptance.md) maps all 25
tickets to their checks and records the successful six-target run. Its
[native result snapshot](docs/native-functional-results.json) preserves the
compiler conditions, hashes, comparison counts and exact budget observations.

Performance tooling is provided in [`tools/benchmark.py`](tools/benchmark.py).
Its default plan and `--check` mode collect no timing or memory measurements.
Measurements and regression thresholds remain pending a user-designated machine,
as required by [ADR-0004](docs/adr/0004-performance-baseline-environment.md).
See the [measurement procedure](docs/performance.md). Integration into an
external application is a later step, separate from this standalone crate.

## Source and license

The retained C++ snapshot removes wide-character/ICU, POSIX basic/Emacs parsing,
unused convenience APIs and nondefault replacement modes. Its normal parser,
matcher, formatter, errors and platform traits remain the behavior reference.
The Windows source retains its per-byte `GetStringTypeExA` classification patch
and zero-on-failure policy. The Rust implementation ports this snapshot rather
than substituting another regex engine.

The code is distributed under the [Boost Software License 1.0](LICENSE_1_0.txt).
Architecture and scope decisions are recorded in [`GLOSSARY.md`](GLOSSARY.md)
and [`docs/adr`](docs/adr).
