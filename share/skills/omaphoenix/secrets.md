# Encrypted Secrets

SSH keys are not backed up until the user opts in:

```bash
omaphoenix secrets init
```

It asks for a passphrase twice (at least 12 characters) and adds `~/.ssh` to snapshots. Private
keys are encrypted with it; public halves (`*.pub`, `config`, `known_hosts`, `authorized_keys`)
are stored plain.

- **The passphrase is never stored.** Lost passphrase = lost encrypted keys, nothing else.
- Let the user type it. Never ask them to paste it into chat, never put it on a command line.
- `OMAPHOENIX_PASSPHRASE` exists for scripts and tests only. Don't set it in the user's shell.
- Restoring the `secret` category asks for the passphrase. Without a terminal it fails with a
  message instead of hanging.

Never print key contents, even to check them. Check presence with `ls -l ~/.ssh`.
