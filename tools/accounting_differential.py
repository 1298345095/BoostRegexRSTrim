#!/usr/bin/env python3
"""Compare source computation accounting and actual engine-limit boundaries.

This is a functional compatibility check. It does not collect elapsed time,
CPU measurements, memory measurements, or performance acceptance data.
Build the Rust driver with ``cargo build --bin regex_driver --features diagnostics``.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from tool_paths import driver_path, report_path, write_json_report


COMPLEXITY_PATTERN = b"(a+)+$"

def accounting_cases():
    groups = {
        "literal": [b"", b"a", b"ab", b"a\x00\xff"],
        "alternatives-captures": [b"a|b", b"ab|a", b"(a)", b"(?:a)", b"(a?)", b"(?|(a)|(b))"],
        "repetition": [b"a*", b"a+", b"a*?", b"a+?", b"a{1,3}", b".*", b"(?-s).*", b"[ab]*", b"[[.ae.]]*", b"(a)*", b"(a)+", b"(a?)+", b"(a|b)+", b"(a(b)?)+", COMPLEXITY_PATTERN],
        "anchors": [b"^a", b"\\Aa", b"\\ba", b"\\<a", b"a$", b"a\\Z", b"\\G."],
        "case-options": [b"(?i)a", b"a(?i)b", b"(?i:a)b"],
        "assertions-atomic": [b"(?=a)a", b"(?!(a))b|a", b"(?<=a)b", b"(?>a)", b"a*+"],
        "calls-conditions": [b"(a)(?1)b", b"a(?R)?b", b"(a)?(?(1)b)", b"(a)?(?(1)b|)", b"(a)?(?(1)b|c)", b"(?(?=a)a)", b"(?(?=a)a|)", b"(?(?=a)a|b)", b"(?(?!a)b|a)", b"(?(DEFINE)a)"],
        "control": [b"a(*COMMIT)b|ac", b"a(*SKIP)b|ac", b"a(*THEN)b|ac", b"a(*ACCEPT)b", b"(?=a(*ACCEPT)b)a", b"(?=a(*COMMIT)b)|a", b"(?=a(*PRUNE)b)|a", b"(?=a(*SKIP)b)|a", b"(?!a(*THEN)b)|a", b"(?>a(*THEN)b)|a"],
        "nested-control": [b"a*?(*THEN)b|a", b"(a|ab)(*THEN)c|ab", b"(?=a*(*COMMIT)b)|a", b"(?(?=a(*COMMIT)b)c|a)", b"(?>(?(?=a(*COMMIT)b)c|a))b|a", b"(?=a(*COMMIT)(*THEN)b)|a"],
    }
    inputs = [b"", b"a", b"b", b"ab", b"aa", b"ba", b"ac", b"aab", b"aaaaab", b"\na", b"a\n", b"aeae"]
    for family, patterns in groups.items():
        for pattern in patterns:
            for data in inputs:
                for operation in ("search", "full"):
                    yield family, operation, pattern, data


def invoke(executable, operation, pattern, data, accounting=True):
    command = [str(executable)]
    wire_operation = "accounting_" + operation if accounting else operation
    request = "\t".join((wire_operation, pattern.hex(), data.hex(), "")) + "\n"
    result = subprocess.run(command, input=request, text=True, capture_output=True, timeout=30)
    if result.returncode:
        raise RuntimeError(f"{executable.name} exited {result.returncode}: {result.stderr[-3000:]}")
    return json.loads(result.stdout)


def failure_code(result):
    return result.get("code") if not result.get("ok") else None


def first_limit_failure(reference, pattern, make_input, low, high, code):
    """Locate a source boundary without assuming native record sizes."""
    if failure_code(invoke(reference, "full", pattern, make_input(low), False)) == code:
        raise RuntimeError(f"expected successful low side for code {code}")
    if failure_code(invoke(reference, "full", pattern, make_input(high), False)) != code:
        raise RuntimeError(f"expected code {code} at high side {high}")
    while low + 1 < high:
        middle = (low + high) // 2
        if failure_code(invoke(reference, "full", pattern, make_input(middle), False)) == code:
            high = middle
        else:
            low = middle
    return high


def boundary_cases(reference):
    # The native matcher limit counts nested atomic matcher invocations, whereas
    # explicit subroutine frames use the separate backtracking-block budget.
    for depth in (78, 79, 80, 81):
        yield "nested-matcher-depth", "full", b"(?>" * depth + b"a" + b")" * depth, b"a", True
    for depth in (79, 80, 81, 100):
        yield "explicit-recursion-depth", "full", b"(?<x>a(?&x)?b)", b"a" * depth + b"b" * depth, True

    complexity = first_limit_failure(reference, COMPLEXITY_PATTERN, lambda n: b"a" * n + b"b", 1, 30, 18)
    for length in (complexity - 1, complexity, complexity + 1):
        yield "complexity-transition", "full", COMPLEXITY_PATTERN, b"a" * length + b"b", True

    storage = first_limit_failure(reference, b"(a)*", lambda n: b"a" * n, 1, 50_000, 19)
    # All requests travel over stdin, avoiding the Windows command-line limit.
    for length in (storage - 1, storage, storage + 1):
        yield "backtracking-storage-transition", "full", b"(a)*", b"a" * length, True

    for pattern_length, input_length in ((1, 9994), (1, 9995), (101, 10_000)):
        yield "complexity-threshold-cap", "full", b"b" * pattern_length, b"a" * input_length, True


def describe(family, operation, pattern, data, expected, actual, accounting):
    return {
        "family": family,
        "operation": operation,
        "pattern_hex": pattern.hex(),
        "input_hex": data.hex(),
        "input_length": len(data),
        "accounting_observed": accounting,
        "reference": expected,
        "rust": actual,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=Path("target/reference/reference"))
    parser.add_argument("--rust", type=Path, default=Path("target/debug/regex_driver"))
    parser.add_argument("--report", type=Path, default=Path("reports/accounting.json"))
    parser.add_argument("--boundaries-only", action="store_true")
    args = parser.parse_args()
    try:
        args.reference = driver_path(args.reference)
        args.rust = driver_path(args.rust)
        args.report = report_path(args.report)
    except ValueError as error:
        parser.error(str(error))
    args.report.parent.mkdir(parents=True, exist_ok=True)
    mismatches = []
    boundaries = []
    total = 0
    # Pin both executables. Concurrent development must not mix binary versions
    # halfway through a report, especially default versus diagnostics builds.
    with tempfile.TemporaryDirectory(prefix="accounting-", dir=args.report.parent) as temporary:
        suffix = ".exe" if os.name == "nt" else ""
        reference = Path(temporary) / ("reference" + suffix)
        rust = Path(temporary) / ("rust" + suffix)
        shutil.copy2(args.reference, reference)
        shutil.copy2(args.rust, rust)
        versions = {"reference": hashlib.sha256(reference.read_bytes()).hexdigest(),
                    "rust": hashlib.sha256(rust.read_bytes()).hexdigest()}
        invoke(rust.resolve(), "full", b"a", b"a")
        reference = reference.resolve()
        rust = rust.resolve()
        if not args.boundaries_only:
            for family, operation, pattern, data in accounting_cases():
                expected = invoke(reference, operation, pattern, data)
                actual = invoke(rust, operation, pattern, data)
                total += 1
                if expected != actual:
                    mismatches.append(describe(family, operation, pattern, data, expected, actual, True))
        for family, operation, pattern, data, observed in boundary_cases(reference):
            expected = invoke(reference, operation, pattern, data, observed)
            actual = invoke(rust, operation, pattern, data, observed)
            total += 1
            row = describe(family, operation, pattern, data, expected, actual, observed)
            # Long storage probes remain exactly reproducible from their byte
            # and length specification without duplicating thousands of bytes.
            if len(data) > 1000:
                row["input_repeat_hex"] = "61"
                row.pop("input_hex")
            boundaries.append(row)
            if expected != actual:
                mismatches.append(row)
        environment = json.loads(subprocess.check_output([str(reference), "--environment"], text=True))
    report = {"schema": 1, "kind": "functional-computation-accounting", "cases": total,
              "mismatch_count": len(mismatches), "mismatches": mismatches,
              "boundaries": boundaries, "executables_sha256": versions,
              "reference_environment": environment, "performance_measured": False}
    write_json_report(args.report, report)
    print(f"{total} accounting/boundary comparisons; {len(mismatches)} mismatches; {args.report}")
    for row in mismatches[:12]:
        print(json.dumps(row))
    return bool(mismatches)


if __name__ == "__main__":
    raise SystemExit(main())
