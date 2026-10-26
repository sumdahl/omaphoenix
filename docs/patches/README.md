# Proposed changes to Manish's secaudit

Patches here are **proposals**. Nothing is committed to `mnshchtri/secaudit` without Manish's OK (AGENTS.md). The user forwards them.

## `secaudit-quirks-S1-S7.patch`
Fixes the seven Bash quirks the Rust port found (docs/10). It applies to `bin/secaudit` at `8a20e26`:

```sh
cd ~/personal/secaudit && git apply secaudit-quirks-S1-S7.patch
```

| # | Change | Visible effect on the reference machine |
|---|---|---|
| S1 | `${x/#"$target_home"/\~}`: quote the pattern and escape the `~` | paths print as `~/…` again (Bash 5.2+ was expanding the `~` back to the home path) |
| S2 | `stat -Lc` in `check_perm` | none here; a symlinked credential file is judged by its target's mode |
| S3 | unreadable history file → `skip` | none here |
| S4 | autologin text normalised before `, getty` is added | none here (`User=xgetty` → `User=x, getty`) |
| S5 | `NF>1 && $2==""` for empty passwords | none here |
| S6 | hook `name -> exe` lines attach to the "Custom pacman hooks" note | none here |
| S7 | hypridle timeout read only from `timeout =` lines | none here (`Idle lock after loginctls` → the real seconds) |

**Verified:** `bash -n` passes. Run on the reference machine before and after, the findings differ only by S1's `~` paths.

**After it lands:** switch the Rust port's S2–S7 to the fixed behaviour (it reproduces the quirks today, for parity), and drop S1's normalisation in `scripts/audit-parity`.
