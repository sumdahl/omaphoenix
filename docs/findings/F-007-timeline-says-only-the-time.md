# F-007: The timeline says when each snapshot was taken, not what changed

| | |
|---|---|
| Product | OmaPhoenix (CLI `list`, TUI Timeline, desktop Timeline) |
| Version | 0.1.0-rc.7 |
| Found | 2026-10-10, choosing a snapshot to roll back to on the reference laptop |
| Severity | low: usability; nothing is wrong, but picking a rollback point means guessing |
| Status | fixed in 0.1.0-rc.8; test `phx_core::app::summary::tests::names_what_was_added_and_removed` |

## What happens

```
884ae58  6m ago    snapshot 2026-10-10T15:36:36+05:45
c528752  10m ago   snapshot 2026-10-10T15:32:54+05:45
8a67d3e  18m ago   snapshot 2026-10-10T15:25:22+05:45
```

Each snapshot's description is only its time. Nothing says that:

- `c528752` added `.config/phx-demo`;
- `884ae58` removed it and added the snapshot timer.

To find the snapshot before a change, the user has to preview them one by one.

## Reproduce

```sh
omaphoenix snapshot; touch ~/.config/phx-x.conf; omaphoenix snapshot; rm ~/.config/phx-x.conf
omaphoenix list            # both lines say only "snapshot <time>"
```

## Cause

`App::snapshot` commits with `format!("snapshot {}", rfc3339_now())` (`crates/phx-core/src/app/snapshot.rs`). The manifest changes it already writes aren't summarised anywhere.

## Fix

1. At commit time, compare the new `manifest.json` with the previous one (the move-aside already has it).
2. Write a short summary into the commit message's subject, keeping the time as the first word so `list` stays sortable and Go-readable. For example:

   ```
   snapshot 2026-10-10T15:36:36+05:45 · +1 service · -1 config
   ```

   Put the item names in the body, up to ~20.
3. Show the subject in `list` and both Timelines.

Done as above, except the body is not shown in the preview yet. It is in the snapshot repo's history (`git -C ~/.local/share/omaphoenix/repo log`). Add it to the preview when someone needs it.

The first snapshot reads `· first snapshot`. Package version upgrades don't count, since the manifest has names only.

Test (phx-core, `app`): snapshot a fake home, add one config file and remove another, then snapshot again. Assert the subject contains `+1 config` and `-1 config`, and the body names both.
