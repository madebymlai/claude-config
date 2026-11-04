import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
SGR = re.compile(r"\x1b\[[0-9;]*m")


class StatusLineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name)
        self.env = {**os.environ, "COLUMNS": "120", "COLORTERM": "truecolor"}
        # Documented Claude payload: 10k fresh + 20k written + 50k read = 80k
        # context input. Output is separate; occupancy is 80k / 200k = 40%.
        self.data = {
            "model": {"display_name": "Opus 5.5", "id": "claude-opus-5-5"},
            "effort": {"level": "high"},
            "context_window": {
                "total_input_tokens": 80000,
                "total_output_tokens": 2000,
                "context_window_size": 200000,
                "used_percentage": 40,
                "current_usage": {
                    "input_tokens": 10000,
                    "cache_creation_input_tokens": 20000,
                    "cache_read_input_tokens": 50000,
                    "output_tokens": 2000,
                },
            },
        }

    def render(self, data=None, columns=120):
        result = subprocess.run(
            [sys.executable, str(ROOT / "statusline.py")],
            input=json.dumps(self.data if data is None else data),
            text=True, capture_output=True, cwd=self.directory,
            env={**self.env, "COLUMNS": str(columns)},
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        return result.stdout

    def test_model_effort_current_tokens_and_input_only_occupancy(self):
        line = self.render()
        self.assertEqual(
            SGR.sub("", line),
            " Opus 5.5 high  ↑80k ↓2.0k  ▰▰▰▰▱▱▱▱▱▱ 80k \n",
        )
        self.assertIn("\x1b[38;2;30;30;46m\x1b[48;2;203;166;247m", line)
        self.assertIn("\x1b[38;2;166;227;161m\x1b[48;2;69;71;90m", line)
        self.assertTrue(line.endswith("\x1b[0m\n"))

    def test_startup_and_missing_fields_show_green_empty_bar_and_zero(self):
        self.data["context_window"].update(
            total_input_tokens=0, total_output_tokens=0,
            current_usage=None, used_percentage=None,
        )
        self.data.pop("effort")
        line = self.render()
        self.assertEqual(
            SGR.sub("", line),
            " Opus 5.5  ↑0 ↓0  ▱▱▱▱▱▱▱▱▱▱ 0 \n",
        )
        self.assertIn("\x1b[38;2;166;227;161m\x1b[48;2;69;71;90m", line)
        self.assertEqual(
            SGR.sub("", self.render({})),
            " Claude  ↑0 ↓0  ▱▱▱▱▱▱▱▱▱▱ 0 \n",
        )

    def test_effective_capacity_scales_clamps_and_colors_the_bar(self):
        for capacity, bar, color in [
            (100000, "▰▰▰▰▰▰▰▰▱▱", "250;179;135"),
            (50000, "▰▰▰▰▰▰▰▰▰▰", "243;139;168"),
            (0, "░░░░░░░░░░", "205;214;244"),
        ]:
            with self.subTest(capacity=capacity):
                self.data["context_window"]["context_window_size"] = capacity
                line = self.render()
                self.assertIn(bar, SGR.sub("", line))
                self.assertEqual(SGR.sub("", line).count("▰"), bar.count("▰"))
                self.assertIn(f"\x1b[38;2;{color}m\x1b[48;2;69;71;90m", line)
                self.assertNotIn("%", SGR.sub("", line))
                self.assertNotIn("/", SGR.sub("", line))

    def test_project_and_live_git_branch_use_payload_directory(self):
        repo = self.directory / "demo"
        repo.mkdir()
        subprocess.run(["git", "init", "-q", "-b", "topic", str(repo)], check=True)
        self.data["workspace"] = {"current_dir": str(repo), "project_dir": str(repo)}
        self.assertIn(" demo  topic ", SGR.sub("", self.render()))
        subprocess.run(
            ["git", "-C", str(repo), "symbolic-ref", "HEAD", "refs/heads/next"], check=True,
        )
        self.assertIn(" demo  next ", SGR.sub("", self.render()))
        self.data["workspace"]["current_dir"] = str(self.directory)
        self.assertNotIn("", SGR.sub("", self.render()))

    def test_narrow_terminals_drop_project_before_core_data_and_never_wrap(self):
        repo = self.directory / "demo"
        repo.mkdir()
        self.data["workspace"] = {"current_dir": str(repo)}
        line = SGR.sub("", self.render(columns=50))
        self.assertNotIn("demo", line)
        self.assertIn("Opus 5.5 high", line)
        self.assertIn("↑80k ↓2.0k", line)
        self.assertIn("▰▰▰▰▱▱▱▱▱▱ 80k", line)
        for columns in (0, 1, 4, 12, 24, 35, 48, 60):
            with self.subTest(columns=columns):
                line = SGR.sub("", self.render(columns=columns))
                self.assertEqual(line.count("\n"), 1)
                # Reserve four cells for Claude's built-in left/right margins.
                self.assertLessEqual(len(line.rstrip("\n")), max(0, columns - 4))
        self.assertEqual(
            SGR.sub("", self.render(columns=35)),
            "Opus 5.5 high ↑80k ↓2.0k 80k\n",
        )

    def test_unicode_clipping_counts_terminal_cells_not_characters(self):
        self.data["model"]["display_name"] = "模型" * 20
        self.assertEqual(SGR.sub("", self.render(columns=24)), "模型" * 5 + "\n")
        self.data["model"]["display_name"] = "e\u0301" * 30
        self.assertEqual(SGR.sub("", self.render(columns=24)), "e\u0301" * 20 + "\n")

    def test_external_labels_cannot_inject_terminal_controls_or_extra_rows(self):
        self.data["model"]["display_name"] = "\x1b[31mOpus\x1b[0m\n\x1b]0;bad title\x07\u202ex"
        repo = self.directory / "demo\nname\x1b[31m"
        repo.mkdir()
        self.data["workspace"] = {"current_dir": str(repo)}
        plain = SGR.sub("", self.render())
        self.assertEqual(
            plain,
            " Opus x high  ↑80k ↓2.0k  ▰▰▰▰▱▱▱▱▱▱ 80k  demo name \n",
        )


if __name__ == "__main__":
    unittest.main()
