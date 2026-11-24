"""Tests for skipping Ladybird's dependency build when its inputs have not changed."""

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from elaris_cli import engine


class DependenciesAreCurrentTests(unittest.TestCase):
    def setUp(self) -> None:
        self.directory = tempfile.TemporaryDirectory()
        root = Path(self.directory.name)
        self.stamp = root / "elaris-dependencies.stamp"
        self.vcpkg = root / "vcpkg_installed"
        patches = [
            mock.patch.object(engine, "DEPENDENCIES_STAMP", self.stamp),
            mock.patch.object(engine, "VCPKG_INSTALLED", self.vcpkg),
            mock.patch.object(engine, "vcpkg_triplet", return_value="x64-test"),
            mock.patch.object(engine, "dependencies_fingerprint", return_value="abc"),
        ]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

    def tearDown(self) -> None:
        self.directory.cleanup()

    def test_missing_stamp_means_rebuild(self) -> None:
        (self.vcpkg / "x64-test").mkdir(parents=True)
        self.assertFalse(engine.dependencies_are_current())

    def test_matching_stamp_with_installed_packages_skips_rebuild(self) -> None:
        (self.vcpkg / "x64-test").mkdir(parents=True)
        self.stamp.write_text("abc")
        self.assertTrue(engine.dependencies_are_current())

    def test_changed_fingerprint_means_rebuild(self) -> None:
        (self.vcpkg / "x64-test").mkdir(parents=True)
        self.stamp.write_text("older")
        self.assertFalse(engine.dependencies_are_current())

    def test_missing_vcpkg_packages_means_rebuild(self) -> None:
        self.stamp.write_text("abc")
        self.assertFalse(engine.dependencies_are_current())


if __name__ == "__main__":
    unittest.main()
