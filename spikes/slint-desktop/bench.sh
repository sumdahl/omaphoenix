#!/usr/bin/env bash
# Same bench as scripts/spike-bench.sh (Tauri), for the Slint spike.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=$here/target/bench
mkdir -p "$out"
current=${XDG_STATE_HOME:-$HOME/.local/state}/omarchy/current/theme/colors.toml
mine=$(sed -n 's/^accent *= *"\(.*\)"/\1/p' "$current")
other=""
for f in "$HOME"/.config/omarchy/themes/*/colors.toml; do
  a=$(sed -n 's/^accent *= *"\(.*\)"/\1/p' "$f")
  if [[ -n $a && $a != "$mine" ]]; then other=$f; break; fi
done
cp "$current" "$out/colors.toml"
rm -f "$out/report.json"
OMAPHOENIX_SPIKE_BENCH=1 OMAPHOENIX_SPIKE_THEME=$out/colors.toml OMAPHOENIX_SPIKE_REPORT=$out/report.json \
  "$here/target/release/slint-desktop-spike" >"$out/app.log" 2>&1 &
app=$!
sleep 3
geo=$(hyprctl clients -j | jq -r '.[] | select(.title == "OmaPhoenix") | "\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"' | sed -n 1p)
[[ -n $geo ]] && grim -g "$geo" "$out/pulse.png" || echo "no window for screenshot"
sleep 7
cp "$other" "$out/colors.toml"
for _ in $(seq 60); do kill -0 "$app" 2>/dev/null || break; sleep 0.5; done
kill "$app" 2>/dev/null || true
[[ -f $out/report.json ]] || { echo "no report"; tail -20 "$out/app.log"; exit 1; }
want=$(sed -n 's/^accent *= *"\(.*\)"/\1/p' "$other")
jq --arg want "$want" '. + {theme_expected_accent: $want, theme_ok: (.theme.accent == $want)}' "$out/report.json" >"$out/r.tmp" && mv "$out/r.tmp" "$out/report.json"
cat "$out/report.json"
