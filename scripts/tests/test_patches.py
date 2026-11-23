"""Tests for the Ladybird patch conflict check."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from elaris_cli import patches

PATCH = """\
--- a/Libraries/Lib/File.cpp
+++ b/Libraries/Lib/File.cpp
@@ -1,3 +1,3 @@
 one
-two
+TWO
 three
"""


class CheckPatchTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        self.engine = root / "engine"
        self.patch = root / "change.patch"
        self.patch.write_text(PATCH)
        (self.engine / "Libraries" / "Lib").mkdir(parents=True)
        self.engine_patch = mock.patch.object(patches, "ENGINE", self.engine)
        self.engine_patch.start()

    def tearDown(self) -> None:
        self.engine_patch.stop()
        self.directory.cleanup()

    def write_source(self, text: str) -> None:
        (self.engine / "Libraries" / "Lib" / "File.cpp").write_text(text)

    def test_targets(self) -> None:
        self.assertEqual(patches.targets(self.patch), ["Libraries/Lib/File.cpp"])

    def test_clean(self) -> None:
        self.write_source("one\ntwo\nthree\n")
        self.assertEqual(patches.check_patch(self.patch, None).status, "clean")

    def test_conflict(self) -> None:
        self.write_source("one\nchanged upstream\nthree\n")
        self.assertEqual(patches.check_patch(self.patch, None).status, "conflict")

    def test_missing_file(self) -> None:
        result = patches.check_patch(self.patch, None)
        self.assertEqual(result.status, "conflict")
        self.assertIn("no longer exists", result.detail)

    def test_offset(self) -> None:
        self.write_source("zero\nzero\nzero\nzero\none\ntwo\nthree\n")
        self.assertEqual(patches.check_patch(self.patch, None).status, "fuzzy")


if __name__ == "__main__":
    unittest.main()
