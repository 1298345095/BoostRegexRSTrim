#!/usr/bin/env python3
"""Validate the driver protocol before treating it as a compatibility oracle."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import unittest


ROOT = Path(__file__).resolve().parents[1]
EXE = ROOT / "target/reference" / ("reference.exe" if os.name == "nt" else "reference")
BASELINE = None
OPTIONS = []


def request(operation, pattern=b"", data=b"", replacement=b""):
    return "\t".join((operation, pattern.hex(), data.hex(), replacement.hex())) + "\n"


def run(text, executable=None):
    process = subprocess.run([str(executable or EXE), *OPTIONS], input=text,
                             text=True, capture_output=True, timeout=30, check=True)
    return [json.loads(line) for line in process.stdout.splitlines()]


def option(*arguments, executable=None):
    process = subprocess.run([str(executable or EXE), *OPTIONS, *arguments],
                             text=True, capture_output=True, timeout=30, check=True)
    return json.loads(process.stdout)


class ReferenceTests(unittest.TestCase):
    def test_compile_and_capture_count(self):
        self.assertEqual(run(request("compile", b"(a)(?:b)(c)")),
                         [{"ok": True, "mark_count": 2}])

    def test_byte_ranges_preserve_nul_and_non_utf8(self):
        pattern = b"\x00\xff\x80"
        result = run(request("search", pattern, b"x" + pattern + b"y"))[0]
        self.assertEqual(result, {"ok": True, "match": {
            "span": [1, 4], "captures": [[1, 4]]}})

    def test_entire_byte_alphabet(self):
        cases = []
        for byte in range(256):
            value = bytes([byte])
            # Quoting uses literal bytes, avoiding signed-char hex semantics.
            cases.append(request("search", b"\\Q" + value + b"\\E", value))
        for byte, response in enumerate(run("".join(cases))):
            with self.subTest(byte=byte):
                self.assertEqual(response, {"ok": True, "match": {
                    "span": [0, 1], "captures": [[0, 1]]}})

    def test_unmatched_and_empty_captures_are_distinct(self):
        self.assertEqual(run(request("search", b"(a)?()"))[0],
                         {"ok": True, "match": {"span": [0, 0],
                          "captures": [[0, 0], None, [0, 0]]}})

    def test_full_match_and_absent_match(self):
        result = run(request("full", b"a", b"xa") + request("full", b"a", b"a"))
        self.assertEqual(result[0], {"ok": True, "match": None})
        self.assertEqual(result[1]["match"]["span"], [0, 1])

    def test_empty_match_advancement(self):
        result = run(request("find_all", b"|a", b"a"))[0]
        self.assertEqual([entry["span"] for entry in result["matches"]],
                         [[0, 0], [0, 1], [1, 1]])

    def test_empty_inputs_and_replacement(self):
        result = run(request("full") + request("replace", b"a", b"aba", b""))
        self.assertEqual(result[0]["match"]["span"], [0, 0])
        self.assertEqual(result[1], {"ok": True, "output_hex": "62"})

    def test_byte_replacement(self):
        result = run(request("replace", b"(a)", b"xaay", b"\xff\x00$1"))[0]
        self.assertEqual(bytes.fromhex(result["output_hex"]), b"x\xff\x00a\xff\x00ay")

    def test_complete_raw_error_including_nul(self):
        result = run(request("compile", b"\x00("))[0]
        self.assertEqual((result["ok"], result["phase"], result["code"], result["position"]),
                         (False, "compile", 8, 2))
        self.assertEqual(bytes.fromhex(result["message_hex"]),
                         b"Unmatched marking parenthesis ( or \\(.  The error occurred "
                         b"while parsing the regular expression: '\x00(>>>HERE>>>'.")

    def test_non_utf8_error(self):
        result = run(request("compile", b"\xff["))[0]
        self.assertEqual((result["phase"], result["code"], result["position"]),
                         ("compile", 7, 2))
        self.assertTrue(bytes.fromhex(result["message_hex"]).endswith(b"'\xff[>>>HERE>>>'."))

    def test_execution_error_and_recovery(self):
        result = run(request("search", b"(a+)+$", b"a" * 30 + b"b") +
                     request("full", b"a", b"a"))
        self.assertEqual((result[0]["ok"], result[0]["phase"], result[0]["code"]),
                         (False, "execute", 18))
        self.assertEqual(result[1]["match"]["span"], [0, 1])

    def test_protocol_errors_and_recovery(self):
        result = run("compile\t0\t\t\nsearch\tgg\t\t\nwrong\t\t\t\n" + request("compile"))
        self.assertTrue(all(entry["phase"] == "protocol" for entry in result[:3]))
        self.assertEqual(result[3], {"ok": True, "mark_count": 0})

    def test_compiled_program_links_and_observer(self):
        for pattern in (b"(a){2,3}|[b-c]", b"(?<a>a(?&a)?b)", b"(?i:(?=a)a)",
                        b"[[.ch.]\0-\x01[=\0=]]"):
            with self.subTest(pattern=pattern):
                program = option("--program", pattern.hex())
                self.assertTrue(program["ok"])
                self.assertEqual(program["pattern_len"], len(pattern))
                self.assertEqual(len(bytes.fromhex(program["start_map_hex"])), 256)
                ids = {state["id"] for state in program["states"]}
                self.assertEqual(len(ids), len(program["states"]))
                for state in program["states"]:
                    for link in ("next", "alt"):
                        if state.get(link) is not None:
                            self.assertIn(state[link], ids)
                    if "map_hex" in state:
                        self.assertEqual(len(bytes.fromhex(state["map_hex"])), 256)
                if BASELINE is not None:
                    self.assertEqual(program, option("--program", pattern.hex(),
                                                     executable=BASELINE))

    def test_compiled_program_retains_full_raw_error(self):
        self.assertEqual(option("--program", b"\0(".hex()),
                         run(request("compile", b"\0("))[0])

    def test_compiled_program_retains_nul_long_set_keys(self):
        program = option("--program", b"[[.ch.]\0-\x01[=\0=]]".hex())
        long_set = next(state for state in program["states"] if state["kind"] == 14)
        self.assertEqual(long_set["singles_hex"], [b"ch".hex()])
        self.assertEqual(long_set["range_endpoints_hex"], ["00", "01"])
        # A primary collation key can contain a literal NUL; it still has a
        # separate string terminator in the retained compiled representation.
        self.assertTrue(long_set["equivalence_keys_hex"])
        self.assertTrue(all(bytes.fromhex(key) for key in long_set["equivalence_keys_hex"]))

    def test_accounting_preserves_match_and_original_counters(self):
        result = option("--accounting", "full", b"ab".hex(), b"ab".hex())
        counters = result.pop("accounting")
        self.assertEqual(result, run(request("full", b"ab", b"ab"))[0])
        self.assertEqual(counters, {"state_count": 3, "max_state_count": 100008,
                                    "blocks_remaining_after_cleanup": 1024})

    def test_accounting_retains_failure_and_cleanup(self):
        pattern, data = b"(a+)+$", b"a" * 30 + b"b"
        result = option("--accounting", "search", pattern.hex(), data.hex())
        counters = result.pop("accounting")
        self.assertEqual(result, run(request("search", pattern, data))[0])
        self.assertGreater(counters["state_count"], counters["max_state_count"])
        self.assertEqual(counters["blocks_remaining_after_cleanup"], 1024)

    def test_accounting_stream_and_large_input(self):
        data = b"a" * 70000
        result = run(request("accounting_full", b"a*", data) +
                     request("accounting_search", b"\0(") +
                     request("accounting_search", b"ab", b"xab"))
        self.assertEqual(result[0]["match"]["span"], [0, len(data)])
        self.assertGreater(result[0]["accounting"]["state_count"], 0)
        self.assertEqual(result[1], run(request("compile", b"\0("))[0])
        self.assertEqual(result[2], option("--accounting", "search", b"ab".hex(),
                                          b"xab".hex()))

    def test_benchmark_stdin_protocol_is_equivalent_to_positional_arguments(self):
        operation, pattern, input_hex, template, iterations = "replace", "286129", "6161", "2431", "2"
        command = [str(BASELINE or EXE), *OPTIONS, "--benchmark-check-stdin"]
        workload = "\t".join((operation, pattern, input_hex, template, iterations)) + "\n"
        completed = subprocess.run(command, input=workload, text=True, capture_output=True,
                                   timeout=30, check=True)
        expected = option("--benchmark-check", operation, pattern, input_hex, template, iterations,
                          executable=BASELINE or EXE)
        self.assertEqual(json.loads(completed.stdout), expected)

    def test_benchmark_checksums_without_measurements(self):
        expected = {"compile": 4933856667531161709, "search": 14147806866978356193,
                    "full": 590684067820433389, "replace": 1156954223697340209}
        for operation, checksum in expected.items():
            with self.subTest(operation=operation):
                result = option("--benchmark-check", operation, b"(a)".hex(),
                                b"aa".hex(), b"$1".hex(), "2")
                self.assertEqual(result, {"ok": True, "operation": operation,
                                          "iterations": 2, "checksum": checksum,
                                          "measured": False})
                if BASELINE is not None:
                    self.assertEqual(result, option("--benchmark-check", operation,
                                     b"(a)".hex(), b"aa".hex(), b"$1".hex(), "2",
                                     executable=BASELINE))

    def test_observer_preserves_uninstrumented_results(self):
        if BASELINE is None:
            self.skipTest("Pass --baseline to verify the uninstrumented build")
        cases = [request("search", b"(a)?()", b""),
                 request("replace", b"a*", b"a\x00\xff", b"<$&>"),
                 request("find_all", b"|a", b"a"),
                 request("compile", b"["), request("compile", b"\xff["),
                 request("compile", b"\x00("),
                 request("search", b"(a+)+$", b"a" * 30 + b"b")]
        observed = run("".join(cases))
        baseline = run("".join(cases), BASELINE)
        for before, after in zip(baseline, observed):
            if "message_hex" in after:
                after["message_hex"] = bytes.fromhex(after["message_hex"]).split(b"\0")[0].hex()
            self.assertEqual(before, after)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=EXE)
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--locale")
    parser.add_argument("--lcid")
    args, remaining = parser.parse_known_args()
    EXE = args.reference.resolve()
    BASELINE = args.baseline.resolve() if args.baseline else None
    if args.locale:
        OPTIONS.extend(["--locale", args.locale])
    if args.lcid:
        OPTIONS.extend(["--lcid", args.lcid])
    unittest.main(argv=[__file__, *remaining])
