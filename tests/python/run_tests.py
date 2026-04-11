#!/usr/bin/env python3
"""External test runner for RythonEngine Python integration tests.

Discovers test_*.py files in this directory, runs each inside the headless
engine, parses structured output, and reports results.

Usage:
    python3 tests/python/run_tests.py                # run all tests
    python3 tests/python/run_tests.py test_types      # run matching tests only

Environment variables:
    RYTHON_TEST_TIMEOUT  seconds before a test is killed (default: 30)
"""

import glob
import os
import subprocess
import sys


def discover_tests(test_dir, filters):
    """Find test_*.py files, optionally filtered by substrings."""
    pattern = os.path.join(test_dir, "test_*.py")
    paths = sorted(glob.glob(pattern))
    if filters:
        paths = [
            p for p in paths
            if any(f in os.path.basename(p) for f in filters)
        ]
    return paths


def parse_results(output):
    """Extract PASS/FAIL lines from structured output block.

    Returns (passed, failed, details) where details is a list of
    (name, passed, detail) tuples.
    """
    lines = output.splitlines()
    in_block = False
    details = []
    for line in lines:
        if line.strip() == "RYTHON_TEST_BEGIN":
            in_block = True
            continue
        if line.strip() == "RYTHON_TEST_END":
            break
        if in_block:
            stripped = line.strip()
            if stripped.startswith("PASS "):
                name = stripped[5:]
                details.append((name, True, ""))
            elif stripped.startswith("FAIL "):
                rest = stripped[5:]
                if ": " in rest:
                    name, detail = rest.split(": ", 1)
                else:
                    name, detail = rest, ""
                details.append((name, False, detail))
    passed = sum(1 for _, p, _ in details if p)
    failed = sum(1 for _, p, _ in details if not p)
    return passed, failed, details


def run_test_module(binary, test_dir, module_name, timeout):
    """Run a single test module in the headless engine.

    Returns (passed, failed, details, error) where error is None on
    success or a string describing the failure.
    """
    cmd = [
        binary,
        "--headless",
        "--script-dir", test_dir,
        "--entry-point", module_name,
    ]
    try:
        result = subprocess.run(
            cmd,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return 0, 0, [], f"TIMEOUT after {timeout}s"
    except FileNotFoundError:
        return 0, 0, [], f"binary not found: {binary}"

    output = result.stdout + result.stderr

    if "RYTHON_TEST_BEGIN" not in output:
        snippet = output[-500:] if len(output) > 500 else output
        return 0, 0, [], (
            f"no test output (exit code {result.returncode})\n{snippet}"
        )

    passed, failed, details = parse_results(output)
    return passed, failed, details, None


def main():
    filters = sys.argv[1:]
    test_dir = os.path.abspath(os.path.dirname(__file__))
    # Resolve project root relative to this script (tests/python/run_tests.py)
    project_root = os.path.abspath(os.path.join(test_dir, "..", ".."))
    binary = os.path.join(project_root, "target", "debug", "rython")
    timeout = int(os.environ.get("RYTHON_TEST_TIMEOUT", "30"))

    test_files = discover_tests(test_dir, filters)
    if not test_files:
        print("No test files found.")
        sys.exit(1)

    total_passed = 0
    total_failed = 0
    any_error = False

    for path in test_files:
        module_name = os.path.splitext(os.path.basename(path))[0]
        print(f"--- {module_name} ---")

        passed, failed, details, error = run_test_module(
            binary, test_dir, module_name, timeout
        )

        if error:
            print(f"  ERROR: {error}")
            any_error = True
            continue

        for name, ok, detail in details:
            if ok:
                print(f"  PASS {name}")
            else:
                print(f"  FAIL {name}: {detail}")

        print(f"  {passed} passed, {failed} failed")
        total_passed += passed
        total_failed += failed
        if failed > 0:
            any_error = True

    print()
    print(f"Total: {total_passed} passed, {total_failed} failed")

    if any_error:
        sys.exit(1)
    sys.exit(0)


if __name__ == "__main__":
    main()
