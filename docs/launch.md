# Launch: v0.1.0

## Order (ADR-0024)

1. Merge the last fixes into `main`.
2. Make the repo public; turn on **Private vulnerability reporting** (Settings → Code security) for SECURITY.md.
3. Tag `v0.1.0`; the release workflow builds it, with `SHA256SUMS` and build provenance.
4. Install it on a real laptop with the README's `curl … | bash`, upgrading an old install: `omaphoenix --version` says 0.1.0, the `secaudit` commands are gone, `omaphoenix audit tui` shows the old scan history, the timer still runs.
5. `gh attestation verify omaphoenix_linux_amd64.tar.gz -R sumdahl/omaphoenix` passes on the downloaded tarball.
6. Post.

## Posts

Every claim comes from our own tests (docs/14). Fill in the two handles before posting; don't guess them.

**Main post, option A**

> Reinstalled Omarchy and lost your whole setup? Never again.
>
> OmaPhoenix: one command snapshots your Omarchy (packages, AUR, plugins, themes, configs, services, encrypted SSH keys).
>
> Fresh install → one command → your desktop is back.
>
> Built in Rust 🦀 @dhh @OMARCHY_HANDLE

**Main post, option B**

> I built OmaPhoenix 🔥 for Omarchy:
>
> • snapshots your whole setup to your own private GitHub repo
> • audits it for leaked secrets before every backup
> • restores it on a fresh install with one command
>
> Free, open source, Rust. @dhh @OMARCHY_HANDLE

**Thread**

> 2/ How it works:
> 1. curl install, then pick "back up": a private repo on your GitHub
> 2. choose what to back up (it asks, never guesses)
> 3. snapshots on a schedule and after every omarchy update
> 4. new laptop: install, pick "restore"
>
> Undo is one command too.

> 3/ The part I care most about: every snapshot is security-audited first.
>
> Leaked tokens, readable keys, sketchy autostarts: left out of the backup instead of copied to your next machine.
>
> Audit engine by @MANISH_HANDLE 🙏

> 4/ Tested end to end before release: fresh Omarchy 4.0.6 VM → full restore of a real daily-driver setup (221 packages, 41 plugins, 57 themes, 1,400+ config files) in about an hour, under 10 MB of RAM.
>
> All in the terminal.
>
> github.com/sumdahl/omaphoenix

**Picture:** the README's before/after pair (docs/img).
