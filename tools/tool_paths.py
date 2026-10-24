"""Filesystem boundaries for the repository's local validation tools.

Inputs belong to the checkout, engine executables are named build artifacts in
``target/``, and generated JSON reports belong to ``reports/``. Resolve symlinks
before checking containment so CLI arguments cannot redirect these operations
outside their intended directories.
"""

import os
import json
from pathlib import Path
import re
import tempfile


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def _bounded_file(value, directory, *, must_exist):
    try:
        boundary = directory.resolve()
        path = Path(value).resolve(strict=must_exist)
    except (OSError, RuntimeError, ValueError) as error:
        raise ValueError(f"Invalid tool path: {value}") from error
    if not boundary.is_relative_to(PROJECT_ROOT) or not path.is_relative_to(boundary):
        raise ValueError(f"Tool path must stay inside {directory}: {value}")
    if path.exists() and not path.is_file():
        raise ValueError(f"Tool path must name a regular file: {value}")
    return path


def project_file(value, *, must_exist=True):
    """Select a checkout-local workload or build metadata file."""
    return _bounded_file(value, PROJECT_ROOT, must_exist=must_exist)


def driver_path(value, *, must_exist=True):
    """Select an engine built in target/, without PATH or shell resolution."""
    path = _bounded_file(value, PROJECT_ROOT / "target", must_exist=must_exist)
    suffix = ".exe" if os.name == "nt" else ""
    names = {name + suffix for name in ("reference", "reference-baseline", "regex_driver")}
    if path.name not in names:
        raise ValueError("Driver must be a target/ build named reference, reference-baseline or regex_driver")
    return path


def character_options(locale=None, lcid=None):
    """Build only native character-selection options, with values kept separate."""
    options = []
    if locale is not None:
        if (not isinstance(locale, str) or not locale or locale.startswith("-")
                or any(ord(byte) < 32 or ord(byte) == 127 for byte in locale)):
            raise ValueError("Locale must be a nonempty native name, not an option or control sequence")
        # Native composite locale names and non-ASCII names remain valid data;
        # they are never interpolated into a shell command.
        options.extend(["--locale", locale])
    if lcid is not None:
        if not isinstance(lcid, str) or re.fullmatch(r"(?:[0-9]+|0[xX][0-9a-fA-F]+)", lcid) is None:
            raise ValueError("LCID must be a positive 32-bit decimal or hexadecimal number")
        value = int(lcid, 16 if lcid.lower().startswith("0x") else 10)
        if not 0 < value <= 0xFFFFFFFF:
            raise ValueError("LCID must be a positive 32-bit decimal or hexadecimal number")
        # The C++ CLI parses base zero while Rust accepts decimal/lowercase hex.
        # A decimal spelling gives both drivers the validated numeric identity.
        options.extend(["--lcid", str(value)])
    return options


def character_command(executable, *, locale=None, lcid=None, environment=False):
    """Construct the complete fixed-shape invocation at the process boundary."""
    command = [str(driver_path(executable)), *character_options(locale, lcid)]
    if environment:
        command.append("--environment")
    return command


def report_path(value):
    """Select a generated JSON report without creating directories or files."""
    path = _bounded_file(value, PROJECT_ROOT / "reports", must_exist=False)
    if path.suffix.lower() != ".json":
        raise ValueError("Generated reports must use a .json filename inside reports/")
    return path


def write_json_report(value, report):
    """Publish complete JSON atomically without following an existing hard link."""
    destination = report_path(value)
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8",
                                         prefix="report-", suffix=".tmp",
                                         dir=destination.parent, delete=False) as stream:
            temporary = Path(stream.name)
            json.dump(report, stream, indent=2)
            stream.write("\n")
        temporary.replace(report_path(destination))
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
