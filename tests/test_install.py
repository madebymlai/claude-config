import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.home = self.directory / "home"
        self.home.mkdir()
        self.env = {**os.environ, "HOME": str(self.home), "COLUMNS": "120"}
        self.env.pop("CLAUDE_CONFIG_DIR", None)

    def install(self, script):
        result = subprocess.run(
            ["bash", str(script)], cwd=self.directory,
            env=self.env, text=True, capture_output=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_standalone_installer_preserves_settings_and_is_repeat_safe(self):
        # Both shell quoting and config-dir quoting are observable by executing
        # the installed command, not by requiring a particular command string.
        checkout = self.directory / "checkout with spaces ' and $dollars"
        checkout.mkdir()
        for name in ("setup-statusline.sh", "statusline.py"):
            shutil.copy2(ROOT / name, checkout / name)
        for existing in (False, True):
            with self.subTest(existing=existing):
                config = self.home / ".claude"
                original = {}
                if existing:
                    config = self.directory / "config override ' $"
                    config.mkdir()
                    self.env["CLAUDE_CONFIG_DIR"] = str(config)
                    original = {
                        "model": "sonnet", "env": {"KEEP_ME": "yes"},
                        "hooks": {"Stop": [{"hooks": [{"type": "command", "command": "true"}]}]},
                        "statusLine": {"type": "command", "command": "old", "padding": 12},
                    }
                    (config / "settings.json").write_text(json.dumps(original))
                self.install(checkout / "setup-statusline.sh")
                settings = config / "settings.json"
                first = settings.read_bytes()
                installed = json.loads(first)
                self.assertEqual(
                    {key: value for key, value in installed.items() if key != "statusLine"},
                    {key: value for key, value in original.items() if key != "statusLine"},
                )
                self.assertEqual(installed["statusLine"]["type"], "command")
                self.assertEqual(installed["statusLine"].get("padding", 0), 0)
                rendered = subprocess.run(
                    ["sh", "-c", installed["statusLine"]["command"]], input="{}",
                    text=True, capture_output=True, env=self.env, cwd=self.directory,
                )
                self.assertEqual(rendered.returncode, 0, rendered.stderr)
                self.assertIn("Claude", rendered.stdout)
                self.assertIn("░░░░░░░░░░ ?", rendered.stdout)
                self.install(checkout / "setup-statusline.sh")
                self.assertEqual(settings.read_bytes(), first)

    def test_full_settings_installer_uses_same_footer_without_losing_hooks(self):
        config = self.home / ".claude"
        config.mkdir()
        settings = config / "settings.json"
        hooks = {"Stop": [{"hooks": [{"type": "command", "command": "true"}]}]}
        settings.write_text(json.dumps({"hooks": hooks, "env": {"KEEP_ME": "yes"}}))
        self.install(ROOT / "setup-settings.sh")
        first = settings.read_bytes()
        installed = json.loads(first)
        rendered = subprocess.run(
            ["sh", "-c", installed["statusLine"]["command"]], input="{}",
            text=True, capture_output=True, env=self.env, cwd=self.directory,
        )
        self.assertEqual(rendered.returncode, 0, rendered.stderr)
        self.assertIn("Claude", rendered.stdout)
        self.assertIn("░░░░░░░░░░ ?", rendered.stdout)
        self.assertEqual(installed["hooks"], hooks)
        self.assertEqual(installed["env"]["KEEP_ME"], "yes")
        self.assertEqual(installed["model"], "opus")
        self.install(ROOT / "setup-settings.sh")
        self.assertEqual(settings.read_bytes(), first)


if __name__ == "__main__":
    unittest.main()
