#!/usr/bin/env python3
"""Build, inspect and smoke-test release archives using only Python's stdlib.

No command in this module publishes files or needs network access.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import struct
import subprocess
import tarfile
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parent.parent
VERSION_RE = re.compile(r"(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)")
TARGETS = {
    "x86_64-unknown-linux-musl": ("tar.gz", "jsonc-merge"),
    "aarch64-apple-darwin": ("tar.gz", "jsonc-merge"),
    "x86_64-apple-darwin": ("tar.gz", "jsonc-merge"),
    "x86_64-pc-windows-msvc": ("zip", "jsonc-merge.exe"),
}


def version(root: Path = ROOT) -> str:
    value = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    if not VERSION_RE.fullmatch(value):
        raise ValueError(f"expected a numeric X.Y.Z workspace version, got {value!r}")
    return value


def tag_version(ref: str, expected: str) -> None:
    if ref.startswith("refs/tags/") and ref != f"refs/tags/v{expected}":
        raise ValueError(f"tag {ref!r} must match workspace version v{expected}")


def source_revision() -> str | None:
    sha = os.environ.get("GITHUB_SHA")
    if not sha:
        return None
    if not re.fullmatch(r"[0-9a-fA-F]{40}", sha):
        raise ValueError("GITHUB_SHA must be a full Git object id")
    expected = subprocess.check_output(["git", "rev-parse", f"{sha}^{{commit}}"], cwd=ROOT, text=True).strip()
    actual = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if actual != expected:
        raise ValueError("checked-out commit differs from GITHUB_SHA")
    subprocess.run(["git", "diff", "--exit-code", "HEAD", "--"], cwd=ROOT, check=True)
    return actual


def archive_name(ver: str, target: str) -> str:
    return f"jsonc-merge-v{ver}-{target}.{TARGETS[target][0]}"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def u32(data: bytes, at: int) -> int:
    return struct.unpack_from("<I", data, at)[0]


def inspect_binary(data: bytes, target: str) -> None:
    """Check architecture and the runtime dependencies we advertise.

    Linux must have no ELF interpreter; Windows must not import a redistributable
    CRT; macOS must use system libraries with a minimum OS no newer than 13.0.
    Native execution in smoke() is a separate check, not replaced by this parser.
    """
    if target == "x86_64-unknown-linux-musl":
        if data[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", data, 18)[0] != 62:
            raise ValueError("expected a little-endian x86-64 ELF executable")
        phoff = struct.unpack_from("<Q", data, 32)[0]
        size, count = struct.unpack_from("<HH", data, 54)
        if count and size < 56:
            raise ValueError("invalid ELF program-header size")
        for i in range(count):
            at = phoff + i * size
            if at + size > len(data):
                raise ValueError("truncated ELF program headers")
            if u32(data, at) == 3:
                raise ValueError("Linux archive must contain a static binary, not an ELF interpreter dependency")
    elif target.endswith("apple-darwin"):
        expected = 0x0100000C if target.startswith("aarch64") else 0x01000007
        if data[:4] != b"\xcf\xfa\xed\xfe" or u32(data, 4) != expected:
            raise ValueError(f"expected a native 64-bit Mach-O executable for {target}")
        at = 32
        minimum = None
        for _ in range(u32(data, 16)):
            cmd, size = struct.unpack_from("<II", data, at)
            if size < 8 or at + size > len(data):
                raise ValueError("invalid Mach-O load command")
            if cmd == 0x32:  # LC_BUILD_VERSION
                if u32(data, at + 8) != 1:
                    raise ValueError("expected the macOS platform")
                minimum = u32(data, at + 12)
            elif cmd == 0x24:  # LC_VERSION_MIN_MACOSX
                minimum = u32(data, at + 8)
            elif cmd in {0xC, 0x80000018, 0x8000001F, 0x80000023}:
                start = at + u32(data, at + 8)
                end = data.find(b"\0", start, at + size)
                if start < at + 12 or end < 0:
                    raise ValueError("invalid Mach-O library name")
                library = data[start:end].decode("utf-8")
                if not library.startswith(("/usr/lib/", "/System/Library/")):
                    raise ValueError(f"non-system macOS runtime dependency: {library}")
            at += size
        if minimum is None or minimum > 0x000D0000:
            raise ValueError("binary must declare a macOS minimum version no newer than 13.0")
    elif target == "x86_64-pc-windows-msvc":
        if data[:2] != b"MZ":
            raise ValueError("expected a Windows PE executable")
        pe = u32(data, 0x3C)
        if data[pe:pe + 4] != b"PE\0\0" or struct.unpack_from("<H", data, pe + 4)[0] != 0x8664:
            raise ValueError("expected an x86-64 PE executable")
        opt = pe + 24
        if struct.unpack_from("<H", data, opt)[0] != 0x20B:
            raise ValueError("expected a PE32+ optional header")
        sections = opt + struct.unpack_from("<H", data, pe + 20)[0]

        def offset(rva: int) -> int:
            if rva < u32(data, opt + 60):
                return rva
            for i in range(struct.unpack_from("<H", data, pe + 6)[0]):
                s = sections + 40 * i
                virtual_size, va, raw_size, raw_at = struct.unpack_from("<IIII", data, s + 8)
                if va <= rva < va + max(virtual_size, raw_size):
                    if rva - va >= raw_size:
                        break
                    return raw_at + rva - va
            raise ValueError("PE import points outside file-backed sections")

        directory = u32(data, opt + 120)  # import directory RVA
        if directory:
            at = offset(directory)
            while True:
                fields = struct.unpack_from("<IIIII", data, at)
                if not any(fields):
                    break
                name = offset(fields[3])
                end = data.find(b"\0", name)
                if end < 0:
                    raise ValueError("unterminated PE library name")
                library = data[name:end].decode("ascii").lower()
                if library.startswith(("vcruntime", "msvcp", "msvcr", "api-ms-win-crt-")) or library == "ucrtbase.dll":
                    raise ValueError(f"Windows binary must use static CRT, found {library}")
                at += 20
    else:
        raise ValueError(f"unsupported release target: {target}")


def binary_readme(ver: str, target: str) -> bytes:
    executable = TARGETS[target][1]
    prefix = ".\\" if target.endswith("windows-msvc") else "./"
    return f"""JSONC Merge {ver} — {target}

No Rust, Python or Node.js installation is required to run this executable.
Git is required only for optional Git merge-driver integration.

From this extracted directory:
  {prefix}{executable} --version
  {prefix}{executable} merge examples/launch/base.jsonc examples/launch/ours.jsonc examples/launch/theirs.jsonc --policy examples/launch/policy.json

The output keeps Worker before API and changes the API program to dist/api.js.
The complete expected output is examples/launch/expected.jsonc.

See README.md and docs/install.md for installation and checksum instructions.
LICENSE, THIRD_PARTY.md and third-party-licenses contain the applicable notices.
macOS/Windows downloads are not distributor-signed or notarized.
""".encode()


def payload(binary: Path, target: str, ver: str, revision: str | None, root: Path = ROOT) -> dict[str, bytes]:
    files = {TARGETS[target][1]: binary.read_bytes()}
    for name in ["LICENSE", "THIRD_PARTY.md", "README.md", "CHANGELOG.md", "CONTRIBUTING.md"]:
        files[name] = (root / name).read_bytes()
    for folder in ["docs", "examples/launch", "third-party-licenses"]:
        for file in sorted((root / folder).rglob("*")):
            if file.is_file():
                files[file.relative_to(root).as_posix()] = file.read_bytes()
    files["BINARY-README.txt"] = binary_readme(ver, target)
    files["BUILD.json"] = (json.dumps({"schema_version": 1, "version": ver, "target": target, "source_revision": revision}, sort_keys=True, indent=2) + "\n").encode()
    files["MANIFEST.json"] = (json.dumps({name: sha256(data) for name, data in sorted(files.items())}, sort_keys=True, indent=2) + "\n").encode()
    return files


def write_archive(files: dict[str, bytes], target: str, ver: str, output: Path) -> Path:
    output.mkdir(parents=True, exist_ok=True)
    destination = output / archive_name(ver, target)
    folder = destination.name.removesuffix("." + TARGETS[target][0])
    executable = TARGETS[target][1]
    if target.endswith("windows-msvc"):
        with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for name, data in sorted(files.items()):
                info = zipfile.ZipInfo(f"{folder}/{name}", date_time=(1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.external_attr = (0o100755 if name == executable else 0o100644) << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, data)
    else:
        with destination.open("wb") as raw, gzip.GzipFile(fileobj=raw, mode="wb", filename="", mtime=0) as zipped, tarfile.open(fileobj=zipped, mode="w", format=tarfile.PAX_FORMAT) as archive:
            for name, data in sorted(files.items()):
                info = tarfile.TarInfo(f"{folder}/{name}")
                info.size = len(data)
                info.mode = 0o755 if name == executable else 0o644
                archive.addfile(info, io.BytesIO(data))
    destination.with_name(destination.name + ".sha256").write_text(f"{sha256(destination.read_bytes())}  {destination.name}\n", encoding="utf-8", newline="\n")
    return destination


def read_archive(archive: Path) -> tuple[dict[str, bytes], dict]:
    files = {}
    modes = {}
    roots = set()

    def add(name: str, data: bytes, mode: int) -> None:
        p = PurePosixPath(name)
        if "\\" in name or ":" in name or p.is_absolute() or len(p.parts) < 2 or any(part in {"", ".", ".."} for part in name.split("/")):
            raise ValueError(f"unsafe archive path: {name}")
        roots.add(p.parts[0])
        relative = "/".join(p.parts[1:])
        if relative in files:
            raise ValueError(f"duplicate archive member: {relative}")
        files[relative] = data
        modes[relative] = mode & 0o7777

    if archive.name.endswith(".zip"):
        with zipfile.ZipFile(archive) as zipped:
            for info in zipped.infolist():
                if info.is_dir() or ((info.external_attr >> 16) & 0o170000) != 0o100000:
                    raise ValueError("release ZIP must contain regular files only")
                add(info.filename, zipped.read(info), info.external_attr >> 16)
    elif archive.name.endswith(".tar.gz"):
        with tarfile.open(archive, "r:gz") as zipped:
            for info in zipped:
                if not info.isfile():
                    raise ValueError("release tarball must contain regular files only")
                add(info.name, zipped.extractfile(info).read(), info.mode)
    else:
        raise ValueError("expected a .zip or .tar.gz release archive")
    if len(roots) != 1:
        raise ValueError("release archive must have exactly one root directory")
    build = json.loads(files["BUILD.json"])
    target, ver = build["target"], build["version"]
    if build["schema_version"] != 1 or target not in TARGETS or not VERSION_RE.fullmatch(ver):
        raise ValueError("invalid release metadata")
    expected_name = archive_name(ver, target)
    if archive.name != expected_name or roots != {expected_name.removesuffix("." + TARGETS[target][0])}:
        raise ValueError("archive name/root does not match its build metadata")
    manifest = json.loads(files["MANIFEST.json"])
    if manifest != {name: sha256(data) for name, data in sorted(files.items()) if name != "MANIFEST.json"}:
        raise ValueError("archive payload differs from its manifest")
    for required in [TARGETS[target][1], "LICENSE", "THIRD_PARTY.md", "BINARY-README.txt", "README.md", "docs/install.md", "examples/launch/expected.jsonc"]:
        if required not in files:
            raise ValueError(f"missing required release file: {required}")
    if not any(name.startswith("third-party-licenses/") for name in files):
        raise ValueError("missing upstream license texts")
    for name, mode in modes.items():
        expected_mode = 0o755 if name == TARGETS[target][1] else 0o644
        if mode != expected_mode:
            raise ValueError(f"unexpected archive permissions for {name}: {mode:o}")
    inspect_binary(files[TARGETS[target][1]], target)
    return files, build


def run(binary: Path, args: list[str], cwd: Path, expected: int = 0) -> subprocess.CompletedProcess:
    result = subprocess.run([str(binary), *args], cwd=cwd, capture_output=True, text=True, encoding="utf-8")
    if result.returncode != expected:
        raise RuntimeError(f"{args!r}: expected exit {expected}, got {result.returncode}\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")
    return result


def smoke(archive: Path) -> None:
    files, build = read_archive(archive)
    with tempfile.TemporaryDirectory(prefix="jsonc-package-") as directory:
        root = Path(directory)
        for name, data in files.items():
            file = root / name
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(data)
        binary = root / TARGETS[build["target"]][1]
        binary.chmod(0o755)
        if run(binary, ["--version"], root).stdout.strip() != f"jsonc-merge {build['version']}":
            raise ValueError("packaged executable version differs from metadata")
        args = ["examples/launch/base.jsonc", "examples/launch/ours.jsonc", "examples/launch/theirs.jsonc", "--policy", "examples/launch/policy.json"]
        before = {name: (root / name).read_bytes() for name in args[:3]}
        run(binary, ["check", *args[:3], "--policy", args[-1]], root)
        result = run(binary, ["merge", *args], root)
        # Normalise only the subprocess text-mode newline translation on Windows.
        if result.stdout != (root / "examples/launch/expected.jsonc").read_text(encoding="utf-8"):
            raise ValueError(f"packaged demo output differs from the checked golden:\n{result.stdout}")
        report = json.loads(run(binary, ["plan", *args], root).stdout)
        if report["schema_version"] != 1 or not report["clean"]:
            raise ValueError("packaged plan report is not clean")
        for name, data in before.items():
            if (root / name).read_bytes() != data:
                raise ValueError(f"ordinary merge changed its input: {name}")
        for name, text in [("base", "0"), ("ours", "1"), ("theirs", "2")]:
            (root / name).write_text(text, encoding="utf-8", newline="\n")
        conflict = run(binary, ["merge", "base", "ours", "theirs"], root, 1)
        if conflict.stdout or json.loads(conflict.stderr)["clean"]:
            raise ValueError("packaged conflict emitted a successful configuration")
        run(binary, ["git-driver", "base", "ours", "theirs", "7"], root, 1)
        if "||||||| BASE" not in (root / "ours").read_text(encoding="utf-8"):
            raise ValueError("packaged Git driver omitted its base alternative")
    print(f"Native extracted-binary checks passed: {archive.name}")


def aggregate(directory: Path, ver: str, revision: str | None) -> Path:
    expected = {archive_name(ver, target) for target in TARGETS}
    found = {p.name for p in directory.iterdir() if p.name.endswith((".tar.gz", ".zip"))}
    if found != expected:
        raise ValueError(f"expected all four release archives; missing={sorted(expected-found)}, unexpected={sorted(found-expected)}")
    lines = []
    for name in sorted(expected):
        file = directory / name
        _, build = read_archive(file)
        if build["version"] != ver or build["source_revision"] != revision:
            raise ValueError(f"mixed release versions/commits: {name}")
        line = f"{sha256(file.read_bytes())}  {name}\n"
        if file.with_name(name + ".sha256").read_text(encoding="utf-8") != line:
            raise ValueError(f"checksum sidecar mismatch: {name}")
        lines.append(line)
    output = directory / "SHA256SUMS"
    checksums = "".join(lines)
    if output.exists() and output.read_text(encoding="utf-8") != checksums:
        raise ValueError("existing SHA256SUMS differs from the verified archives")
    output.write_text(checksums, encoding="utf-8", newline="\n")
    return output


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    meta = sub.add_parser("metadata")
    meta.add_argument("--github-output", type=Path)
    package = sub.add_parser("package")
    package.add_argument("--target", choices=TARGETS, required=True)
    package.add_argument("--binary", type=Path, required=True)
    package.add_argument("--output", type=Path, required=True)
    verify = sub.add_parser("verify")
    verify.add_argument("archive", type=Path)
    check = sub.add_parser("smoke")
    check.add_argument("archive", type=Path)
    combine = sub.add_parser("aggregate")
    combine.add_argument("directory", type=Path)
    args = parser.parse_args()
    ver = version()
    tag_version(os.environ.get("GITHUB_REF", ""), ver)
    if args.command == "metadata":
        if args.github_output:
            with args.github_output.open("a", encoding="utf-8", newline="\n") as output:
                output.write(f"version={ver}\ntag=v{ver}\n")
        print(json.dumps({"version": ver, "tag": f"v{ver}"}))
    elif args.command == "package":
        inspect_binary(args.binary.read_bytes(), args.target)
        archive = write_archive(payload(args.binary, args.target, ver, source_revision()), args.target, ver, args.output)
        read_archive(archive)
        print(archive)
    elif args.command == "verify":
        _, build = read_archive(args.archive)
        print(json.dumps(build, sort_keys=True))
    elif args.command == "smoke":
        smoke(args.archive)
    elif args.command == "aggregate":
        print(aggregate(args.directory, ver, source_revision()))


if __name__ == "__main__":
    main()
