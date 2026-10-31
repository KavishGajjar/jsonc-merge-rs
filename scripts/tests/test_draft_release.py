import copy
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import urllib.error

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import draft_release
import release
from test_release import REVISION, bundle


class FakeGitHub:
    repository = "owner/project"

    def __init__(self):
        self.existing = None
        self.tag_revision = REVISION
        self.writes = []
        self.fail_once = False
        self.publish_before_upload = False
        self.annotated = False

    def request(self, method, path, data=None, **kwargs):
        if path.endswith("/git/ref/tags/v0.1.1"):
            return {"object": {"type": "tag" if self.annotated else "commit", "sha": "b" * 40 if self.annotated else self.tag_revision}}
        if "/git/tags/" in path:
            return {"object": {"type": "commit", "sha": self.tag_revision}}
        if "/releases/tags/" in path:
            return copy.deepcopy(self.existing)
        if method == "POST" and path.endswith("/releases"):
            self.writes.append((method, path, data))
            self.existing = {"id": 7, "draft": data["draft"], "prerelease": data["prerelease"], "assets": [], "html_url": "https://github.com/owner/project/releases/tag/v0.1.1"}
            return copy.deepcopy(self.existing)
        if method == "GET" and path.endswith("/releases/7"):
            result = copy.deepcopy(self.existing)
            if self.publish_before_upload:
                result["draft"] = False
            return result
        if method == "POST" and path.startswith("/upload/"):
            name = draft_release.urllib.parse.parse_qs(draft_release.urllib.parse.urlsplit(path).query)["name"][0]
            if self.fail_once and len(self.existing["assets"]) == 1:
                self.fail_once = False
                raise RuntimeError("simulated upload connection failure")
            self.writes.append((method, path, data))
            asset = {"name": name, "digest": f"sha256:{release.sha256(data)}", "size": len(data)}
            self.existing["assets"].append(asset)
            return asset
        raise AssertionError(f"unexpected API request: {method} {path}")


class DraftTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        bundle(self.directory)
        self.api = FakeGitHub()

    def create(self):
        return draft_release.create_draft(self.api, self.directory, "0.1.1", REVISION, "Release notes")

    def test_only_an_unpublished_preview_is_created(self):
        self.api.annotated = True
        self.assertIn("v0.1.1", self.create())
        metadata = self.api.writes[0][2]
        self.assertTrue(metadata["draft"])
        self.assertTrue(metadata["prerelease"])
        self.assertEqual(metadata["make_latest"], "false")
        self.assertEqual(metadata["target_commitish"], REVISION)
        self.assertEqual(len(self.api.existing["assets"]), 5)
        self.assertEqual({a["name"] for a in self.api.existing["assets"]}, {release.archive_name("0.1.1", target) for target in release.TARGETS} | {"SHA256SUMS"})

    def test_identical_retry_is_idempotent(self):
        self.create()
        writes = len(self.api.writes)
        self.create()
        self.assertEqual(len(self.api.writes), writes)

    def test_partial_upload_can_resume_without_duplicate_assets(self):
        self.api.fail_once = True
        with self.assertRaisesRegex(RuntimeError, "simulated upload"):
            self.create()
        self.create()
        self.assertEqual(len(self.api.existing["assets"]), 5)
        self.assertEqual(len(self.api.writes), 6)

    def test_published_or_non_preview_releases_are_untouched(self):
        self.create()
        for field in ["draft", "prerelease"]:
            self.api.existing[field] = False
            writes = len(self.api.writes)
            with self.assertRaisesRegex(ValueError, "refusing to modify"):
                self.create()
            self.assertEqual(len(self.api.writes), writes)
            self.api.existing[field] = True

    def test_retargeted_tag_is_rejected_before_writes(self):
        self.api.tag_revision = "c" * 40
        with self.assertRaisesRegex(ValueError, "tested commit"):
            self.create()
        self.assertEqual(self.api.writes, [])

    def test_existing_asset_with_different_bytes_is_never_overwritten(self):
        self.create()
        self.api.existing["assets"][0]["digest"] = "sha256:" + "0" * 64
        writes = len(self.api.writes)
        with self.assertRaisesRegex(ValueError, "different bytes"):
            self.create()
        self.assertEqual(len(self.api.writes), writes)

    def test_publication_while_pending_aborts_uploads(self):
        self.api.publish_before_upload = True
        with self.assertRaisesRegex(ValueError, "published while"):
            self.create()
        self.assertEqual(len(self.api.writes), 1)

    def test_invalid_bundle_never_creates_a_draft(self):
        next(self.directory.glob("*.tar.gz")).unlink()
        with self.assertRaisesRegex(ValueError, "all four"):
            self.create()
        self.assertEqual(self.api.writes, [])

    def test_non_tag_invocations_cannot_publish(self):
        with mock.patch.dict("os.environ", {"GITHUB_EVENT_NAME": "workflow_dispatch", "GITHUB_REF": "refs/heads/main"}, clear=True), mock.patch.object(sys, "argv", ["draft_release.py", str(self.directory)]):
            with self.assertRaisesRegex(ValueError, "matching version-tag push"):
                draft_release.main()


class TransportTests(unittest.TestCase):
    def test_only_explicit_404_is_treated_as_missing(self):
        client = draft_release.GitHub("owner/project", "test-token")
        for code in [401, 403, 500]:
            with self.subTest(code=code):
                error = urllib.error.HTTPError("https://api.github.com/test", code, "failure", {}, io.BytesIO(b"real upstream error"))
                with mock.patch("urllib.request.build_opener") as opener:
                    opener.return_value.open.side_effect = error
                    with self.assertRaisesRegex(RuntimeError, f"HTTP {code}: real upstream error"):
                        client.request("GET", "/test", missing_ok=True)
        error = urllib.error.HTTPError("https://api.github.com/test", 404, "missing", {}, io.BytesIO(b"not found"))
        with mock.patch("urllib.request.build_opener") as opener:
            opener.return_value.open.side_effect = error
            self.assertIsNone(client.request("GET", "/test", missing_ok=True))

    def test_redirects_are_not_followed_with_the_token(self):
        self.assertIsNone(draft_release.NoRedirect().redirect_request(None, None, 302, "redirect", {}, "https://another-host.invalid"))


if __name__ == "__main__":
    unittest.main()
