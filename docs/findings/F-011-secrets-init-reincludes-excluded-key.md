# F-011: `secrets init` can include a key you excluded

| | |
|---|---|
| Product | OmaPhoenix |
| Version | 0.1.0-rc.7 (and rc.8 as planned) |
| Found | 2026-10-10, Manish's review of #21 |
| Severity | low: needs a deliberate exclusion followed by a second `secrets init` |
| Status | open |

## What happens

You untick one private key (say `.ssh/id_work`) in Backup → secret and save. Later, `omaphoenix secrets init` runs again, for example on a reinstall. Because not every key is selected, it appends `secret:.ssh/*` to `selection.conf`. Rules are last-match-wins, so the wildcard comes after your exclusion and includes `id_work` again.

## Reproduce

```sh
h=$(mktemp -d); export HOME=$h OMAPHOENIX_REPO=$h/repo OMAPHOENIX_AUDIT=off
mkdir -p $h/.ssh && ssh-keygen -q -t ed25519 -N '' -f $h/.ssh/id_a && ssh-keygen -q -t ed25519 -N '' -f $h/.ssh/id_work
omaphoenix init && OMAPHOENIX_PASSPHRASE=fake-test-passphrase-123 omaphoenix secrets init
# untick .ssh/id_work in: omaphoenix select, then w
rm -r $h/repo/secrets
OMAPHOENIX_PASSPHRASE=fake-test-passphrase-123 omaphoenix secrets init
tail -2 $h/repo/selection.conf    # BUG: "secret:.ssh/*" comes after the exclusion
```

## Cause

`App::init_secrets` (`crates/phx-core/src/app/mod.rs`) appends `secret:.ssh/*` unless every key is already selected. It can't tell a key nobody chose from one you excluded. The rc.7 code behaved the same.

## Fix

Use `reviewed.txt`. Add rules only for keys the user hasn't reviewed (`secret:<id>`, one per key, as Backup writes them), never a wildcard. A reviewed, unticked key stays out.

Test (phx-core, `app`): `reviewed.txt` lists `.ssh/id_work`, and the rules exclude it. After `init_secrets`, `rules.selected("secret", ".ssh/id_work")` is false. Fails today.
