#!/usr/bin/env python3
"""Reproducible byte-for-byte functional comparisons; never a timing benchmark."""

import argparse
import hashlib
import json
import os
import platform
import random
import subprocess
from pathlib import Path

from tool_paths import character_command, character_options, driver_path, report_path, write_json_report


def corpus(seed, random_cases):
    patterns = [
        b"", b"abc", b"a\x00\xff", b"\\Q.a*\\E", b"a\\tb", b"\\x61+", b"\\cA", b"\\012",
        b".", b"(?-s).", b"(?s).", b"(?x) a # comment\n b", b"^a$", b"(?-m)^a$", b"\\Aa\\z",
        b"a\\Z", b"\\G.", b"\\bword\\b", b"\\B", b"\\<a\\>", b"\\R", b"\\X", b"\\C",
        b"(a)", b"(a)?b", b"(a*)", b"(a|ab)c", b"(ab|a)b", b"(|a)", b"a|", b"(?:a|b)+",
        b"(?|(a)|(b)(c))(d)", b"(?<word>a+)", b"(?'word'a)", b"(?<x>a)|(?<x>b)",
        b"a*", b"a+", b"a?", b"a{2}", b"a{1,3}", b"a*?", b"a+?", b"a{1,3}?", b"(a?)*", b"(a*)+",
        b"(a|aa)*b", b"(a|b)+", b"(a(b)?)+", b"((a)|b)+", b"(?:)*", b"(?:a?){2,4}", b"(?:(a)|b)+",
        b"[a-z]+", b"[^a]+", b"[\\x00-\\x20]", b"[\xff\x00]", b"[[:alpha:]]", b"[[:^digit:]]",
        b"(?i)[a-z]+", b"[[:word:]]", b"[[:blank:]]", b"[[:ascii:]]", b"[[:unicode:]]",
        b"[[.newline.]]", b"[[.ae.]]", b"[a[.ae.]]", b"[[=a=]]", b"[[.ae.]a-z]", b"[^a[.ae.]]",
        b"(a)\\1", b"(a)?b\\1", b"(?i)(a)\\1", b"(?<x>a)\\k<x>", b"(?<x>a)\\g{x}",
        b"(?<x>a)|(?<x>b)\\k<x>", b"(a)(b)\\g{-2}", b"(a|ab)\\1",
        b"(?>a|ab)c", b"a*+a", b"a++b", b"(a|ab)++c", b"(?>(a+))a", b"(?>((a)))b|a",
        b"(?=a)a", b"(?!a)b", b"(?=(a))a", b"(?=(a))b|a", b"(?!(a)b)a|a", b"(?!(a))b|a",
        b"(?<=a)b", b"(?<!a)b", b"(?<=(a))b", b"(?<=ab|cd)e", b"(?<=a|bc)d",
        b"(a)(?1)b", b"(?<x>a(?&x)?b)", b"(a(?1)?b)", b"a(?R)?b", b"(?(DEFINE)(?<x>a))(?&x)",
        b"(a)?(?(1)b|c)", b"(?(?=a)a|b)", b"(?(?!a)b|a)", b"(?<x>a)?(?(<x>)b|c)", b"(a(?(R)b|c))",
        b"a\\Kb", b"a(*FAIL)|b", b"a(*ACCEPT)b", b"(a(*ACCEPT)b)c", b"a(*COMMIT)b|ac",
        b"a(*PRUNE)b|ac", b"a(*SKIP)b|ac", b"a(*THEN)b|ac", b"a(*THEN)(b|c)d|abx",
        b"a(?i)b|c", b"(?i:a)b", b"(a(?i)b)c", b"(?i)(a)(?-i)\\1", b"(?=a(?=a))a",
        b"\\", b"[", b"(", b")", b"*", b"a**", b"[z-a]", b"a{2,1}", b"(?", b"(?)", b"(?q)",
        b"\\x{ff}", b"\\g{missing}", b"(?<x>a", b"(?R)", b"(?<=a+)b", b"\x00\xff[", b"a\x00\\",
        # Minimized regressions from structural/parser/accounting audits.
        b"((a)?)*", b"(a?)+", b"(?:$)+?", b"(?:$)++", b"((?>(?:(?=a))*)(?:$){1,3})",
        b"(?=a(*ACCEPT)b)a", b"(a)(b)(*ACCEPT)c", b"a\\Kb|ab",
        b"[[.ae.]]*", b"[[.ae.]]+", b"[[.ae.]]+?ae", b"[[.ae.]]*+ae",
        b"(?=a(*COMMIT)b)|a", b"(?=a(*PRUNE)b)|a", b"(?=a(*SKIP)b)|a",
        b"(?!a(*THEN)b)|a", b"(?>a(*THEN)b)|a",
        b"(?=(a)(*COMMIT)b)c|ac", b"(?=(a)(*PRUNE)b)c|ac", b"(?=(a)(*SKIP)b)c|ac",
        b"(?!(a)(*THEN)b)c|ac", b"(?>(a)(*THEN)b)c|ac",
        b"(a)?(?(1)b)", b"(a)?(?(1)b|)", b"(?(?=a)a)", b"(?(?=a)a|)",
        b"(?<=(?(1)a|b))", b"(?<=(?(?=a)a|b))", b"[--[",
        b"(?(?=a)*b)", b"(?(1)*b)", b"(?(?=a)*+b)", b"(?(1)*+b)",
        b"(*ACCEPT)(a)", b"(a)(*ACCEPT)(b)", b"(?<x>a)?(*ACCEPT)(?<y>b)",
    ]
    inputs = [b"", b"a", b"b", b"ab", b"abc", b"aab", b"aaabbb", b"baaaac", b"abab", b"ac", b"abx",
              b"a\nb", b"\r\n", b"a\r\n\f", b"word a", b"Ae aE ae", b"a\x00\xff", b"\x00\xff", b"AAb"]
    templates = [b"<$&:$1:$2>", b"$`|$'|$+|$^N", b"$+{x}\\U$&\\E", b"\xff\x00$$\\n"]
    yield from fixed_cases(patterns, inputs, templates)
    # This seed exists only to replay the test corpus; no security decision or
    # identifier depends on the pseudorandom values.
    rng = random.Random(seed)
    yield from generated_cases(rng, random_cases, templates)
    yield from malformed_cases(rng, random_cases)
    yield from integer_boundary_cases()


def fixed_cases(patterns, inputs, templates):
    for pattern in patterns:
        yield "compile", pattern, b"", b""
        for data in inputs:
            for operation in ("search", "full", "find_all", "replace"):
                yield operation, pattern, data, templates[(len(pattern) + len(data)) % len(templates)] if operation == "replace" else b""


def generated_cases(rng, random_cases, templates):
    atoms = [b"a", b"b", b".", b"[ab]", b"[^a]", b"\\d", b"\\w", b"(a)", b"(?:ab|a)", b"(?=a)a", b"(?!b).", b"\\x00", b"\\b"]
    quantifiers = [b"", b"?", b"*", b"+", b"{1,3}", b"*?", b"+?", b"?+", b"*+"]
    for _ in range(random_cases):
        pieces = [rng.choice(atoms) + rng.choice(quantifiers) for _ in range(rng.randrange(1, 5))]
        pattern = (b"|" if rng.randrange(4) == 0 else b"").join(pieces)
        if rng.randrange(5) == 0:
            pattern = b"(?i)" + pattern
        data = bytes(rng.choice(b"aabB12 \n\x00\xff") for _ in range(rng.randrange(16)))
        for operation in ("search", "full", "find_all", "replace"):
            yield operation, pattern, data, rng.choice(templates) if operation == "replace" else b""


def malformed_cases(rng, random_cases):
    # Arbitrary bytes exercise raw diagnostic context and target char signedness
    # without executing potentially nonprogressing accepted expressions.
    malformed_alphabet = b"[](){}?*+|\\01239<>=!:.-'ismxabc\x00\x7f\x80\xff"
    for _ in range(min(random_cases, 512)):
        pattern = bytes(rng.choice(malformed_alphabet) for _ in range(rng.randrange(32)))
        yield "compile", pattern, b"", b""


def integer_boundary_cases():
    # The native Windows integer helper checks its threshold after arithmetic.
    # Keep its boundary behavior visible in the native matrix, including formatter casts.
    for number in (b"9223372036854775806", b"9223372036854775807", b"9223372036854775808", b"9223372036854775809"):
        yield "compile", b"a{" + number + b"}", b"", b""
        for template in (b"$" + number, b"${" + number + b"}", b"\\x{" + number + b"}"):
            yield "replace", b"a", b"ab", template


def run_driver(executable, cases, *, locale=None, lcid=None):
    command = character_command(executable, locale=locale, lcid=lcid)
    requests = "".join("\t".join((op, pattern.hex(), data.hex(), template.hex())) + "\n" for op, pattern, data, template in cases)
    result = subprocess.run(command, input=requests, text=True,
                            capture_output=True, timeout=180, shell=False)
    if result.returncode:
        raise RuntimeError(f"{executable} exited {result.returncode}: {result.stderr[-4000:]}")
    lines = result.stdout.splitlines()
    if len(lines) != len(cases):
        raise RuntimeError(f"{executable} returned {len(lines)} responses for {len(cases)} requests")
    return [json.loads(line) for line in lines]


def driver_environment(executable, *, locale=None, lcid=None):
    command = character_command(executable, locale=locale, lcid=lcid, environment=True)
    return json.loads(subprocess.check_output(command, text=True, timeout=30, shell=False))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=Path("target/reference/reference"))
    parser.add_argument("--rust", type=Path, default=Path("target/debug/regex_driver"))
    parser.add_argument("--report", type=Path, default=Path("reports/differential.json"))
    parser.add_argument("--seed", type=int, default=1298345095)
    parser.add_argument("--random-cases", type=int, default=1000)
    parser.add_argument("--locale")
    parser.add_argument("--lcid")
    args = parser.parse_args()
    try:
        args.reference = driver_path(args.reference)
        args.rust = driver_path(args.rust)
        args.report = report_path(args.report)
        character_options(args.locale, args.lcid)
    except ValueError as error:
        parser.error(str(error))
    if args.random_cases < 0:
        parser.error("random-cases must be nonnegative")
    selection = {"locale": args.locale, "lcid": args.lcid}
    cases = list(corpus(args.seed, args.random_cases))
    expected = run_driver(args.reference, cases, **selection)
    actual = run_driver(args.rust, cases, **selection)
    mismatches = []
    for index, (case, reference, rust) in enumerate(zip(cases, expected, actual)):
        if reference != rust:
            op, pattern, data, template = case
            mismatches.append({"case": index, "operation": op, "pattern_hex": pattern.hex(), "input_hex": data.hex(), "template_hex": template.hex(), "reference": reference, "rust": rust})
    environment = driver_environment(args.reference, **selection)
    rust_environment = driver_environment(args.rust, **selection)
    report = {"schema": 1, "kind": "functional-compatibility", "seed": args.seed, "random_cases": args.random_cases,
              "cases": len(cases), "mismatch_count": len(mismatches), "mismatches": mismatches,
              "reference_environment": environment, "host": platform.platform(),
              "rust_environment": rust_environment,
              "executables_sha256": {"reference": hashlib.sha256(args.reference.read_bytes()).hexdigest(), "rust": hashlib.sha256(args.rust.read_bytes()).hexdigest()},
              "runner_image": os.environ.get("ImageVersion"), "performance_acceptance": "pending designated machine"}
    write_json_report(args.report, report)
    print(f"{len(cases)} comparisons; {len(mismatches)} mismatches; report {args.report}")
    for mismatch in mismatches[:12]:
        print(json.dumps(mismatch))
    return bool(mismatches)


if __name__ == "__main__":
    raise SystemExit(main())
