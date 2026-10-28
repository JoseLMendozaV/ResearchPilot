import json
import subprocess
import sys
import unittest


class WorkerHealthTest(unittest.TestCase):
    def test_worker_health(self) -> None:
        completed = subprocess.run(
            [sys.executable, "-m", "researchpilot_worker", "health"],
            check=True,
            capture_output=True,
            text=True,
        )
        self.assertEqual(json.loads(completed.stdout)["status"], "ok")

    def test_capabilities_report_optional_engines(self) -> None:
        completed = subprocess.run(
            [sys.executable, "-m", "researchpilot_worker", "capabilities"],
            check=True,
            capture_output=True,
            text=True,
        )
        report = json.loads(completed.stdout)
        self.assertEqual(report["protocolVersion"], 1)
        self.assertIn("available", report["tesseract"])
        self.assertIn("pdfplumber", report)
