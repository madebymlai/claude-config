#!/usr/bin/env bash
# Merge laimk's Claude Code settings into the user settings file
# ($CLAUDE_CONFIG_DIR/settings.json, else ~/.claude/settings.json).
# The keys below overwrite the same keys there; every other key, hooks
# included, is kept. Also installs the shared Powerline status line. Safe to run again.
set -euo pipefail

command -v jq >/dev/null || { echo "setup-settings: jq is required" >&2; exit 1; }

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
config_dir=${CLAUDE_CONFIG_DIR:-$HOME/.claude}
settings=$config_dir/settings.json

"$repo/setup-statusline.sh"

ours=$(cat <<'JSON'
{
  "env": {
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "240000",
    "CLAUDE_CODE_MAX_SUBAGENT_SPAWN_DEPTH": "1"
  },
  "attribution": {
    "commit": "",
    "pr": ""
  },
  "permissions": {
    "defaultMode": "bypassPermissions"
  },
  "skipDangerousModePermissionPrompt": true,
  "model": "opus",
  "effortLevel": "xhigh",
  "modelSettings": {
    "claude-opus-5-5": {
      "effortLevel": "xhigh"
    }
  },
  "worktree": {
    "bgIsolation": "none",
    "baseRef": "head"
  },
  "enableArtifact": false,
  "enableWorkflows": false,
  "workflowKeywordTriggerEnabled": false,
  "enabledPlugins": {
    "laimk@claude-config": true
  },
  "outputStyle": "Concise",
  "promptSuggestionEnabled": false,
  "awaySummaryEnabled": false,
  "dialogExpiry": "never",
  "tui": "fullscreen",
  "autoMemoryEnabled": false,
  "autoCompactEnabled": false,
  "switchModelsOnFlag": false,
  "autoContinueAtUsageLimit": false,
  "remoteControlAtStartup": false,
  "agentPushNotifEnabled": true,
  "useAutoModeDuringPlan": false,
  "feedbackDrafts": "off"
}
JSON
)

mkdir -p "$config_dir"
[[ -f $settings ]] || echo '{}' >"$settings"

tmp=$(mktemp "$settings.XXXXXX")
trap 'rm -f "$tmp"' EXIT

# The claude-config marketplace points at this checkout, wherever it is.
jq --argjson ours "$ours" --arg repo "$repo" '
  . * $ours
  * {extraKnownMarketplaces: {"claude-config": {source: {source: "directory", path: $repo}}}}
' "$settings" >"$tmp"
mv "$tmp" "$settings"

echo "setup-settings: merged into $settings"
