# claude-config

Personal Claude Code settings, skills, and agents.

## Install the status line only

Requires Bash, `jq`, Python 3, and a terminal font with Powerline glyphs.
Git is optional; without it, the branch label is omitted.

```sh
./setup-statusline.sh
```

This replaces only `statusLine` in `${CLAUDE_CONFIG_DIR:-$HOME/.claude}/settings.json`.
Other settings, including hooks and permissions, are preserved. Re-running it is
safe. The command points at `statusline.py` in this checkout; keep the checkout in
place, or rerun the installer after moving it.

The one-row Catppuccin Mocha Powerline footer shows:

- Current model and reasoning effort, when Claude supplies it.
- `↑` input and `↓` output tokens **in the current context**, not cumulative session
  totals. Input includes fresh input, cache writes, and cache reads.
- A ten-cell context bar labelled with **used input tokens only**: no percentage
  or printed maximum. Full scale is Claude's reported `context_window_size`, not
  an independently inferred compaction threshold. Output is excluded from the
  bar, matching Claude's occupancy calculation.
- Current directory basename and Git branch when space permits. These disappear
  first on narrower terminals; very narrow terminals use clipped compact text.

Startup/no usage shows a green empty bar, `▱▱▱▱▱▱▱▱▱▱ 0`, like Pi.
Nonzero usage with unknown occupancy still shows `░░░░░░░░░░ ?`.
Context colors turn peach at 70% and red at 90%.

Claude's **native permission-mode indicator remains below this row**. Claude
2.1.318 does not send the live mode to custom status lines; no hook tracking or
potentially stale permission badge is added. The script does not read transcripts
or change permission settings. Token semantics follow that version's
[documented status-line data](https://code.claude.com/docs/en/statusline).

Claude picks up changes on its next status-line update, such as a permission-mode
change. Restart Claude if needed. `disableAllHooks: true` or managed hook policy
can prevent custom status lines from running.

## Install all personal settings

```sh
./setup-settings.sh
```

This also installs the same status line, then merges the other personal defaults
and local plugin marketplace. **Unlike the status-line-only installer, it changes
model, effort, permissions, and other preferences**, including enabling permission
bypass. Unrelated settings and existing hooks remain intact.

## Verify

```sh
python3 -B -m unittest discover -s tests -v
bash -n setup-settings.sh setup-statusline.sh
```

Tests exercise JSON stdin → rendered stdout and installers against temporary
settings, including quoted checkout paths and repeat installation. They do not
modify your Claude configuration.
