# F-005: Restoring one item asks for the secrets passphrase anyway

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, rolling back a single config item on the reference laptop |
| Severity | medium: no wrong files are written, but it asks for the most sensitive input when it isn't needed. Without a terminal (desktop app, scripts) such a restore would fail. |
| Status | fixed in 0.1.0-rc.8; test `phx_core::category::secret::tests::restoring_other_items_never_asks_for_the_passphrase` |

## What happens

```
❯ omaphoenix restore .config/phx-demo --at c528752 --dry-run --no-restart
✓ config    applied 0 · already present 0 · planned 1
    + overwrite ~/.config/phx-demo/note.conf
✓ secret    applied 0 · already present 0 · planned 3
    + decrypt ~/.ssh/aur
    + decrypt ~/.ssh/id_work
    + decrypt ~/.ssh/id_personal

❯ omaphoenix restore .config/phx-demo --at c528752 --no-restart
▸ secret
secrets passphrase:
✓ secret    applied 0 · already present 0
```

Only `.config/phx-demo` was asked for. The SSH keys are out of scope, yet:

- the dry run plans to decrypt them;
- the real run asks for the passphrase, decrypts the vault, and then (correctly) writes nothing.

## Reproduce

Needs `omaphoenix secrets init` done once.

```sh
mkdir -p ~/.config/phx-demo && echo v1 > ~/.config/phx-demo/note.conf
omaphoenix snapshot
echo v2 > ~/.config/phx-demo/note.conf
omaphoenix restore .config/phx-demo --dry-run --no-restart   # BUG: "+ decrypt ~/.ssh/..."
omaphoenix restore .config/phx-demo --no-restart             # BUG: asks for the passphrase
rm -r ~/.config/phx-demo
```

## Cause

`Secret::restore` (`crates/phx-core/src/category/secret/mod.rs`):

- In a dry run, it plans every name in `vault.txt`.
- Otherwise, it calls `open_vault`, which asks for the passphrase.

The item filter, `Env::want` (`category/env.rs`), is applied only per entry inside `open_vault` (`secret/vault.rs`), after decryption.

## Workaround

Pass `--only config` (or the right category) along with the item. Or press Ctrl+C at the prompt: the other stages have already run, and only the secret stage is skipped.

## Fix

Before planning or opening the vault:

1. Filter the names in `vault.txt` through `env.want(name)`.
2. If none is wanted, skip the vault: no plan lines, no prompt.
3. In a dry run, plan only the wanted names.

This combines with F-003, which compares against `vault.sha256` instead of planning blindly.

Tests (phx-core, `category/secret`):

- Restore a non-secret item with a vault present and a passphrase callback that fails the test if it is called. Assert it is never called and nothing is planned.
- Restore `.ssh/id_x` by name and assert the callback is called.

The first fails today.
