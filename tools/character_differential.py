#!/usr/bin/env python3
"""Byte character-rule and default-formatter comparisons against local C++.

Both executables use the four-field TSV/JSON protocol from reference.md. This
script is an explicit test tool; Cargo library consumers never invoke it.
"""

import argparse
import json
import os
from pathlib import Path
import random
import subprocess
import sys


CLASSES = (
    b"alnum", b"alpha", b"blank", b"cntrl", b"d", b"digit", b"graph",
    b"h", b"l", b"lower", b"print", b"punct", b"s", b"space", b"u",
    b"unicode", b"upper", b"v", b"w", b"word", b"xdigit",
)
SEED = 189


def cases(random_templates):
    corpus = bytes(range(256))
    result = []

    def add(label, operation, pattern, data=corpus, template=b""):
        result.append({"label": label, "operation": operation,
                       "pattern_hex": pattern.hex(), "input_hex": data.hex(),
                       "template_hex": template.hex()})

    for name in CLASSES:
        for prefix in (b"", b"(?i)"):
            for negate in (b"", b"^"):
                pattern = prefix + b"([[:" + negate + name + b":]])"
                add("class:" + pattern.decode(), "find_all", pattern)
    for byte in range(256):
        add(f"case-fold:{byte:02x}", "find_all", f"(?i:\\x{byte:02x})".encode())
    for mode in (b"L", b"U", b"l", b"u"):
        add("case-format:" + mode.decode(), "replace", b"([\\s\\S])",
            template=b"\\" + mode + b"$1")
    for escape in (b"b", b"B", b"<", b">"):
        add("word-boundary:" + escape.decode(), "find_all", b"\\" + escape)
    for name in (b"NUL", b"DEL", b"tab", b"newline", b"a", b"A", b"\xff",
                 b"ch", b"ae", b"ll", b"ss", b"nj", b"dz", b"lj"):
        add("collating:" + name.hex(), "find_all", b"[[." + name + b".]]",
            corpus + b"chCHaeAEllLLssSSnjNJdzDZljLJ")
    for pattern in (b"[[=a=]]", b"[[=A=]]", b"[[=\x00=]]", b"[[=\xff=]]",
                    b"[[.a.]-[.z.]]", b"(?i)[[.a.]-[.z.]]", b"[^[.ch.]]",
                    b"[[.missing.]]", b"[[=missing=]]", b"[[:missing:]]"):
        add("collation-pattern:" + pattern.hex(), "find_all", pattern)

    templates = [b"\\" + bytes([byte]) for byte in range(256)]
    templates += [b"$" + bytes([byte]) for byte in range(256)]
    templates += [
        b"$1", b"$12", b"$123", b"$-1", b"$+1", b"$ 1", b"${1}",
        b"$4294967296", b"$9223372036854775807", b"${^N}", b"${^^N}",
        b"${MATCH}", b"$MATCHabc", b"${LAST_SUBMATCH_RESULT}",
        b"$+{letter}", b"$+{letter", b"\\x-1", b"\\x{-1}",
        b"\\x{999999999999999999999999999}", b"\\x{ff}", b"\\01234",
        b"\\U\\lAB\\E-\\L\\uAB\\E", b"\\l\\uABC", b"\\u$3A",
        b"$`/$&/$'", b"$LAST_PAREN_MATCH/$LAST_SUBMATCH_RESULT/$^N",
    ]
    parts = [b"x", b"\0", b"\xff", b"$1", b"$3", b"${1}", b"$+",
             b"$+{letter}", b"$+{missing", b"${^N}", b"$^N",
             b"$LAST_SUBMATCH_RESULT", b"$MATCH", b"$PREMATCH", b"$POSTMATCH",
             b"$`", b"$'", b"$&", b"$$", b"\\u", b"\\L", b"\\E", b"\\U",
             b"\\l", b"\\x", b"\\x{", b"\\x{-1}", b"\\x41", b"\\01234",
             b"1}", b"-1}", b"ff}", b"0x42}", b"9999999999999999999999999}"]
    rng = random.Random(SEED)
    templates += [b"".join(rng.choices(parts, k=rng.randrange(1, 8)))
                  for _ in range(random_templates)]
    for index, template in enumerate(templates):
        add(f"formatter:{index}", "replace", b"(?<letter>A)(b)(c)?", b"preAbpost", template)
    # Repeated matches and adjacent empty matches test formatter prefix ranges
    # and per-match case state independently of the above fixed-match corpus.
    for pattern in (b"(a)", b"(a*)", b"(?<x>a)|(b)", b"((a)b)"):
        for template in (b"$`:$&:$'", b"$+/$LAST_SUBMATCH_RESULT/$^N",
                         b"\\u$1\\L$2", b"$+{x}", b"\0\xff$0"):
            add("enumerated-format:" + pattern.hex() + ":" + template.hex(),
                "replace", pattern, b"xabaa", template)
    return result


def run(executable, arguments, requests):
    wire = "".join("\t".join((case["operation"], case["pattern_hex"],
                             case["input_hex"], case["template_hex"])) + "\n"
                   for case in requests)
    completed = subprocess.run([str(executable), *arguments], input=wire, text=True,
                               capture_output=True, timeout=90)
    if completed.returncode:
        raise RuntimeError(f"{executable} exited {completed.returncode}: {completed.stderr}")
    lines = completed.stdout.splitlines()
    if len(lines) != len(requests):
        raise RuntimeError(f"{executable} returned {len(lines)} responses for {len(requests)} requests")
    return [json.loads(line) for line in lines]


def minimize_template(reference, rust, arguments, case):
    """Retain a byte-minimal template for a reproducible formatting difference."""
    value = bytes.fromhex(case["template_hex"])
    changed = True
    while changed:
        changed = False
        for index in range(len(value)):
            candidate = value[:index] + value[index + 1:]
            request = {**case, "template_hex": candidate.hex()}
            if run(reference, arguments, [request]) != run(rust, arguments, [request]):
                value = candidate
                changed = True
                break
    return {**case, "template_hex": value.hex()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=Path("target/reference/reference" + (".exe" if os.name == "nt" else "")))
    parser.add_argument("--rust", type=Path, required=True)
    parser.add_argument("--report", "--output", dest="output", type=Path, default=Path("reports/characters.json"))
    parser.add_argument("--locale", action="append", default=[])
    parser.add_argument("--lcid", action="append", default=[])
    parser.add_argument("--default-only", action="store_true")
    parser.add_argument("--random-templates", type=int, default=2000)
    args = parser.parse_args()
    reference = args.reference.resolve()
    rust = args.rust.resolve()
    scenarios = [("default", [])]
    if not args.default_only:
        locales = args.locale or ([] if os.name == "nt" else
                                  ["C.UTF-8", "en_US.UTF-8"] if sys.platform.startswith("linux") else
                                  ["en_US.UTF-8"])
        lcids = args.lcid or (["1041", "2052"] if os.name == "nt" else [])
        scenarios += [("locale:" + name, ["--locale", name]) for name in locales]
        scenarios += [("lcid:" + lcid, ["--lcid", lcid]) for lcid in lcids]
    requests = cases(args.random_templates)
    report = {"seed": SEED, "random_templates": args.random_templates,
              "cases_per_scenario": len(requests), "scenarios": [],
              "performance_acceptance": "not measured by functional tests"}
    for label, arguments in scenarios:
        scenario = {"name": label, "arguments": arguments, "mismatches": []}
        try:
            expected = run(reference, arguments, requests)
            actual = run(rust, arguments, requests)
            for request, cpp, port in zip(requests, expected, actual):
                if cpp != port:
                    difference = {"request": request, "reference": cpp, "rust": port}
                    if request["operation"] == "replace" and len(scenario["mismatches"]) < 3:
                        difference["minimized_request"] = minimize_template(reference, rust, arguments, request)
                    scenario["mismatches"].append(difference)
            scenario["status"] = "failed" if scenario["mismatches"] else "passed"
        except (RuntimeError, ValueError, OSError, subprocess.TimeoutExpired) as error:
            scenario["status"] = "environment-or-protocol-failure"
            scenario["error"] = str(error)
        report["scenarios"].append(scenario)
        print(f"{label}: {scenario['status']} ({len(scenario['mismatches'])} differences)", flush=True)
    report["status"] = "passed" if all(s["status"] == "passed" for s in report["scenarios"]) else "failed"
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
