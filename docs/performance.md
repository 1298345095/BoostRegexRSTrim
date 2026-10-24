# Comparative performance measurement

Performance baselines, thresholds, and acceptance remain pending. Under
[ADR-0004](adr/0004-performance-baseline-environment.md), actual measurements run
only on a machine designated by the user. CI results are functional evidence;
their timings are not performance acceptance.

`tools/benchmark.py` implements repeatable comparative measurement plumbing for
compilation, searching, full matching, replacement, and process peak memory. It
defaults to a **dry run** that writes a plan without launching either engine.
The `--check` mode executes identical byte workloads and compares checksums while
collecting no timing or memory measurements. These modes can validate the tooling
before the user designates a measurement machine.

## Build and check the plumbing

Build the uninstrumented C++ reference and the Rust executable in release mode:

```sh
python tools/build_reference.py --baseline --profile release --output target/benchmark/reference-baseline
cargo build --release --features measurements --bin regex_driver
python tools/benchmark.py --check --reference target/benchmark/reference-baseline --rust target/release/regex_driver --report reports/benchmark-plumbing.json
```

The `measurements` feature enables the OS memory-statistics wrapper. On Unix it
uses the pure Rust `libc` ABI definitions; that optional helper is absent from
ordinary library dependencies. Check mode works without the feature; measured
mode reports a build instruction if it is absent, before starting the loop.

Use `.exe` suffixes on Windows. If Cargo has an explicit build target, the Rust
executable is at `target/<triple>/release/regex_driver[.exe]`. Select that path with
`--rust`. The C++ build requires the matching Visual Studio developer environment
on Windows, as described in [reference.md](reference.md).

The C++ release profile uses `-O3 -DNDEBUG` with GCC/Clang or `/O2 /DNDEBUG` with
MSVC; Cargo uses its release profile. The build metadata records the full
commands and compiler versions. These are optimized builds with assertions
disabled; toolchain-specific settings should be reviewed with the baseline
results before agreeing on thresholds. The raw-error observer must not be used
for performance measurements because it retains an extra error-message copy.
The harness rejects an observed or non-release C++ build for `--run`.
It also verifies the executable against the C++ build record's SHA-256 and
requires the Rust executable to report disabled debug assertions, enabled
measurements, and disabled diagnostic instrumentation.

Create a plan without executing workloads:

```sh
python tools/benchmark.py --dry-run --report reports/benchmark-plan.json
```

The report contains byte-exact workloads, proposed build and measurement
commands, sample counts, iteration counts, and available build/environment
metadata. It contains no measurement samples or performance summaries.

## Run on a designated machine

After the user designates the machine, execute there with its agreed identifier:

```sh
python tools/benchmark.py --run --designated-machine USER-APPROVED-MACHINE --reference target/benchmark/reference-baseline --rust target/release/regex_driver --iterations 1000 --samples 7 --machine-notes "Record power mode, background load, virtualization, and other conditions" --report reports/benchmark-results.json
```

Omitting `--designated-machine` rejects `--run` before either engine starts. The
identifier records the user's designation; naming a machine in a command does
not itself establish that the user approved it. Do not substitute CI machines
or the planning computer for the designated environment.

First the harness compares the two engines' measurement-free checksums. It then
collects the requested number of independent process samples, alternating which
engine goes first to reduce order bias. Every measured sample uses identical
pattern, input, template, and iteration values for both engines. A missing
measurement, execution error, checksum difference, or invalid response fails the
report; it is not silently dropped from the statistics.

The resulting JSON retains all per-sample measurements and reports each engine's
median, minimum, and maximum nanoseconds per iteration, together with maximum
process peak memory. No regression threshold is assumed. The report retains
`performance_acceptance: pending_user_baselines_and_thresholds` even when
measurement plumbing succeeds.

## Measurement boundaries

| Measurement | Included | Excluded |
| --- | --- | --- |
| Compile loop | Construct, compile, consume mark count/pattern length, destroy compiled regex each iteration | Process startup and hex decoding |
| Search/full loop | Execute the reusable compiled expression, allocate results, consume capture participation and byte spans | Initial compilation, process startup, hex decoding |
| Replacement loop | Enumerate matches, expand templates, allocate output, consume all output bytes | Initial compilation, process startup, hex decoding |
| Process peak memory | OS process peak RSS or peak working set, including startup, inputs, compilation, and execution | Per-operation attribution or subtraction of runtime overhead |

Both engines use a monotonic clock around their internal operation loop. Result
consumption is deliberately included in both loops to keep the work observable
and comparable. A sample launches a fresh process; there is no implied
cross-process cache warmup. Increase iterations enough to make process-independent
loop measurements stable, then examine the retained individual samples.

On Linux, `ru_maxrss` is converted from KiB to bytes. On macOS it is already bytes.
On Windows the metric is `PeakWorkingSetSize` from the operating system. These
are process-wide peaks, not allocation counts or live heap sizes, and must not
be compared across different machines or runtime environments as though they
were the same experiment. Memory overhead from each runtime remains part of the
reported process measurement.

The unit handling follows the [Linux `getrusage` manual](https://man7.org/linux/man-pages/man2/getrusage.2.html),
the current Apple kernel's [`ru_maxrss` assignment](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_resource.c)
and [byte-sized resident-memory definition](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/mach/task_info.h),
and Microsoft's [`PROCESS_MEMORY_COUNTERS` documentation](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters).

The report includes executable SHA-256 hashes, Rust and driver source hashes,
retained reference header hashes,
C++ build metadata, the declared Rust release command, Cargo lockfile hash,
Rust/Cargo versions, architecture, OS/runtime information, locale environment,
engine environment responses, and supplied machine notes. Rebuild and record a
new run if an executable, workload, compiler, or relevant runtime condition
changes.

## Workloads and driver contract

The built-in workloads exercise arbitrary byte literals, groups and alternatives,
literal search, captures/backreferences, lookaround, recursive expressions,
backtracking, capture expansion, and case conversion. They are a starting set
for measurement, not a claim of representative application traffic. The user
can supply a JSON array with `--workloads`:

```json
[
  {
    "name": "literal-search",
    "operation": "search",
    "pattern_hex": "6162",
    "input_hex": "78616279",
    "template_hex": ""
  }
]
```

Each record has exactly those five keys. Operations are `compile`, `search`,
`full`, or `replace`; names are unique; hexadecimal byte fields are contiguous
and have even length. No text encoding conversion is involved.

Each driver accepts:

```text
--benchmark-check OPERATION PATTERN_HEX INPUT_HEX TEMPLATE_HEX ITERATIONS
--benchmark OPERATION PATTERN_HEX INPUT_HEX TEMPLATE_HEX ITERATIONS
```

Check mode returns `ok`, `operation`, `iterations`, `checksum`, and
`measured:false`. It does not call a performance clock or read process memory.
Measured mode adds `elapsed_ns`, `peak_rss_bytes`, and `measured:true`.

The checksum begins at unsigned 64-bit `14695981039346656037`. Each value updates
it using wrapping multiplication by `1099511628211`, then XOR with the value.
Compilation feeds mark count excluding group 0 and pattern byte length. Matching
feeds match presence, then capture count including group 0, and each capture's
participation followed by its start/end offsets when participating. Replacement
feeds output length followed by each byte. The checksum persists across
iterations. Checksums validate that both timed loops consume equivalent results;
the full functional differential suite remains the compatibility gate.
