# DESIGN.md: the joint OmaPhoenix × Security Audit desktop app

**This file is the single source of truth for how the product looks, moves, speaks and behaves.**

- If code, a mockup or another doc disagrees with this file, this file wins. Change it here first, then change the code.
- It is framework-neutral on purpose: it states what a person must experience. The Rust UI stack is chosen in [docs/06-rust-rewrite.md](docs/06-rust-rewrite.md) against these requirements.
- **Scope:** both front ends, the **desktop app** and the **TUI** (ADR-0007). They are equals over one Rust core and share the same IA, voice, keymap, consent rules and states. §12 says how each section translates to the terminal.
- **Status:** v0.1 draft, 2026-10-09. The owners are Sumiran Dahal (OmaPhoenix) and Manish Karki (Security Audit). Changes need both.

---

## 1. What a person should feel

People open this app at two very different moments:

| Moment | How they arrive | How they must leave |
|---|---|---|
| **Everyday** (a glance, a snapshot, a scan) | Mildly curious, busy | Reassured in under 3 seconds: "I'm protected, nothing needs me." |
| **Migration** (old machine to fresh install) | Anxious. Their whole working life is on the line | In control, then relieved: "Everything came back, and nothing bad came with it." |

Security Audit already gives people **clarity without fear.** It names a problem, explains it in plain words, and offers a fix it never runs on its own.

OmaPhoenix already gives people **continuity without risk.** It keeps a timeline, previews every change, keeps the old copy, and never deletes.

The joint app keeps both feelings and adds one: **ceremony at the right moments.** A migration is a rare, meaningful event. It should feel like moving house with a careful friend, not like running a script.

### The emotional arc of a migration

```
 anxiety ──▶ understanding ──▶ confidence ──▶ trust ──▶ relief
 "what will   audit explains    preview shows   restore runs,  re-audit: "same
  I lose?"    what's risky      exactly what    honestly,      or better than
              and why           will happen     step by step   before"
```

Every screen in the migration flow must move the user one step right on this arc, never left. A surprise, a vague error, a fake progress bar or an unexplained root prompt moves them left. Those are bugs.

---

## 2. Principles

Ordered. When two conflict, the higher one wins.

1. **Honest before pleasant.** Never show success that didn't happen, never fake progress, never hide a failure behind a calm colour. (This comes from both codebases: OmaPhoenix's "no fake ok" rule, and secaudit's exit code 1 on any failure.)
2. **Nothing changes without consent; everything changed can be undone.** Preview first, apply second, undo always available. A fix, a restore, an upload: each is a deliberate act with a visible "before".
3. **Secrets are never shown.** Not in the UI, logs, toasts, copy buttons or screenshots. Show the file, never the value. There is no "reveal" button.
4. **Calm by default, loud only when it matters.** One risk worth acting on gets attention. Twenty informational notes do not.
5. **Feels physical.** Things have mass, start from where they were, follow your hand, and settle. Nothing teleports; nothing waits on an animation to finish before responding.
6. **Keyboard-first, never keyboard-only.** Everything the TUIs can do has a key. Everything also works with a pointer, a touchpad and a screen reader.
7. **Belongs to Omarchy.** It wears the user's current theme and font, and changes live when they switch themes. It should look like it shipped with the OS.

---

## 3. Voice and copy

Both products already speak the same way, and this formalises it.

| Do | Don't |
|---|---|
| "3 files would leave this machine with credentials in them." | "⚠️ CRITICAL SECURITY RISK DETECTED!!" |
| "No automatic fix for this one. Here's how to address it." | "Error: fix unavailable" |
| "Restored 41 plugins. 2 need a reinstall by hand: kiro-cli, uv." | "Restore completed with warnings" |
| "Snapshot taken. Nothing changed since 14:30." | "Success!" |
| "This needs your password to write 7 files in /etc." | "Authentication required" |

**Rules:**
- **Sentences, not labels.** Say what happened, then what to do next. Use numbers, never "some" or "several".
- **Second person, present tense.** No exclamation marks, except a single one allowed on the final migration-complete screen.
- **Severity words are fixed:** *risk* (fail), *worth a look* (warn), *note* (info), *fine* (pass), *skipped* (skip). Never "critical", "danger" or "threat". The score and colour carry the weight.
- **Name things the way the user knows them:** "Hyprland keybindings", not `.config/hypr/bindings.lua`. The path is one click or key away for those who want it.
- **Errors give the cause and the next step, in plain words.** Raw tool output stays behind "Details".

---

## 4. Product structure

**Home: "Pulse."** One screen that answers "am I okay?" at a glance:
- **Protection:** last snapshot (relative time), automatic snapshots on or off, unpushed work.
- **Health:** the audit score ring with its grade, and the count of risks.
- **One primary action,** chosen by state: *Take snapshot*, *Review 2 risks*, or *Continue migration*.

**Sections** (sidebar, `1`–`6`):

| # | Section | Comes from |
|---|---|---|
| 1 | Pulse | new |
| 2 | Timeline: snapshots, diffs, restore a moment or a single item | OmaPhoenix Snapshots tab |
| 3 | Audit: findings by section; fix, accept, history and trend | secaudit TUI |
| 4 | Backup: what is captured, per category and item | OmaPhoenix Backup tab |
| 5 | Migrate: guided audit → clean → snapshot → (new machine) restore → re-audit | new, the joint flow |
| 6 | Settings: schedule, repo, secrets, policies | both |

**Migrate is a wizard, not a page.** Its steps are explicit and resumable; quitting halfway and coming back later is normal. Every step shows the next action and a way back.

---

## 5. Visual language

### 5.1 Colour: derived from the Omarchy theme, never hardcoded

The source is `~/.local/state/omarchy/current/theme/colors.toml`, re-read live when the theme changes (watch the file, and hook `omarchy-theme-set`).

| Design token | Theme key | Use |
|---|---|---|
| `surface.base` | `background` | Window |
| `surface.sunken` | `dark_background` | Sidebar, wells |
| `surface.deep` | `darker_background` | Behind sheets (scrim tint) |
| `surface.raised` | `lighter_background` | Cards, popovers |
| `text.primary` | `foreground` | Body |
| `text.secondary` | `light_foreground` | Supporting text |
| `text.muted` | `muted` / `dark_foreground` | Captions, timestamps |
| `accent` | `accent` | Focus, primary action, selection, the phoenix ember |
| `selection` | `selection` | Selected rows |
| `state.risk` | `red` | fail |
| `state.attention` | `yellow` | warn |
| `state.fine` | `green` | pass and "protected" |
| `state.info` | `cyan` | info and progress |

**Rules:**
- **`mode`** (`dark`/`light`) picks the elevation strategy: lighter surfaces upward in dark mode, shadows in light mode.
- **Contrast is checked at runtime.** Text must reach WCAG AA (4.5:1 body, 3:1 large text and UI). If a theme pairing fails, nudge lightness in OKLCH until it passes; never change hue.
- **Colour never carries meaning alone.** Every state also has an icon and a word.
- **Severity colour sits only on the small marker** (dot, icon, ring), never on large surfaces. Large red areas cause alarm, which breaks principle 4.

### 5.2 Type
- **Use the user's Omarchy font.** Read it from fontconfig and Omarchy's font setting; the app must look like the user's terminal and bar.
- **Scale:** 12 · 13 (body) · 15 · 20 · 28 · 40 (the score only).
- **Weights:** two (regular and semibold). Line height 1.45 for body and 1.2 for display.
- **Tabular figures** for counts, sizes, times and the score, so numbers don't jitter as they change.
- **Monospace** only for paths, commands and package names.

### 5.3 Space, shape, depth
- **4-pt grid.** Component padding is 12/16, section gaps 24/32.
- **Radius:** 6 for controls, 10 for cards, 14 for sheets. If the user's Hyprland `rounding` is 0, use 0: match the desktop.
- **Depth uses at most three levels:** base, raised, overlay. Elevation shows by surface tone, with one soft shadow only in light mode.
- **Hairlines:** 1 px at `text.muted` with 15 % alpha. Prefer space over lines.

### 5.4 Icons
- **Outline icons on a 1.5 px stroke and 16/20 px grid,** one family only.
- **The phoenix ember** (a small accent-coloured flame mark) is the only brand flourish. It appears in Pulse when fully protected, and at the end of a migration. Nowhere else.

---

## 6. Motion and physics

Motion exists to explain cause and effect, and to make the interface feel like objects rather than pictures. It is never decoration.

### 6.1 The laws (non-negotiable)

1. **Interruptible.** Every animation can be redirected mid-flight. It continues from its current position **and velocity**, never from its start or end. A user who changes their mind is never punished.
2. **Input is never blocked.** Keys and clicks act immediately; motion catches up. No "wait until the transition ends".
3. **Springs, not curves,** for anything that moves or resizes. Springs carry velocity across interruptions; fixed-duration easing curves can't. Opacity-only fades may use short curves.
4. **Things come from somewhere.**
   - A detail view grows from the row that opened it.
   - A restored item travels from the timeline to its category.
   - A dismissed sheet returns the way it came.
5. **Continuity of identity.** The same object keeps the same shape and position across screens (shared-element transitions). A snapshot dot in Pulse *is* the dot in Timeline.
6. **Respect reduced motion.** With the system's reduced-motion setting on, or Omarchy's animations disabled:
   - movement is replaced by short cross-fades (≤ 120 ms)
   - springs become instant settles
   - nothing loops

### 6.2 Spring tokens

Parameters are given as **response** (seconds, the perceived duration) and **damping ratio** (1 = no overshoot), which is how designers reason. The implementation converts them: `stiffness = (2π / response)² × mass`, `damping = 4π × ratio × mass / response`, with `mass = 1`.

| Token | Response | Damping | Use |
|---|---|---|---|
| `spring.snap` | 0.20 s | 1.0 | Focus ring, selection highlight, toggles. Feels instant, never jumps |
| `spring.glide` | 0.35 s | 1.0 | Navigation between sections, list reorder, panel resize |
| `spring.sheet` | 0.42 s | 0.92 | Sheets and dialogs entering and leaving, drawers. A whisper of settle |
| `spring.reveal` | 0.50 s | 0.85 | Detail expanding from a row; the score ring filling |
| `spring.ember` | 0.70 s | 0.70 | Only for the protected and migration-complete moments. The single place a visible overshoot is allowed |

**Fades:** 120 ms in, 90 ms out (exits are faster than entrances, because people are done with what is leaving).

**Stagger:** lists appear with 18 ms per item, capped at 8 items; the rest arrive together. Never stagger on scroll.

### 6.3 Real-world physics details

- **Rubber-banding.** Scrolling past the end resists with a decaying offset, `offset = (1 − 1/(d × 0.55 / h + 1)) × h`, and springs back with `spring.glide`.
- **Momentum.** A touchpad flick carries velocity and decays with friction (deceleration rate 0.998 per ms). A flick on a selection list stops on an item boundary, never mid-row.
- **Velocity hand-off.** When a dragged sheet is released, its release velocity seeds the spring. Fast flicks dismiss; slow drags return.
- **Weight.** Large surfaces (sheets, panels) use longer responses than small ones (chips, rings). A panel never moves faster than a button.
- **No physics for state words.** Text content (counts, names) cross-fades. Numbers *roll* digit by digit using `spring.snap`, and only when a number changes while visible.

### 6.4 Signature moments

These are the only places motion is allowed to be noticed.

| Moment | What happens |
|---|---|
| **Snapshot taken** | The newest timeline dot drops in from above (`spring.sheet`), and the previous "now" marker slides down one slot. A single soft accent pulse runs along the timeline rail (opacity only, 600 ms, once). |
| **Audit score** | The ring fills from its previous value to the new one with `spring.reveal`, and the number rolls. A grade change (C→B) cross-fades the letter after the ring settles, never before. |
| **Risk resolved** | The finding row collapses (height `spring.glide`) as its marker turns from `state.risk` to `state.fine`. The score ring then nudges up. Cause precedes effect. |
| **Restore progress** | Categories are a vertical stack of cards. Each card's progress fills with the *real* fraction; a finished card settles with `spring.snap` and its icon becomes a check. No indeterminate spinners for work that has a count. |
| **Migration complete** | The ember appears (`spring.ember`), the before/after scores sit side by side, and the copy reads: "Everything came back. Nothing risky came with it." This is the one exclamation point we allow, if the copy wants it. |

### 6.5 Performance and memory budget
- **Memory (hard requirement):** the desktop app idles at **≤ 150 MB** total across all its processes (PSS), with a ceiling of **250 MB**; the TUI at **≤ 24 MB**. Details and the CLI budgets are in RUST.md §9. A stack that can't meet this doesn't ship, however good it looks.
- **Frame rate:** 120 fps on 120 Hz displays, never below 60. Only transform and opacity are animated; never layout properties per frame.
- **Responsiveness:** first paint in under 300 ms, and input-to-feedback within one frame (8–16 ms). Anything slower than 100 ms shows progress in place.

---

## 7. Interaction

### 7.1 Keyboard

This keymap unifies both TUIs, and the conflicts are resolved here.

| Key | Action | Note |
|---|---|---|
| `1`–`6` | Sections | |
| `↑↓`/`jk`, `←→`/`hl` | Move / into / out of | |
| `space` | Select / tick | both TUIs agree |
| `enter` | Open / preview | |
| `/` | Search the current section | secaudit |
| `⌃k` / `super+k`-style palette | Command palette: every action by name | new |
| `s` | **Snapshot now** (Timeline, Pulse) · **Scan** (Audit) | context-scoped, same idea "capture state" |
| `R` | **Rescan** (Audit) | secaudit used `r`; moved, because `r` restores |
| `r` | **Restore** (Timeline, opens the preview first) | OmaPhoenix |
| `x` | Run fix (Audit; always confirms) | secaudit |
| `a` | Accept risk (Audit) | secaudit |
| `w` | Save selection (Backup) | OmaPhoenix |
| `u` | Undo the last apply | new |
| `?` | All keys, for this screen | |
| `esc` | Back / close; never discards without asking | |

**Focus** is always visible, a 2 px `accent` ring moving with `spring.snap` between elements. Focus is never trapped except inside a modal sheet.

### 7.2 Pointer and touchpad
- **Targets** are at least 28 px. Hover reveals secondary actions after 80 ms, so passing through doesn't flicker.
- **Right-click** gives the same actions as the row's keyboard keys.
- **Swipe left on a finding** offers *Accept* / *Fix*. Releasing past 40 % commits, with velocity hand-off.

### 7.3 Consent, privilege and danger

- **Preview is a real dry run.** It shows exactly what will be created, overwritten, installed and enabled, with counts. "Apply" stays disabled until the preview has loaded.
- **Root is explained before it is asked for.** A sheet lists what needs root and why ("write 7 files in /etc, enable 3 services"), then the system polkit prompt appears. The app never runs as root.
- **Destructive or irreversible actions** (deleting a snapshot, re-keying secrets) need a typed confirmation of the object's name. Undoable actions don't; they show an *Undo* toast for 8 s instead.
- **Fixes from Audit always show the exact command first.** It can be copied; it runs only after the user confirms.

---

## 8. States

| State | Rule |
|---|---|
| Empty | Explain what will appear here and offer the one action that fills it. No sad illustrations. |
| Loading (known size) | Real progress with counts ("Scanning 9 of 14 sections"). |
| Loading (unknown) | A thin accent shimmer on the region that will change, after 150 ms (never flash it for fast loads). |
| Partial | Show what's ready; mark the rest pending in place. |
| Error | Plain cause, then next step, then "Details" (raw output). Keep the user's context; never clear the screen. |
| Offline | Snapshots commit locally and sync later, shown as a quiet "will sync" badge, not an error. |
| Not installed / not set up | One sentence plus a setup action. secaudit missing means the audit is unavailable, not that the app is broken. |

---

## 9. Accessibility
- **Screen readers:** the full tree is exposed over AT-SPI with names, roles and states. Severity is announced in words.
- **Contrast:** checked at runtime per theme (see 5.1).
- **Text:** respects the system scale up to 200 % with no clipping; layouts reflow.
- **Motion:** reduced motion honoured (see 6.1, law 6).
- **Colour:** no meaning in colour alone, and a colour-blind pass on the risk/attention/fine set in every Omarchy theme.
- **Time:** no time limits, except toasts, which pause on hover or focus.

---

## 10. Omarchy integration
- Launched from the Omarchy menu and a keybinding. Opens as a floating, centred window by default (a Hyprland window rule shipped in the install), and works tiled too: layouts adapt from 720 px wide.
- Live theme and font switching with no restart. The cross-fade on theme change is 200 ms.
- Respects Hyprland `rounding`, `gaps` and animation-disable settings where readable.
- Notifications go through the desktop's notification daemon, using the same voice rules.

---

## 11. Definition of done for any UI change
- [ ] Matches this file, or this file was updated in the same change (with both owners' OK).
- [ ] Works by keyboard alone and by pointer alone. Focus is always visible.
- [ ] Every animation is interruptible mid-flight without a jump (test: press the key twice quickly).
- [ ] Reduced motion is checked.
- [ ] Looks right in at least 5 Omarchy themes, including one light theme.
- [ ] Contrast passes in each of those themes.
- [ ] Copy reviewed against §3. No secret value is reachable on screen, in logs or on the clipboard.
- [ ] 120 fps on the reference machine for the touched screens; no layout animation.
- [ ] Screen-reader pass for new controls.

---

## 12. The TUI: same product, terminal medium

The TUI is not a reduced desktop app. It is the same product in a different medium, and it must feel like the same people made it.

| Topic | Desktop | TUI |
|---|---|---|
| Structure (§4) | Sidebar with sections 1–6, Migrate wizard | Same sections on the same `1`–`6` keys; tab bar; the wizard as numbered full-screen steps |
| Voice (§3) | Identical | Identical. Same strings from the same catalogue |
| Colour (§5.1) | Omarchy `colors.toml` tokens | The **same tokens**, mapped to truecolor. Fall back to the 16-colour ANSI palette (which Omarchy themes also set) when truecolor is missing; colour must never be the only signal |
| Type and space | Omarchy font, 4-pt grid | One cell is the unit; 1-cell padding, a blank line between groups; box-drawing only for real containers |
| Keymap (§7.1) | As specified | **Identical** keys. The command palette is `:` or `⌃k` |
| Motion (§6) | Springs | **Restrained:** no movement of layout. Allowed are a 1-frame highlight pulse on change, number roll for counts, real progress bars, and a spinner only for unknown durations. At most 30 redraws/s, and nothing animates while the user types |
| Consent (§7.3) | Sheets, polkit | Inline confirm screens; root work hands the terminal to `sudo` with a plain explanation printed first |
| Startup | < 300 ms | **< 100 ms, and never blocked on a terminal query** (learned the hard way: a 5 s stall on the Linux console). Colour detection is async with a dark default |
| Accessibility | AT-SPI | Works with screen readers that read the terminal (plain text order, no information in colour or position only); `NO_COLOR` respected; usable at 80×24 |
| Where it must work | Hyprland | Any terminal, SSH, and a bare Linux console (`TERM=linux`) on a fresh install |

The TUI's definition of done adds the following to §11:
- tested at 80×24 and on `TERM=linux` with no terminal replies
- every screen reachable without a mouse
- output still readable with `NO_COLOR=1`
