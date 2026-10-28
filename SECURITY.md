# Security

OmaPhoenix handles SSH keys, credentials and root-only system files, so security reports get priority.

## Reporting a problem

**Please don't open a public issue.** Report it privately through GitHub: [Security → Report a vulnerability](https://github.com/sumdahl/omaphoenix/security/advisories/new). Include what you saw, how to reproduce it, and the output of `omaphoenix --version`. Leave out real keys, tokens and snapshot contents.

We'll answer within a week, agree on a fix and a date with you, and credit you in the release notes unless you'd rather not be named.

## Supported versions

Only the latest release gets security fixes.

## What's in scope

- Anything that sends snapshot data somewhere other than the user's own repo.
- Secrets reaching a snapshot unencrypted, or the vault being decryptable without the passphrase.
- A restore or a fix running something the user didn't confirm, or writing outside what the dry run showed.
- The installer accepting a download that doesn't match the release's `SHA256SUMS`.
- The audit missing or misreporting a check in a way that hides a real risk.

## How the product is built to be safe

- Snapshots go only to the user's private git repo; there is no OmaPhoenix server.
- SSH keys and credentials are encrypted with [age](https://age-encryption.org) and a passphrase before they leave the machine.
- Every snapshot is audited first; files that look like they hold secrets are left out.
- The audit only reads; a fix runs only when the user picks it and confirms the exact command.
- Restores keep the files they replace, and `omaphoenix undo` reverses them.
- Releases are built by GitHub Actions, with `SHA256SUMS` and build provenance (`gh attestation verify`).
