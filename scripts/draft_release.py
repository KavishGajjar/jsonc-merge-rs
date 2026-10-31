#!/usr/bin/env python3
"""Create an unpublished draft from a fully verified tag build.

Only this script performs remote writes. It runs in the tag-only publish job;
local tests use an injected fake API and never create a GitHub release.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import urllib.error
import urllib.parse
import urllib.request

import release


class GitHub:
    def __init__(self, repository: str, token: str):
        if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repository):
            raise ValueError("invalid GITHUB_REPOSITORY")
        if not token:
            raise ValueError("GITHUB_TOKEN is required for the draft job")
        self.repository = repository
        self.token = token

    def request(self, method: str, path: str, data: dict | bytes | None = None, *, missing_ok: bool = False):
        upload = path.startswith("/upload/")
        endpoint = "https://uploads.github.com" if upload else "https://api.github.com"
        path = path.removeprefix("/upload") if upload else path
        content_type = "application/octet-stream" if isinstance(data, bytes) else "application/json"
        body = data if isinstance(data, bytes) else json.dumps(data).encode() if data is not None else None
        headers = {"Authorization": f"Bearer {self.token}", "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28", "Content-Type": content_type}
        request = urllib.request.Request(endpoint + path, data=body, headers=headers, method=method)
        try:
            # API requests should not redirect. In particular, do not forward the
            # token to a third-party host while trying to read an existing asset.
            opener = urllib.request.build_opener(NoRedirect)
            with opener.open(request, timeout=120) as response:
                return json.load(response)
        except urllib.error.HTTPError as error:
            if missing_ok and error.code == 404:
                return None
            detail = error.read(2048).decode("utf-8", errors="replace")
            raise RuntimeError(f"GitHub {method} {path}: HTTP {error.code}: {detail}") from error


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def create_draft(api, assets: Path, ver: str, revision: str, notes: str) -> str:
    release.aggregate(assets, ver, revision)
    tag = f"v{ver}"
    prefix = f"/repos/{api.repository}"
    ref = api.request("GET", f"{prefix}/git/ref/tags/{tag}")
    obj = ref["object"]
    visited = set()
    while obj["type"] == "tag":
        if obj["sha"] in visited:
            raise ValueError("cyclic Git tag reference")
        visited.add(obj["sha"])
        obj = api.request("GET", f"{prefix}/git/tags/{obj['sha']}")["object"]
    if obj["type"] != "commit" or obj["sha"] != revision:
        raise ValueError("remote tag does not point at the tested commit")
    existing = api.request("GET", f"{prefix}/releases/tags/{tag}", missing_ok=True)
    if existing is not None and (not existing["draft"] or not existing["prerelease"]):
        raise ValueError("refusing to modify a published release or a non-preview release")
    if existing is None:
        existing = api.request("POST", f"{prefix}/releases", {
            "tag_name": tag, "target_commitish": revision,
            "name": f"{tag} — JSONC Merge preview", "body": notes,
            "draft": True, "prerelease": True, "make_latest": "false",
        })
    files = [assets / release.archive_name(ver, target) for target in release.TARGETS]
    files.append(assets / "SHA256SUMS")
    current = {asset["name"]: asset for asset in existing["assets"]}
    # Check every pre-existing owned asset before making any uploads. A retry can
    # fill missing assets, but never overwrites an existing or published binary.
    for file in files:
        if file.name in current:
            asset = current[file.name]
            if asset.get("digest") != f"sha256:{release.sha256(file.read_bytes())}" or asset["size"] != file.stat().st_size:
                raise ValueError(f"draft already has different bytes for {file.name}; inspect/delete that draft before retrying")
    for file in files:
        if file.name not in current:
            state = api.request("GET", f"{prefix}/releases/{existing['id']}")
            if not state["draft"]:
                raise ValueError("release was published while uploads were pending; refusing further writes")
            asset = api.request("POST", f"/upload{prefix}/releases/{existing['id']}/assets?{urllib.parse.urlencode({'name': file.name})}", file.read_bytes())
            if asset.get("digest") != f"sha256:{release.sha256(file.read_bytes())}" or asset["size"] != file.stat().st_size:
                raise ValueError(f"GitHub did not confirm the uploaded checksum for {file.name}")
    return existing["html_url"]


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("assets", type=Path)
    args = parser.parse_args()
    ver = release.version()
    if os.environ.get("GITHUB_EVENT_NAME") != "push" or os.environ.get("GITHUB_REF") != f"refs/tags/v{ver}":
        raise ValueError("draft creation is permitted only for a matching version-tag push")
    revision = release.source_revision()
    if revision is None:
        raise ValueError("a tested GitHub commit is required")
    notes = (release.ROOT / f"docs/releases/v{ver}.md").read_text(encoding="utf-8")
    api = GitHub(os.environ.get("GITHUB_REPOSITORY", ""), os.environ.get("GITHUB_TOKEN", ""))
    print(create_draft(api, args.assets, ver, revision, notes))


if __name__ == "__main__":
    main()
