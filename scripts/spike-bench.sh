#!/usr/bin/env bash
# GUI-stack spike bench (issue #12): run the desktop app in bench mode on the
# real Hyprland session, swap a scratch copy of the theme mid-run, screenshot
# the window, and print the frame/continuity/theme report.
# Never touches the user's real theme: the app reads a scratch copy.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
bin=$root/target/release/omaphoenix-desktop
out=${1:-$root/target/spike}
mkdir -p "$out"
current=${XDG_STATE_HOME:-$HOME/.local/state}/omarchy/current/theme/colors.toml
mine=$(sed -n 's/^accent *= *"\(.*\)"/\1/p' "$current")
other=""
for f in "$HOME"/.config/omarchy/themes/*/colors.toml; do
  a=$(sed -n 's/^accent *= *"\(.*\)"/\1/p' "$f")
  if [[ -n $a && $a != "$mine" ]]; then other=$f; break; fi
done
[[ -n $other ]] || { echo "need a second Omarchy theme with a different accent"; exit 1; }

cp "$current" "$out/colors.toml"
rm -f "$out/report.json"
OMAPHOENIX_SPIKE_BENCH=1 OMAPHOENIX_SPIKE_THEME=$out/colors.toml OMAPHOENIX_SPIKE_REPORT=$out/report.json \
  "$bin" >"$out/app.log" 2>&1 &
app=$!

sleep 3
geo=$(hyprctl clients -j | jq -r '.[] | select(.title == "OmaPhoenix") | "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"' | sed -n 1p)
[[ -n $geo ]] && grim -g "$geo" "$out/pulse.png" || echo "no window found for screenshot"
sleep 7
cp "$other" "$out/colors.toml" # live theme switch, as omarchy-theme-set would
echo "theme swapped to $(basename "$(dirname "$other")")"

for _ in $(seq 60); do kill -0 "$app" 2>/dev/null || break; sleep 0.5; done
kill "$app" 2>/dev/null || true
[[ -f $out/report.json ]] || { echo "no report; app log:"; tail -20 "$out/app.log"; exit 1; }
want=$(sed -n 's/^accent *= *"\(.*\)"/\1/p' "$other")
jq --arg want "$want" '. + {theme_expected_accent: $want, theme_ok: (.theme.accent == $want)}' "$out/report.json" >"$out/report.tmp"
mv "$out/report.tmp" "$out/report.json"
cat "$out/report.json"
