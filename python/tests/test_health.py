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
