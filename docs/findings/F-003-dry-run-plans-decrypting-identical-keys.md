# F-003: A dry run says it would decrypt keys that are already identical

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, `restore --dry-run` right after the first clean snapshot on the reference laptop |
| Severity | low: a misleading dry-run line; a real restore compares and writes nothing |
| Status | fixed in 0.1.0-rc.8; test `phx_core::category::secret::tests::a_dry_run_counts_identical_keys_as_present_without_the_passphrase` |

## What happens

On a machine that matches the snapshot exactly, every category reports `applied 0`, except:

```
✓ secret    applied 0 · already present 6 · planned 3
    + decrypt ~/.ssh/aur
    + decrypt ~/.ssh/id_work
    + decrypt ~/.ssh/id_personal
```

The three private keys on disk are identical to the ones in the vault, so nothing would change. A user reading the dry run can't tell that.

## Reproduce

```sh
omaphoenix secrets init       # once; any passphrase
omaphoenix snapshot
omaphoenix restore --dry-run  # "planned N · + decrypt ~/.ssh/..." for every encrypted key
```

## Cause

`Secret::restore` (`crates/phx-core/src/category/secret/mod.rs`) can't decrypt during a dry run, because it never asks for the passphrase there. So it lists every name in `vault.txt` as planned. The real run decrypts and compares per file (`vault.rs`), so it counts identical keys as present.

## Workaround

None needed. Read `+ decrypt` lines in a dry run as "would check, and write only if different".

## Fix

Without the passphrase:

1. Build the vault's plaintext tar from the files on disk. Use the same reproducible tar the snapshot builds (`vault.rs`: sorted, real modes, no times or owners) over the names in `vault.txt`.
2. Compare its SHA-256 with `vault.sha256`.
3. If they match, count them all as present.
4. Otherwise keep the `planned` lines, worded `would decrypt and restore ~/<name> (differs or missing)`.

Test (phx-core, `category/secret`): capture fake keys into a vault in a temp home, then dry-run restore into the same home. Assert `planned` is empty and `present` counts the keys. Change one key and assert it is planned. Fails today.
