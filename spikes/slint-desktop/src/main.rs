//! `OmaPhoenix` desktop on Slint: GUI-stack spike (issue #19).
//!
//! Same screens, same theme mapping and same bench as the Tauri spike
//! (`crates/omaphoenix-desktop` + `ui/desktop`), so the numbers compare.

mod spring;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};
use std::{env, fs, thread};

use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use spring::{REVEAL, SHEET, Spring};

slint::include_modules!();

const ROW: f32 = 44.0;
const SHEET_H: f32 = 460.0;

// ---------- theme (DESIGN.md §5.1) ----------

const TOKEN_MAP: &[(&str, &[&str])] = &[
    ("surface-base", &["background"]),
    ("surface-sunken", &["dark_background", "background"]),
    ("surface-deep", &["darker_background", "dark_background", "background"]),
    ("surface-raised", &["lighter_background", "selection", "background"]),
    ("text-primary", &["foreground"]),
    ("text-secondary", &["light_foreground", "foreground"]),
    ("text-muted", &["muted", "dark_foreground", "foreground"]),
    ("accent", &["accent", "blue"]),
    ("selection", &["selection", "lighter_background"]),
    ("state-risk", &["red"]),
    ("state-attention", &["yellow"]),
    ("state-fine", &["green"]),
    ("state-info", &["cyan", "blue"]),
];

fn theme_file() -> Option<PathBuf> {
    if let Ok(p) = env::var("OMAPHOENIX_SPIKE_THEME") {
        return Some(PathBuf::from(p));
    }
    let state = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| Path::new(&h).join(".local/state")))?;
    Some(state.join("omarchy/current/theme/colors.toml"))
}

fn read_tokens(path: &Path) -> Option<BTreeMap<&'static str, slint::Color>> {
    let table: toml::Table = fs::read_to_string(path).ok()?.parse().ok()?;
    let hex = |k: &str| table.get(k).and_then(toml::Value::as_str).and_then(parse_hex);
    Some(
        TOKEN_MAP
            .iter()
            .filter_map(|(token, keys)| keys.iter().find_map(|k| hex(k)).map(|c| (*token, c)))
            .collect(),
    )
}

fn parse_hex(s: &str) -> Option<slint::Color> {
    let s = s.trim().strip_prefix('#')?;
    let v = u32::from_str_radix(s.get(..6)?, 16).ok()?;
    Some(slint::Color::from_rgb_u8((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

fn apply_theme(app: &App, tokens: &BTreeMap<&'static str, slint::Color>) {
    let t = app.global::<Theme>();
    for (k, c) in tokens {
        match *k {
            "surface-base" => t.set_surface_base(*c),
            "surface-sunken" => t.set_surface_sunken(*c),
            "surface-deep" => t.set_surface_deep(*c),
            "surface-raised" => t.set_surface_raised(*c),
            "text-primary" => t.set_text_primary(*c),
            "text-secondary" => t.set_text_secondary(*c),
            "text-muted" => t.set_text_muted(*c),
            "accent" => t.set_accent(*c),
            "selection" => t.set_selection(*c),
            "state-risk" => t.set_state_risk(*c),
            "state-attention" => t.set_state_attention(*c),
            "state-fine" => t.set_state_fine(*c),
            "state-info" => t.set_state_info(*c),
            _ => {}
        }
    }
}

fn omarchy_font() -> String {
    Command::new("omarchy-font-current")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        .filter(|f| !f.is_empty())
        .unwrap_or_else(|| "monospace".to_owned())
}

fn reduced_motion() -> bool {
    Command::new("hyprctl")
        .args(["getoption", "animations:enabled", "-j"])
        .output()
        .ok()
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
        .and_then(|v| v.get("int").and_then(serde_json::Value::as_i64))
        .is_some_and(|e| e == 0)
}

/// Polls the theme file; Omarchy swaps the `current/theme` symlink, so the
/// resolved path and mtime are both compared.
fn watch_theme(app: slint::Weak<App>, applied: std::sync::Arc<std::sync::Mutex<Option<String>>>) {
    thread::spawn(move || {
        let stamp = |p: &Path| -> Option<(PathBuf, SystemTime)> {
            let real = fs::canonicalize(p).ok()?;
            Some((real.clone(), fs::metadata(&real).ok()?.modified().ok()?))
        };
        let Some(path) = theme_file() else { return };
        let mut last = stamp(&path);
        loop {
            thread::sleep(Duration::from_millis(300));
            let now = stamp(&path);
            if now == last {
                continue;
            }
            last = now;
            if let Some(tokens) = read_tokens(&path) {
                let accent = tokens.get("accent").map(|c| format!("#{:02x}{:02x}{:02x}", c.red(), c.green(), c.blue()));
                let app = app.clone();
                let applied = applied.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = app.upgrade() {
                        apply_theme(&app, &tokens);
                        if let Ok(mut a) = applied.lock() {
                            *a = accent;
                        }
                    }
                });
            }
        }
    });
}

// ---------- motion ----------

struct Motion {
    ring: Spring,
    drop: Spring,
    sheet: Spring,
    sheet_open: bool,
    reduced: bool,
    last: Option<Instant>,
    drag: Vec<(Instant, f32)>,
    drag_start: f32,
}

impl Motion {
    fn moving(&self) -> bool {
        self.ring.moving() || self.drop.moving() || self.sheet.moving()
    }
}

/// Starts the frame driver if it isn't running. It writes spring values into
/// the UI and stops itself once every spring has settled: no idle wakeups.
fn kick(app: &App, motion: &Rc<RefCell<Motion>>, timer: &Rc<Timer>) {
    if timer.running() {
        return;
    }
    motion.borrow_mut().last = None;
    let (weak, m, t) = (app.as_weak(), motion.clone(), Rc::downgrade(timer));
    timer.start(TimerMode::Repeated, Duration::from_millis(4), move || {
        let Some(app) = weak.upgrade() else { return };
        let mut m = m.borrow_mut();
        let now = Instant::now();
        let dt = m.last.map_or(1.0 / 60.0, |l| (now - l).as_secs_f32().min(1.0 / 20.0));
        m.last = Some(now);
        m.ring.step(dt);
        m.drop.step(dt);
        m.sheet.step(dt);
        app.set_ring_value(m.ring.value);
        app.set_drop(m.drop.value);
        app.set_sheet_y(m.sheet.value);
        // Once the closing sheet is off screen it's invisible: settle it now
        // instead of ticking the CPU through a sub-pixel tail nobody sees.
        if !m.sheet_open && m.sheet.value >= SHEET_H - 0.5 {
            m.sheet.set(SHEET_H);
            app.set_sheet_y(SHEET_H);
            app.set_sheet_visible(false);
        }
        if !m.moving() {
            if let Some(t) = t.upgrade() {
                t.stop();
            }
        }
    });
}

// ---------- bench ----------

#[derive(Default)]
struct Bench {
    frames: Vec<f64>,
    slow: Vec<(u8, f64, u128)>, // (phase, interval ms, ms since start)
    started: Option<Instant>,
    last_frame: Option<Instant>,
    ring: Vec<f32>,
    timeline: Vec<f32>,
    sheet: Vec<f32>,
    phase: u8, // 1 ring, 2 timeline, 3 sheet
}

fn stats(xs: &[f64]) -> serde_json::Value {
    let mut s = xs.to_vec();
    s.sort_by(f64::total_cmp);
    let q = |p: f64| s.get(((p * s.len() as f64) as usize).min(s.len().saturating_sub(1))).copied().unwrap_or(0.0);
    serde_json::json!({ "n": s.len(), "p50": q(0.5), "p95": q(0.95), "p99": q(0.99), "max": s.last().copied().unwrap_or(0.0) })
}

fn continuity(v: &[f32]) -> serde_json::Value {
    let max_step = v.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0_f32, f32::max);
    let (lo, hi) = v.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
    let range = if v.is_empty() { 0.0 } else { hi - lo };
    serde_json::json!({ "frames": v.len(), "maxStep": max_step, "range": range, "maxStepShare": if range > 0.0 { max_step / range } else { 0.0 } })
}

fn pss_mb() -> f64 {
    let me = std::process::id();
    let mut todo = vec![me];
    let mut total = 0u64;
    while let Some(p) = todo.pop() {
        if let Ok(s) = fs::read_to_string(format!("/proc/{p}/smaps_rollup")) {
            total += s.lines().find_map(|l| l.strip_prefix("Pss:")).and_then(|r| r.split_whitespace().next()?.parse().ok()).unwrap_or(0);
        }
        if let Ok(c) = fs::read_to_string(format!("/proc/{p}/task/{p}/children")) {
            todo.extend(c.split_whitespace().filter_map(|x| x.parse::<u32>().ok()));
        }
    }
    total as f64 / 1024.0
}

fn main() -> Result<(), slint::PlatformError> {
    let app = App::new()?;
    let font = omarchy_font();
    app.global::<Theme>().set_font(SharedString::from(font));
    if let Some(tokens) = theme_file().and_then(|p| read_tokens(&p)) {
        apply_theme(&app, &tokens);
    }
    let applied = std::sync::Arc::new(std::sync::Mutex::new(None::<String>));
    watch_theme(app.as_weak(), applied.clone());

    let snaps = Rc::new(VecModel::from(vec![
        Snap { id: "c4ed652".into(), when: "14:32 · 2h ago".into(), note: "+2 plugins, theme → blade-runner-2049".into() },
        Snap { id: "8a9416f".into(), when: "09:10 · 7h ago".into(), note: "hypr bindings changed".into() },
        Snap { id: "10b9447".into(), when: "yesterday".into(), note: "+ollama, +flea-bin".into() },
    ]));
    app.set_snaps(ModelRc::from(snaps.clone()));
    // Created once at startup (hidden): instantiating rows and rasterizing
    // their text on every open cost a missed frame in the bench.
    let preview_rows = vec![
        PreviewRow { category: "packages".into(), count: 67, detail: "install from the repos and AUR".into() },
        PreviewRow { category: "plugins".into(), count: 41, detail: "14 from git, 27 copied".into() },
        PreviewRow { category: "themes".into(), count: 57, detail: "active: blade-runner-2049".into() },
        PreviewRow { category: "config".into(), count: 1430, detail: "files; 63 replaced, kept as .pre-restore".into() },
        PreviewRow { category: "/etc".into(), count: 7, detail: "needs your password".into() },
        PreviewRow { category: "services".into(), count: 11, detail: "enabled, not started".into() },
        PreviewRow { category: "skipped".into(), count: 3, detail: "monitor layout, touchpad: other hardware".into() },
    ];
    let rows = Rc::new(VecModel::from(preview_rows));
    app.set_rows(ModelRc::from(rows.clone()));

    let reduced = reduced_motion();
    let motion = Rc::new(RefCell::new(Motion {
        ring: Spring::new(0.0, REVEAL),
        drop: Spring::new(0.0, SHEET),
        sheet: Spring::new(SHEET_H, SHEET),
        sheet_open: false,
        reduced,
        last: None,
        drag: Vec::new(),
        drag_start: 0.0,
    }));
    let timer = Rc::new(Timer::default());

    let set_score = {
        let (w, m, t) = (app.as_weak(), motion.clone(), timer.clone());
        move |score: i32| {
            let Some(app) = w.upgrade() else { return };
            app.set_score(score);
            let r = m.borrow().reduced;
            m.borrow_mut().ring.to(score as f32, REVEAL, r);
            kick(&app, &m, &t);
        }
    };
    set_score(72);

    let counter = Rc::new(RefCell::new(0u32));
    let take_snapshot = {
        let (w, m, t, snaps, counter) = (app.as_weak(), motion.clone(), timer.clone(), snaps.clone(), counter.clone());
        move || {
            let Some(app) = w.upgrade() else { return };
            let n = {
                let mut c = counter.borrow_mut();
                *c += 1;
                *c
            };
            snaps.insert(0, Snap { id: format!("new{n}").into(), when: "just now".into(), note: format!("{n} files changed").into() });
            if snaps.row_count() > 6 {
                snaps.remove(6);
            }
            let mut mm = m.borrow_mut();
            let (v, r) = (mm.drop.value, mm.reduced);
            mm.drop.set(v - ROW); // FLIP: start one row up, from wherever it is
            mm.drop.to(0.0, SHEET, r);
            // The offset must change in the same frame as the insert, or one
            // frame renders the list shifted a whole row (caught by the bench).
            app.set_drop(mm.drop.value);
            drop(mm);
            kick(&app, &m, &t);
        }
    };
    app.on_take_snapshot(take_snapshot.clone());

    let open_preview = {
        let (w, m, t) = (app.as_weak(), motion.clone(), timer.clone());
        move || {
            let Some(app) = w.upgrade() else { return };
            app.set_section(1);
            app.set_sheet_visible(true);
            app.set_rows_loaded(false);
            app.set_rows_revealed(false);
            let r = m.borrow().reduced;
            {
                let mut mm = m.borrow_mut();
                mm.sheet_open = true;
                mm.sheet.to(0.0, SHEET, r);
            }
            kick(&app, &m, &t);
            let w2 = w.clone();
            Timer::single_shot(Duration::from_millis(350), move || {
                let Some(app) = w2.upgrade() else { return };
                // rows already exist (created at startup); just mark them loaded.
                app.set_rows_loaded(true);
                let w3 = app.as_weak();
                Timer::single_shot(Duration::from_millis(16), move || {
                    if let Some(app) = w3.upgrade() {
                        app.set_rows_revealed(true);
                    }
                });
            });
        }
    };
    app.on_open_preview(open_preview.clone());

    let close_preview = {
        let (w, m, t) = (app.as_weak(), motion.clone(), timer.clone());
        move || {
            let Some(app) = w.upgrade() else { return };
            let r = m.borrow().reduced;
            {
                let mut mm = m.borrow_mut();
                mm.sheet_open = false;
                mm.sheet.to(SHEET_H, SHEET, r);
            }
            if r {
                app.set_sheet_visible(false);
            }
            kick(&app, &m, &t);
        }
    };
    app.on_close_preview(close_preview.clone());

    {
        let w = app.as_weak();
        app.on_go(move |s| {
            if let Some(app) = w.upgrade() {
                app.set_section(s);
            }
        });
    }
    {
        let (w, m) = (app.as_weak(), motion.clone());
        app.on_drag_sheet(move |dy| {
            let Some(app) = w.upgrade() else { return };
            let mut mm = m.borrow_mut();
            if mm.drag.is_empty() {
                mm.drag_start = mm.sheet.value;
            }
            // Rubber band upward (DESIGN.md §6.3).
            let raw = mm.drag_start + dy;
            let v = if raw < 0.0 { -(1.0 - 1.0 / ((-raw * 0.55) / SHEET_H + 1.0)) * SHEET_H } else { raw };
            mm.sheet.set(v);
            mm.drag.push((Instant::now(), v));
            if mm.drag.len() > 6 {
                mm.drag.remove(0);
            }
            app.set_sheet_y(v);
        });
    }
    {
        let (w, m, t, close) = (app.as_weak(), motion.clone(), timer.clone(), close_preview.clone());
        app.on_release_sheet(move || {
            let Some(app) = w.upgrade() else { return };
            let (velocity, y, r) = {
                let mut mm = m.borrow_mut();
                let v = match (mm.drag.first(), mm.drag.last()) {
                    (Some(a), Some(b)) if b.0 > a.0 => (b.1 - a.1) / (b.0 - a.0).as_secs_f32(),
                    _ => 0.0,
                };
                mm.drag.clear();
                (v, mm.sheet.value, mm.reduced)
            };
            if velocity > 900.0 || y > SHEET_H * 0.4 {
                close();
            } else {
                m.borrow_mut().sheet.fling(velocity, 0.0, SHEET, r);
                kick(&app, &m, &t);
            }
        });
    }

    // ---------- bench mode: same script as the Tauri spike ----------
    if env::var_os("OMAPHOENIX_SPIKE_BENCH").is_some() {
        let bench = Rc::new(RefCell::new(Bench::default()));
        {
            let (b, w, t, snaps) = (bench.clone(), app.as_weak(), timer.clone(), snaps.clone());
            let _ = app.window().set_rendering_notifier(move |state, _| {
                if !matches!(state, slint::RenderingState::AfterRendering) {
                    return;
                }
                let Some(app) = w.upgrade() else { return };
                let mut b = b.borrow_mut();
                let now = Instant::now();
                // Pacing only while something animates: idle windows don't render.
                if t.running() {
                    if let Some(l) = b.last_frame {
                        let ms = (now - l).as_secs_f64() * 1000.0;
                        b.frames.push(ms);
                        if ms > 25.0 {
                            let since = b.started.get_or_insert(now).elapsed().as_millis();
                            let phase = b.phase;
                            b.slow.push((phase, ms, since));
                        }
                    }
                    b.started.get_or_insert(now);
                    b.last_frame = Some(now);
                } else {
                    b.last_frame = None;
                }
                match b.phase {
                    1 => b.ring.push(app.get_ring_value()),
                    2 => {
                        // On-screen position of an existing row (what the eye follows).
                        let idx = (0..snaps.row_count()).find(|&i| snaps.row_data(i).is_some_and(|s| s.id == "c4ed652"));
                        if let Some(i) = idx {
                            b.timeline.push(app.get_drop() + i as f32 * ROW);
                        }
                    }
                    3 => b.sheet.push(app.get_sheet_y()),
                    _ => {}
                }
            });
        }
        type Step = (u64, Box<dyn Fn()>);
        let s = set_score.clone();
        let ts = take_snapshot.clone();
        let (op, cp) = (open_preview.clone(), close_preview.clone());
        let ph = |p: u8, b: &Rc<RefCell<Bench>>| {
            let b = b.clone();
            move || b.borrow_mut().phase = p
        };
        let w_go = app.as_weak();
        let go = move |s: i32| {
            if let Some(a) = w_go.upgrade() {
                a.set_section(s);
            }
        };
        let (s1, s2, s3) = (s.clone(), s.clone(), s.clone());
        let (ts1, ts2, ts3) = (ts.clone(), ts.clone(), ts.clone());
        let (op1, op2, cp1, cp2) = (op.clone(), op.clone(), cp.clone(), cp.clone());
        let (g0, g1) = (go.clone(), go.clone());
        let steps: Vec<Step> = vec![
            (1500, Box::new(ph(1, &bench))),
            (0, Box::new(move || s1(30))),
            (120, Box::new(move || s2(95))),
            (150, Box::new(move || s3(55))),
            (1500, Box::new(ph(2, &bench))),
            (0, Box::new(move || ts1())),
            (80, Box::new(move || ts2())),
            (80, Box::new(move || ts3())),
            (1200, Box::new(move || g1(1))),
            (300, Box::new(ph(3, &bench))),
            (0, Box::new(move || op1())),
            (200, Box::new(move || cp1())),
            (100, Box::new(move || op2())),
            (1500, Box::new(move || cp2())),
            (900, Box::new(ph(0, &bench))),
        ];
        // Run steps on the UI thread, each after its delay.
        let mut at = 0u64;
        for (delay, f) in steps {
            at += delay;
            Timer::single_shot(Duration::from_millis(at), f);
        }
        // Fast section switches.
        for i in 0..8u64 {
            let g = g0.clone();
            Timer::single_shot(Duration::from_millis(at + i * 60), move || g(if i % 2 == 1 { 1 } else { 0 }));
        }
        at += 8 * 60 + 600;
        let (b, ap, w) = (bench.clone(), applied.clone(), app.as_weak());
        let poll = Rc::new(Timer::default());
        let poll2 = poll.clone();
        let deadline = Instant::now() + Duration::from_millis(at) + Duration::from_secs(15);
        let start_at = Instant::now() + Duration::from_millis(at);
        poll.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let now = Instant::now();
            if now < start_at {
                return;
            }
            let accent = ap.lock().ok().and_then(|a| a.clone());
            if accent.is_none() && now < deadline {
                return;
            }
            poll2.stop();
            let b = b.borrow();
            let median = {
                let mut s = b.frames.clone();
                s.sort_by(f64::total_cmp);
                s.get(s.len() / 2).copied().unwrap_or(0.0)
            };
            let dropped = b.frames.iter().filter(|&&d| d > median * 1.5).count();
            let report = serde_json::json!({
                "stack": "slint",
                "frames": { "interval_ms": stats(&b.frames), "dropped": dropped,
                    "slow": b.slow.iter().map(|(p, ms, t)| serde_json::json!({"phase": p, "ms": ms, "t": t})).collect::<Vec<_>>() },
                "continuity": { "score_ring": continuity(&b.ring), "timeline": continuity(&b.timeline), "sheet": continuity(&b.sheet) },
                "theme": { "accent": accent },
                "memory_pss_mb_after_bench": pss_mb(),
            });
            if let Ok(path) = env::var("OMAPHOENIX_SPIKE_REPORT") {
                let _ = fs::write(path, serde_json::to_vec_pretty(&report).unwrap_or_default());
            }
            if let Some(app) = w.upgrade() {
                let _ = app.hide();
            }
            let _ = slint::quit_event_loop();
        });
        std::mem::forget(poll);
    }

    app.run()
}
