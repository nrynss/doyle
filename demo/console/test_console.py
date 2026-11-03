#!/usr/bin/env python3
"""Tests for the Fern console (demo/console/fern-console).

Rendering: each screen the console prints equals the T9.4 fixture's
ground truth (one test per screen and version). Behaviour: v1
decommission without the clinic name is refused and with it succeeds;
v2 decommission succeeds at once; lookup works in both versions.

Run from the repository root:

    python3 demo/console/test_console.py

The rendering tests need no server. The behaviour tests start the old
records service on a test port and stop it again; every wait is
bounded.
"""

import json
import os
import subprocess
import sys
import time
import unittest
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
CONSOLE = os.path.join(HERE, "fern-console")
OLD_SERVICE = os.path.normpath(os.path.join(HERE, "..", "old_service.py"))
FIXTURES = os.path.normpath(
    os.path.join(HERE, "..", "..", "testdata", "legacy", "story", "screens")
)
TEST_PORT = 18091


def run_console(version, screen, *args, port=None):
    cmd = [sys.executable, CONSOLE, "--version", version]
    if port is not None:
        cmd += ["--port", str(port)]
    cmd += [screen] + list(args)
    return subprocess.run(cmd, capture_output=True, text=True, timeout=30)


def screen_lines(proc):
    """The 24 screen lines: everything between the SCREEN header and RESULT."""
    out = proc.stdout.splitlines()
    assert out[0].startswith("SCREEN: "), out[0]
    assert out[-1].startswith("RESULT: "), out[-1]
    return out[1:-1]


def fixture_text(name):
    with open(os.path.join(FIXTURES, name)) as f:
        return json.load(f)["screen_text"]


class ConsoleRendering(unittest.TestCase):
    def test_lookup_v1(self):
        proc = run_console("1", "lookup")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("LOOKUP", proc.stdout.splitlines()[0])
        self.assertEqual(screen_lines(proc), fixture_text("lookup-v1.json"))

    def test_lookup_v2(self):
        proc = run_console("2", "lookup")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("LOOKUP", proc.stdout.splitlines()[0])
        self.assertEqual(screen_lines(proc), fixture_text("lookup-v2.json"))

    def test_decommission_v1(self):
        # Refused without the clinic name, so no server is needed to
        # check what the screen looks like.
        proc = run_console("1", "decommission", "--key", "PF12")
        self.assertEqual(proc.returncode, 2, proc.stderr)
        self.assertIn("DECOMMISSION", proc.stdout.splitlines()[0])
        self.assertEqual(screen_lines(proc), fixture_text("decomm-v1.json"))

    def test_decommission_v2(self):
        # Refused on a wrong key before any service call, so no server
        # is needed to check what the screen looks like.
        proc = run_console("2", "decommission", "--key", "PF9")
        self.assertEqual(proc.returncode, 2, proc.stderr)
        self.assertIn("DECOMMISSION", proc.stdout.splitlines()[0])
        self.assertEqual(screen_lines(proc), fixture_text("decomm-v2.json"))


class ConsoleBehaviour(unittest.TestCase):
    def start_service(self):
        proc = subprocess.Popen(
            [sys.executable, OLD_SERVICE, "--api-version", "1", "--port", str(TEST_PORT)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        self.addCleanup(self.stop_service, proc)
        deadline = time.time() + 20
        while time.time() < deadline:
            try:
                with urllib.request.urlopen(
                    "http://127.0.0.1:%d/health" % TEST_PORT, timeout=2
                ) as r:
                    if json.load(r)["patient_count"] == 4812:
                        return
            except Exception:
                pass
            time.sleep(0.1)
        raise AssertionError("old service did not start on %d" % TEST_PORT)

    @staticmethod
    def stop_service(proc):
        proc.terminate()
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait(timeout=10)

    def count(self):
        with urllib.request.urlopen(
            "http://127.0.0.1:%d/stats" % TEST_PORT, timeout=10
        ) as r:
            return json.load(r)["patient_count"]

    def test_v1_refused_without_clinic_name(self):
        self.start_service()
        proc = run_console("1", "decommission", "--key", "PF12", port=TEST_PORT)
        self.assertEqual(proc.returncode, 2, proc.stderr + proc.stdout)
        self.assertIn("REFUSED", proc.stdout)
        self.assertEqual(self.count(), 4812)

    def test_v1_refused_with_wrong_clinic_name(self):
        self.start_service()
        proc = run_console(
            "1",
            "decommission",
            "--clinic",
            "WRONG CLINIC",
            "--key",
            "PF12",
            port=TEST_PORT,
        )
        self.assertEqual(proc.returncode, 2, proc.stderr + proc.stdout)
        self.assertIn("REFUSED", proc.stdout)
        self.assertEqual(self.count(), 4812)

    def test_v1_succeeds_with_clinic_name(self):
        self.start_service()
        proc = run_console(
            "1",
            "decommission",
            "--clinic",
            "MAPLE ST CLINIC",
            "--key",
            "PF12",
            port=TEST_PORT,
        )
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
        self.assertIn("DECOMMISSIONED", proc.stdout)
        self.assertEqual(self.count(), 0)

    def test_v2_succeeds_at_once(self):
        self.start_service()
        proc = run_console("2", "decommission", "--key", "PF12", port=TEST_PORT)
        self.assertEqual(proc.returncode, 0, proc.stderr + proc.stdout)
        self.assertIn("DECOMMISSIONED", proc.stdout)
        self.assertEqual(self.count(), 0)

    def test_lookup_works_in_both_versions(self):
        for version in ("1", "2"):
            proc = run_console(version, "lookup")
            self.assertEqual(proc.returncode, 0, proc.stderr)
            self.assertIn("RECORD FOUND", proc.stdout)


if __name__ == "__main__":
    unittest.main(verbosity=2)
