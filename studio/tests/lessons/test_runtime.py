"""A lesson script runs as Python and writes the files its Markdown names."""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

DEMO = Path(__file__).parent / "vector-cache" / "demo.py"


class LessonScriptTest(unittest.TestCase):
    def test_plain_script_prints_predictions_and_creates_picture(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            copied = Path(folder) / "demo.py"
            copied.write_bytes(DEMO.read_bytes())
            completed = subprocess.run([sys.executable, str(copied)], cwd=folder, capture_output=True, text=True, check=True)
            self.assertEqual(completed.stdout, "query (2,) → rose\nquery (2,) → tulip\nplotted (3, 2)\n")
            self.assertTrue((Path(folder) / "output" / "flowers.svg").read_text().startswith("<svg"))


if __name__ == "__main__":
    unittest.main()
