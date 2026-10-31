"""Archive tests use synthetic executable headers; native execution is tested
separately by the release matrix and the offline musl Docker check.
"""
import io
import json
from pathlib import Path
import struct
import sys
import tarfile
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release

REVISION = "a" * 40


def elf(interpreter=False):
    data = bytearray(120)
    data[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<H", data, 18, 62)
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<HH", data, 54, 56, 1)
    struct.pack_into("<I", data, 64, 3 if interpreter else 1)
    return bytes(data)


def macho(arm=False, minimum=0xD0000, library=None):
    command = struct.pack("<IIIIII", 0x32, 24, 1, minimum, minimum, 0)
    if library:
        name = library.encode() + b"\0"
        size = (24 + len(name) + 7) // 8 * 8
        command += struct.pack("<IIIIII", 0xC, size, 24, 0, 0, 0) + name + bytes(size - 24 - len(name))
    header = struct.pack("<IIIIIIII", 0xFEEDFACF, 0x0100000C if arm else 0x01000007, 0, 2, 2 if library else 1, len(command), 0, 0)
    return header + command


def pe(library="KERNEL32.dll"):
    data = bytearray(1024)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 128)
    data[128:132] = b"PE\0\0"
    struct.pack_into("<HH", data, 132, 0x8664, 1)
    struct.pack_into("<H", data, 148, 240)
    opt = 152
    struct.pack_into("<H", data, opt, 0x20B)
    struct.pack_into("<I", data, opt + 60, 512)
    struct.pack_into("<I", data, opt + 120, 0x1000)
    struct.pack_into("<IIII", data, opt + 240 + 8, 512, 0x1000, 512, 512)
    struct.pack_into("<IIIII", data, 512, 0, 0, 0, 0x1050, 1)
    name = library.encode() + b"\0"
    data[592:592 + len(name)] = name
    return bytes(data)


def fake_binary(target):
    if "linux" in target:
        return elf()
    if "apple" in target:
        return macho(arm=target.startswith("aarch64"))
    return pe()


def fixture(directory, target, ver="0.1.1", revision=REVISION):
    file = directory / "input-binary"
    file.write_bytes(fake_binary(target))
    files = release.payload(file, target, ver, revision)
    return release.write_archive(files, target, ver, directory), files


def bundle(directory, revision=REVISION):
    for target in release.TARGETS:
        fixture(directory, target, revision=revision)


class BinaryInspectionTests(unittest.TestCase):
    def test_synthetic_headers_match_all_declared_architectures(self):
        for target in release.TARGETS:
            with self.subTest(target=target):
                release.inspect_binary(fake_binary(target), target)

    def test_dynamic_linux_binary_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "static binary"):
            release.inspect_binary(elf(True), "x86_64-unknown-linux-musl")

    def test_macos_architecture_os_floor_and_libraries(self):
        for data in [macho(False), macho(True, 0xE0000), macho(True, library="@rpath/libextra.dylib")]:
            with self.subTest(data=data):
                with self.assertRaises(ValueError):
                    release.inspect_binary(data, "aarch64-apple-darwin")
        release.inspect_binary(macho(True, library="/usr/lib/libSystem.B.dylib"), "aarch64-apple-darwin")

    def test_redistributable_windows_crt_is_rejected(self):
        for library in ["VCRUNTIME140.dll", "MSVCP140.dll", "ucrtbase.dll", "api-ms-win-crt-runtime-l1-1-0.dll"]:
            with self.subTest(library=library):
                with self.assertRaisesRegex(ValueError, "static CRT"):
                    release.inspect_binary(pe(library), "x86_64-pc-windows-msvc")

    def test_wrong_binary_format_is_rejected(self):
        for target in release.TARGETS:
            with self.subTest(target=target):
                with self.assertRaises((ValueError, struct.error)):
                    release.inspect_binary(b"not an executable", target)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)

    def test_every_package_round_trips_and_includes_notices_and_demo(self):
        for target in release.TARGETS:
            with self.subTest(target=target):
                file, expected = fixture(self.directory, target)
                actual, build = release.read_archive(file)
                self.assertEqual(actual, expected)
                self.assertEqual(build["source_revision"], REVISION)
                self.assertIn("examples/launch/policy.json", actual)
                sidecar = file.with_name(file.name + ".sha256").read_text(encoding="utf-8")
                self.assertEqual(sidecar, f"{release.sha256(file.read_bytes())}  {file.name}\n")

    def test_archive_metadata_and_executable_modes_are_stable(self):
        target = "x86_64-unknown-linux-musl"
        file, payload = fixture(self.directory, target)
        before = file.read_bytes()
        release.write_archive(payload, target, "0.1.1", self.directory)
        self.assertEqual(before, file.read_bytes())
        with tarfile.open(file) as archive:
            for member in archive:
                self.assertEqual(member.mtime, 0)
                self.assertEqual(member.uid, 0)
                self.assertEqual(member.mode, 0o755 if member.name.endswith("/jsonc-merge") else 0o644)

    def test_archive_tampering_is_detected(self):
        target = "x86_64-unknown-linux-musl"
        _, files = fixture(self.directory, target)
        files["README.md"] = b"modified after manifest creation"
        file = release.write_archive(files, target, "0.1.1", self.directory)
        with self.assertRaisesRegex(ValueError, "manifest"):
            release.read_archive(file)

    def test_corrupted_executable_permissions_are_rejected(self):
        for target in ["x86_64-unknown-linux-musl", "x86_64-pc-windows-msvc"]:
            with self.subTest(target=target):
                file, _ = fixture(self.directory, target)
                executable = release.TARGETS[target][1]
                if file.suffix == ".zip":
                    with zipfile.ZipFile(file) as archive:
                        members = [(info, archive.read(info)) for info in archive.infolist()]
                    with zipfile.ZipFile(file, "w") as archive:
                        for info, data in members:
                            if info.filename.endswith("/" + executable):
                                info.external_attr = 0o100644 << 16
                            archive.writestr(info, data)
                else:
                    with tarfile.open(file, "r:gz") as archive:
                        members = [(info, archive.extractfile(info).read()) for info in archive]
                    with tarfile.open(file, "w:gz") as archive:
                        for info, data in members:
                            if info.name.endswith("/" + executable):
                                info.mode = 0o644
                            archive.addfile(info, io.BytesIO(data))
                with self.assertRaisesRegex(ValueError, "archive permissions"):
                    release.read_archive(file)

    def test_path_traversal_and_windows_drive_paths_are_rejected(self):
        target = "x86_64-unknown-linux-musl"
        _, original = fixture(self.directory, target)
        for name in ["../escape", "C:/escape", "a/../../escape", "a\\escape", "a//escape"]:
            with self.subTest(name=name):
                files = dict(original)
                files[name] = b"invalid"
                file = release.write_archive(files, target, "0.1.1", self.directory)
                with self.assertRaisesRegex(ValueError, "unsafe archive path"):
                    release.read_archive(file)

    def test_links_are_never_extracted(self):
        file = self.directory / release.archive_name("0.1.1", "x86_64-unknown-linux-musl")
        with tarfile.open(file, "w:gz") as archive:
            info = tarfile.TarInfo("root/link")
            info.type = tarfile.SYMTYPE
            info.linkname = "../../outside"
            archive.addfile(info)
        with self.assertRaisesRegex(ValueError, "regular files"):
            release.read_archive(file)

    def test_complete_bundle_creates_a_sorted_checksum_manifest(self):
        bundle(self.directory)
        file = release.aggregate(self.directory, "0.1.1", REVISION)
        lines = file.read_text(encoding="utf-8").splitlines()
        self.assertEqual(len(lines), 4)
        self.assertEqual(lines, sorted(lines, key=lambda line: line.split("  ")[1]))
        self.assertEqual(release.aggregate(self.directory, "0.1.1", REVISION), file)

    def test_missing_target_and_mixed_commit_fail_before_aggregation(self):
        bundle(self.directory)
        first = self.directory / release.archive_name("0.1.1", next(iter(release.TARGETS)))
        first.unlink()
        with self.assertRaisesRegex(ValueError, "all four"):
            release.aggregate(self.directory, "0.1.1", REVISION)
        fixture(self.directory, next(iter(release.TARGETS)), revision="b" * 40)
        with self.assertRaisesRegex(ValueError, "mixed release"):
            release.aggregate(self.directory, "0.1.1", REVISION)

    def test_checksum_tampering_and_stale_manifest_are_not_hidden(self):
        bundle(self.directory)
        sums = release.aggregate(self.directory, "0.1.1", REVISION)
        sums.write_text("incorrect\n")
        with self.assertRaisesRegex(ValueError, "existing SHA256SUMS"):
            release.aggregate(self.directory, "0.1.1", REVISION)
        sums.unlink()
        first = self.directory / release.archive_name("0.1.1", next(iter(release.TARGETS)))
        first.with_name(first.name + ".sha256").write_text("incorrect\n")
        with self.assertRaisesRegex(ValueError, "sidecar"):
            release.aggregate(self.directory, "0.1.1", REVISION)

    def test_tag_and_workspace_version_must_match(self):
        release.tag_version("refs/heads/main", "0.1.1")
        release.tag_version("refs/tags/v0.1.1", "0.1.1")
        for ref in ["refs/tags/v0.1.0", "refs/tags/not-a-version"]:
            with self.assertRaises(ValueError):
                release.tag_version(ref, "0.1.1")


if __name__ == "__main__":
    unittest.main()
