from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import release


class DemoTests(unittest.TestCase):
    def test_readme_shows_the_actual_checked_demo_output(self):
        text = (release.ROOT / "README.md").read_text(encoding="utf-8")
        block = text.split("<!-- demo-output:start -->\n```jsonc\n", 1)[1].split("```\n<!-- demo-output:end -->", 1)[0]
        self.assertEqual(block, (release.ROOT / "examples/launch/expected.jsonc").read_text(encoding="utf-8"))

    def test_golden_keeps_order_edit_and_both_comments(self):
        base = (release.ROOT / "examples/launch/base.jsonc").read_text(encoding="utf-8")
        ours = (release.ROOT / "examples/launch/ours.jsonc").read_text(encoding="utf-8")
        theirs = (release.ROOT / "examples/launch/theirs.jsonc").read_text(encoding="utf-8")
        golden = (release.ROOT / "examples/launch/expected.jsonc").read_text(encoding="utf-8")
        self.assertEqual(theirs, base.replace("src/api.js", "dist/api.js"))
        self.assertEqual(golden, ours.replace("src/api.js", "dist/api.js"))
        self.assertLess(golden.index('"name": "Worker"'), golden.index('"name": "API"'))
        self.assertLess(golden.index("// Run the background worker."), golden.index('"name": "Worker"'))
        self.assertLess(golden.index("// Start the API locally."), golden.index('"name": "API"'))


if __name__ == "__main__":
    unittest.main()
