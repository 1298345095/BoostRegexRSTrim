#!/usr/bin/env python3
"""Prepare or validate measurements; actual runs require a designated machine."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys

from tool_paths import driver_path, project_file, report_path, write_json_report


ROOT = Path(__file__).resolve().parents[1]
SUFFIX = ".exe" if os.name == "nt" else ""


def workload(name, operation, pattern, data=b"", replacement=b""):
    return {"name": name, "operation": operation, "pattern_hex": pattern.hex(),
            "input_hex": data.hex(), "template_hex": replacement.hex()}


WORKLOADS = [
    workload("compile-literal-bytes", "compile", b"literal\0\xffpattern"),
    workload("compile-captures-and-alternatives", "compile", b"(?<word>[a-z]+)(?: ([0-9]+)| (other))?"),
    workload("search-literal", "search", b"needle", b"haystack " * 1000 + b"needle"),
    workload("search-capture-backreference", "search", b"(?<word>[a-z]+) +\\g{word}", b"one two three three four"),
    workload("search-lookaround", "search", b"(?<=ab)c(?=d)", b"xabcd " * 100),
    workload("full-recursive", "full", rb"\((?:[^()]|(?R))*\)", b"((one)(two(three)))"),
    workload("full-backtracking", "full", b"(ab|a)+b", b"ab" * 64 + b"b"),
    workload("replace-byte-captures", "replace", b"([a-z]+)", b"one two\0three\xfffour " * 50, b"<$1>\0"),
    workload("replace-case", "replace", b"(?<word>[a-z]+)", b"one two three " * 50, rb"\U$+{word}\E"),
]


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def validate_workload(item, names):
    if not isinstance(item, dict) or set(item) != {"name", "operation", "pattern_hex", "input_hex", "template_hex"}:
        raise ValueError("A workload needs exactly name, operation, pattern_hex, input_hex and template_hex")
    if not isinstance(item["name"], str) or not item["name"] or item["name"] in names:
        raise ValueError("Workload names must be unique nonempty strings")
    names.add(item["name"])
    if item["operation"] not in ("compile", "search", "full", "replace"):
        raise ValueError("Unsupported measurement operation")
    for key in ("pattern_hex", "input_hex", "template_hex"):
        validate_hex(item[key])


def validate_hex(value):
    if not isinstance(value, str) or len(value) % 2 or any(c not in "0123456789abcdefABCDEF" for c in value):
        raise ValueError("Workload byte fields must use contiguous, even-length hexadecimal")


def load_workloads(path):
    values = json.loads(project_file(path).read_text(encoding="utf-8")) if path else WORKLOADS
    if not isinstance(values, list) or not values:
        raise ValueError("Workloads must be a nonempty JSON array")
    names = set()
    for item in values:
        validate_workload(item, names)
    return values


def command(executable, item, iterations, measured):
    return [str(executable), "--benchmark" if measured else "--benchmark-check",
            item["operation"], item["pattern_hex"], item["input_hex"],
            item["template_hex"], str(iterations)]


def invoke(argv, timeout):
    result = subprocess.run(argv, cwd=ROOT, text=True, capture_output=True,
                            check=True, timeout=timeout)
    value = json.loads(result.stdout)
    if not value.get("ok"):
        raise ValueError("Driver rejected workload: " + json.dumps(value, ensure_ascii=True))
    return value


def tool_version(argv):
    try:
        result = subprocess.run(argv, text=True, capture_output=True, timeout=20, check=False)
    except FileNotFoundError:
        return {"command": argv, "available": False}
    return {"command": argv, "status": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr}


def parse_arguments():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--dry-run", action="store_true", help="Default: emit the plan without starting either engine")
    modes.add_argument("--check", action="store_true", help="Validate checksums and protocol without clocks or memory measurements")
    modes.add_argument("--run", action="store_true", help="Collect measurements on a user-designated machine")
    parser.add_argument("--designated-machine", help="User-provided identifier of the approved measurement machine")
    parser.add_argument("--machine-notes", default="",
                        help="Record power mode, background load, virtualization, and other run conditions")
    parser.add_argument("--reference", type=Path, default=ROOT / "target/reference" / ("reference-baseline" + SUFFIX))
    parser.add_argument("--rust", type=Path, default=ROOT / "target/release" / ("regex_driver" + SUFFIX))
    parser.add_argument("--reference-build-info", type=Path)
    parser.add_argument("--rust-build-command", default="cargo build --release --features measurements --bin regex_driver")
    parser.add_argument("--workloads", type=Path)
    parser.add_argument("--iterations", type=int, default=1000)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--timeout", type=int, default=300)
    parser.add_argument("--report", type=Path, default=ROOT / "reports/benchmark.json")
    args = parser.parse_args()
    if args.iterations < 1 or args.samples < 1 or args.timeout < 1:
        parser.error("iterations, samples and timeout must be positive")
    if args.run and (not args.designated_machine or not args.designated_machine.strip()):
        parser.error("ADR-0004 requires --designated-machine NAME before measurements; no measurement was started")
    try:
        # Dry runs may name binaries that have not been built yet. All modes
        # still confine executable selection to this checkout's build artifacts.
        args.reference = driver_path(args.reference, must_exist=False)
        args.rust = driver_path(args.rust, must_exist=False)
        args.report = report_path(args.report)
        if args.workloads:
            args.workloads = project_file(args.workloads)
        if args.reference_build_info:
            args.reference_build_info = project_file(args.reference_build_info)
    except (ValueError, OSError) as error:
        parser.error(str(error))
    return parser, args


def load_build_info(args):
    path = project_file(args.reference_build_info or args.reference.parent / "build-info-baseline.json",
                        must_exist=False)
    if not path.is_file():
        return None
    metadata = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(metadata, dict):
        raise ValueError("Reference build information must be a JSON object")
    return metadata


def prepare_report(args, metadata, workloads):
    reference, rust = args.reference, args.rust
    mode = "measured" if args.run else "plumbing-check" if args.check else "dry-run"
    return {
        "schema_version": 1, "mode": mode,
        "performance_acceptance": "pending_user_baselines_and_thresholds",
        "designated_machine": args.designated_machine,
        "machine_notes": args.machine_notes,
        "machine": {"system": platform.platform(), "architecture": platform.machine(),
                    "libc": platform.libc_ver(), "python": sys.version,
                    "logical_cpus": os.cpu_count(), "processor": platform.processor()},
        "environment": {name: os.environ.get(name) for name in
                        ("LANG", "LC_ALL", "LC_CTYPE", "LC_COLLATE", "RUSTFLAGS", "CXX")},
        "engines": {
            "reference": {"path": str(reference), "sha256": digest(reference), "build_info": metadata},
            "rust": {"path": str(rust), "sha256": digest(rust),
                     "declared_build_command": args.rust_build_command,
                     "cargo_lock_sha256": digest(ROOT / "Cargo.lock")},
        },
        "sources_sha256": {
            str(path.relative_to(ROOT)): digest(path)
            for path in sorted([ROOT / "Cargo.toml", ROOT / "tools/reference.cpp",
                                ROOT / "tools/benchmark.py", *(ROOT / "src").rglob("*.rs")])
        },
        "iterations_per_sample": args.iterations,
        "samples_per_workload": args.samples,
        "workloads": workloads,
        "planned_build_commands": [
            ["python", "tools/build_reference.py", "--baseline", "--profile", "release"],
            ["cargo", "build", "--release", "--features", "measurements", "--bin", "regex_driver"],
        ],
        "planned_measurement_commands": [
            {"workload": item["name"], "reference": command(reference, item, args.iterations, True),
             "rust": command(rust, item, args.iterations, True)} for item in workloads
        ],
        "checks": [], "samples": [], "summary": [],
        "method": {
            "clock": "Each engine uses its monotonic clock around the operation loop only",
            "compile": "Repeated full compilation; pattern decoding and process startup excluded",
            "execute": "Compile once before timer; capture/output allocation and consumption included",
            "memory": "Process peak RSS/working set in bytes; includes startup, input and compilation",
            "order": "Alternate reference-first and Rust-first across repeated samples",
            "comparison": "Same workload bytes and iterations on one designated machine; no cross-machine absolute comparison",
        },
    }


def validate_release_build(args, metadata):
    if not metadata or metadata.get("profile") != "release" or metadata.get("raw_error_observer") is not None:
        raise ValueError("Measurements require the recorded uninstrumented C++ --baseline --profile release build")
    if metadata.get("executable_sha256") != digest(args.reference):
        raise ValueError("The selected C++ executable must match its recorded build SHA-256")
    build_command = args.rust_build_command.split()
    if not build_command or Path(build_command[0]).name != "cargo" or "--release" not in build_command:
        raise ValueError("Record a comparable Rust release build command with --rust-build-command")


def record_environments(args, report):
    report["toolchains"] = {"rustc": tool_version(["rustc", "--version", "--verbose"]),
                            "cargo": tool_version(["cargo", "--version"])}
    for name, executable in (("reference", args.reference), ("rust", args.rust)):
        report[name + "_environment"] = json.loads(
            subprocess.check_output([str(executable), "--environment"], text=True, timeout=args.timeout))
    rust = report["rust_environment"]
    if args.run and (rust.get("debug_assertions") is not False or
                     rust.get("measurements") is not True or rust.get("diagnostics") is not False):
        raise ValueError("Measurements require a Rust --release --features measurements build without diagnostics")


def check_workload(args, item):
    baseline = invoke(command(args.reference, item, 2, False), args.timeout)
    implementation = invoke(command(args.rust, item, 2, False), args.timeout)
    if baseline != implementation or baseline.get("measured") is not False:
        raise ValueError("Plumbing/checksum mismatch for " + item["name"] + ": " +
                         json.dumps({"reference": baseline, "rust": implementation}))
    if any(key in baseline for key in ("elapsed_ns", "peak_rss_bytes")):
        raise ValueError("A plumbing check must not collect timing or memory measurements")
    return {"workload": item["name"], "result": baseline}


def validate_sample(response, item, iterations):
    if (response.get("measured") is not True or
            not isinstance(response.get("elapsed_ns"), int) or response["elapsed_ns"] < 0 or
            response.get("operation") != item["operation"] or response.get("iterations") != iterations):
        raise ValueError("Missing engine timing result")
    if not isinstance(response.get("peak_rss_bytes"), int) or response["peak_rss_bytes"] < 1:
        raise ValueError("Missing process peak-memory result")


def sample_workload(args, item, report):
    for sample in range(args.samples):
        order = [("reference", args.reference), ("rust", args.rust)]
        if sample % 2:
            order.reverse()
        responses = {}
        for engine, executable in order:
            response = invoke(command(executable, item, args.iterations, True), args.timeout)
            validate_sample(response, item, args.iterations)
            report["samples"].append({"workload": item["name"], "sample": sample,
                                      "engine": engine, **response})
            responses[engine] = response
        if responses["reference"]["checksum"] != responses["rust"]["checksum"]:
            raise ValueError("Measurement output checksums diverged for " + item["name"])


def summarize_samples(args, report):
    for item in report["workloads"]:
        for engine in ("reference", "rust"):
            samples = [s for s in report["samples"] if s["workload"] == item["name"] and s["engine"] == engine]
            elapsed = [s["elapsed_ns"] / args.iterations for s in samples]
            report["summary"].append({"workload": item["name"], "engine": engine,
                                      "ns_per_iteration_median": statistics.median(elapsed),
                                      "ns_per_iteration_min": min(elapsed),
                                      "ns_per_iteration_max": max(elapsed),
                                      "peak_rss_bytes_max": max(s["peak_rss_bytes"] for s in samples)})


def validate_and_run(args, report):
    if args.run:
        validate_release_build(args, report["engines"]["reference"]["build_info"])
    if not (args.run or args.check):
        report["status"] = "plan_only"
        return
    if not args.reference.is_file() or not args.rust.is_file():
        raise ValueError("Build both explicitly selected engine executables before checking or measuring")
    record_environments(args, report)
    for item in report["workloads"]:
        report["checks"].append(check_workload(args, item))
        if args.run:
            sample_workload(args, item, report)
    if args.run:
        summarize_samples(args, report)
    report["status"] = "passed"


def main():
    parser, args = parse_arguments()
    try:
        report = prepare_report(args, load_build_info(args), load_workloads(args.workloads))
    except (ValueError, OSError) as error:
        parser.error(str(error))
    try:
        validate_and_run(args, report)
    except (ValueError, OSError, subprocess.SubprocessError) as error:
        report["status"] = "failed"
        report["error"] = str(error)
    output = args.report
    write_json_report(output, report)
    print(f"{report['mode']}: {report['status']}; report {output}")
    if report["status"] == "failed":
        print(report["error"], file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
