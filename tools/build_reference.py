#!/usr/bin/env python3
"""Build the opt-in compatibility reference; never called by Cargo."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]


def observed_error_header(destination):
    """Add length-bearing observation, keeping the original C++ sources intact."""
    source = ROOT / "boost/regex/pattern_except.hpp"
    original = source.read_text(encoding="utf-8")
    replacements = [
        ("m_error_code(err), m_position(pos) {}",
         "m_error_code(err), m_position(pos), m_raw_message(s) {}"),
        ("m_error_code(err), m_position(0) {}",
         "m_error_code(err), m_position(0), m_raw_message(\n"
         "            ::boost::BOOST_REGEX_DETAIL_NS::get_default_error_string(err)) {}"),
        ("  void raise() const { throw *this; }",
         "  void raise() const { throw *this; }\n"
         "  const std::string &raw_message() const noexcept { return m_raw_message; }"),
        ("  std::ptrdiff_t m_position;",
         "  std::ptrdiff_t m_position;\n  std::string m_raw_message;"),
    ]
    modified = original
    for before, after in replacements:
        if modified.count(before) != 1:
            raise RuntimeError("Raw-error observer no longer matches the retained source")
        modified = modified.replace(before, after, 1)
    header = destination / "boost/regex/pattern_except.hpp"
    header.parent.mkdir(parents=True, exist_ok=True)
    header.write_text(modified, encoding="utf-8")
    return {
        "original_sha256": hashlib.sha256(original.encode()).hexdigest(),
        "observed_sha256": hashlib.sha256(modified.encode()).hexdigest(),
        "replacements": len(replacements),
    }


def capture(command):
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                            errors="backslashreplace", check=False)
    return {"command": command, "status": result.returncode,
            "stdout": result.stdout, "stderr": result.stderr}


def observed_accounting_header(destination):
    """Expose existing counters through const accessors; no members or writes."""
    source = ROOT / "boost/regex/v5/perl_matcher.hpp"
    original = source.read_text(encoding="utf-8")
    anchor = "  bool match();\n  bool find();\n"
    accessors = anchor + (
        "  std::ptrdiff_t reference_state_count() const { return state_count; }\n"
        "  std::ptrdiff_t reference_max_state_count() const { return max_state_count; }\n"
        "  unsigned reference_used_block_count() const { return used_block_count; }\n"
    )
    if original.count(anchor) != 1:
        raise RuntimeError("Accounting observer no longer matches the retained source")
    modified = original.replace(anchor, accessors, 1)
    header = destination / "boost/regex/v5/perl_matcher.hpp"
    header.parent.mkdir(parents=True, exist_ok=True)
    header.write_text(modified, encoding="utf-8")
    return {"original_sha256": hashlib.sha256(original.encode()).hexdigest(),
            "observed_sha256": hashlib.sha256(modified.encode()).hexdigest(),
            "replacements": 1}


def reference_command(args, compiler, msvc, include, source, executable, directory, suffix, observation):
    """Keep platform-specific build flags separate from build orchestration."""
    if msvc:
        flags = ["/nologo", "/std:c++17", "/O2", "/EHsc", "/W4", "/Zc:__cplusplus"]
        if args.profile == "release":
            flags += ["/DNDEBUG"]
        flags += ["/I" + str(path) for path in include]
        if observation:
            flags += ["/DBOOST_REGEX_REFERENCE_RAW_OBSERVER=1", "/DBOOST_REGEX_REFERENCE_ACCOUNTING_OBSERVER=1"]
        command = compiler + flags + [source, "/Fe:" + str(executable),
                                      "/Fo:" + str(directory / ("reference" + suffix + ".obj"))]
    else:
        flags = ["-std=c++17", "-O3" if args.profile == "release" else "-O2",
                 "-Wall", "-Wextra", "-pedantic", "-pthread"]
        if args.profile == "release":
            flags += ["-DNDEBUG"]
        flags += ["-I" + str(path) for path in include]
        if observation:
            flags += ["-DBOOST_REGEX_REFERENCE_RAW_OBSERVER=1", "-DBOOST_REGEX_REFERENCE_ACCOUNTING_OBSERVER=1"]
        command = compiler + flags + [source, "-o", str(executable)]
    return command

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cxx", default=os.environ.get("CXX", "cl" if os.name == "nt" else "c++"))
    parser.add_argument("--baseline", action="store_true",
                        help="Build without the length-bearing raw-error observer")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--profile", choices=("test", "release"), default="test",
                        help="Release disables assertions and selects full compiler optimization")
    args = parser.parse_args()
    directory = ROOT / "target/reference"
    directory.mkdir(parents=True, exist_ok=True)
    suffix = "-baseline" if args.baseline else ""
    executable = args.output or directory / ("reference" + suffix + (".exe" if os.name == "nt" else ""))
    executable = executable.resolve()
    executable.parent.mkdir(parents=True, exist_ok=True)
    compiler = shlex.split(args.cxx, posix=os.name != "nt")
    msvc = Path(compiler[0]).stem.lower() in ("cl", "clang-cl")
    include = [ROOT]
    observation = None
    accounting = None
    if not args.baseline:
        overlay = directory / "raw-error-observer"
        observation = observed_error_header(overlay)
        accounting = observed_accounting_header(overlay)
        include.insert(0, overlay)
    source = str(ROOT / "tools/reference.cpp")
    command = reference_command(args, compiler, msvc, include, source, executable,
                                directory, suffix, observation)
    print("Compiling " + str(executable), flush=True)
    subprocess.run(command, cwd=ROOT, check=True)
    metadata = {
        "profile": args.profile,
        "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
        "driver_sha256": hashlib.sha256(Path(source).read_bytes()).hexdigest(),
        "build_command": command,
        "compiler": capture(compiler if msvc else compiler + ["--version"]),
        "system": platform.platform(),
        "machine": platform.machine(),
        "libc": platform.libc_ver(),
        "python": sys.version,
        "environment": {name: os.environ.get(name) for name in
                        ("LANG", "LC_ALL", "LC_CTYPE", "LC_COLLATE", "ImageOS",
                         "ImageVersion", "RUNNER_OS", "RUNNER_ARCH")},
        "reference": json.loads(subprocess.check_output(
            [str(executable), "--environment"], cwd=ROOT, text=True)),
        "raw_error_observer": observation,
        "accounting_observer": accounting,
        "retained_sources_sha256": {
            str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted((ROOT / "boost").rglob("*.hpp"))
        },
    }
    report = executable.parent / ("build-info" + suffix + ".json")
    report.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    print("Environment recorded in " + str(report), flush=True)


if __name__ == "__main__":
    main()
