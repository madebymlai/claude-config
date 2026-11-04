#!/usr/bin/env bash
# Install only the Powerline status line; preserve every unrelated setting.
# Like setup-settings.sh, this points at the checkout and is safe to run again.
set -euo pipefail

command -v jq >/dev/null || { echo "setup-statusline: jq is required" >&2; exit 1; }
command -v python3 >/dev/null || { echo "setup-statusline: python3 is required" >&2; exit 1; }

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
config_dir=${CLAUDE_CONFIG_DIR:-$HOME/.claude}
settings=$config_dir/settings.json

mkdir -p "$config_dir"
[[ -f $settings ]] || echo '{}' >"$settings"

tmp=$(mktemp "$settings.XXXXXX")
trap 'rm -f "$tmp"' EXIT

jq --arg script "$repo/statusline.py" '
  .statusLine = {
    type: "command",
    command: ("python3 " + ($script | @sh)),
    padding: 0
  }
' "$settings" >"$tmp"
mv "$tmp" "$settings"

echo "setup-statusline: installed in $settings"
