# F-008: `doctor` says the SSH keys aren't backed up after saving in Backup

| | |
|---|---|
| Product | OmaPhoenix (`doctor`, Pulse, the login health notification) |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, `omaphoenix doctor` after reviewing in Backup on the reference laptop |
| Severity | medium: a false alarm about the most important item in the backup, and its advice ends in an error |
| Status | fixed in 0.1.0-rc.8; test `phx_core::doctor::tests::secrets_are_judged_by_each_real_key` |

## What happens

The private keys are in the snapshot, encrypted in `secrets/vault.age`, and a restore dry run lists them. Yet:

```
! secrets   3 SSH private key(s) are not in snapshots; a reinstall means new keys everywhere. Run: omaphoenix secrets init
```

Following the advice gives `error: secrets are already set up`. That's safe, because the key is never replaced, but it is a dead end.

## Reproduce

```sh
omaphoenix secrets init        # selection.conf gets: secret:.ssh/*
omaphoenix doctor | grep secrets            # ✓ SSH keys are in snapshots, encrypted
omaphoenix select              # just press w, then q
grep '^secret' ~/.local/share/omaphoenix/repo/selection.conf   # now one line per real file
omaphoenix doctor | grep secrets            # BUG: "3 SSH private key(s) are not in snapshots"
```

## Cause

`check_secrets` (`crates/phx-core/src/doctor/mod.rs`) decides with a made-up name:

```rust
self.rules().is_ok_and(|r| r.selected("secret", ".ssh/id_x"))
```

1. `secrets init` writes `secret:.ssh/*`, which matches `.ssh/id_x`.
2. Saving in Backup rebuilds the rules (`selection::build`). `secret` is opt-in (`OPT_IN`), so build writes one rule per ticked item (`secret:.ssh/aur`, `secret:.ssh/id_personal`, …) and never a wildcard.
3. The made-up `.ssh/id_x` no longer matches, so the check fails.

`App::init_secrets` (`app/mod.rs`) uses the same probe to decide whether to append `secret:.ssh/*`.

## Workaround

None needed: the keys are backed up. To confirm:

```sh
omaphoenix restore --dry-run | grep decrypt      # lists every private key
```

## Fix

Count the real private keys (`secret::private_ssh_keys` already finds them) and check each by its own id: `r.selected("secret", &format!(".ssh/{name}"))`. Then report one of:

- `✓ SSH keys are in snapshots, encrypted` when all are selected and secrets are ready;
- `! N of M SSH private key(s) are not in snapshots (name, …). Tick them in Backup → secret` when some are unticked;
- the current `secrets init` advice only when `secrets_ready` is false.

Use the same per-key check in `init_secrets` instead of the probe.

Tests (phx-core, `doctor`):

- A fake home with two keys, secrets set up, and rules naming both: assert `ok`. Fails today.
- Rules naming one of the two: assert a warning that names the other.
- Not set up: assert the `secrets init` advice.
