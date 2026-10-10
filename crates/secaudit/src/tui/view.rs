//! Drawing, laid out as the Python app's Textual CSS lays it out: same rows,
//! columns, widths, glyphs and colours. `app` holds the state.

#![allow(
    clippy::many_single_char_names,
    reason = "drawing code: p palette, r rect, x/y/w/h"
)]
#![allow(
    clippy::too_many_lines,
    reason = "one function per screen, mirroring compose() in the original"
)]

use std::fmt::Write as _;

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use secaudit_core::store::{Item, KEEP_RUNS, Run, sev_name, sev_order};

use super::app::{App, CHOOSER, Focus, Hits, Modal, NARROW, Screen, Severity, Tab, WIDE};
use super::scan::{LogLine, RowState, Scan};
use super::theme::{Pal, blend};
use super::times::{ago, local_time};
use super::widgets::{self, Key, badge, clip, fill, pad, put, wrap};

/// `ScanScreen.FRAMES`.
const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Width of the keys panel (`?`), its border included.
const HELP_W: u16 = 39;

fn w16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn mmss(sec: u64) -> String {
    format!("{}:{:02}", sec / 60, sec % 60)
}

/// Rich-style word wrap of styled text to `width` columns.
pub fn wrap_spans(spans: &[Span<'_>], width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut out = Vec::new();
    let mut para: Vec<(char, Style)> = Vec::new();
    let mut paras = Vec::new();
    for s in spans {
        for c in s.content.chars() {
            if c == '\n' {
                paras.push(std::mem::take(&mut para));
            } else {
                para.push((c, s.style));
            }
        }
    }
    paras.push(para);
    for p in paras {
        // Words with their trailing spaces.
        let mut words: Vec<Vec<(char, Style)>> = Vec::new();
        let mut cur = Vec::new();
        let mut in_space = false;
        for cell in p {
            if cell.0 != ' ' && in_space {
                words.push(std::mem::take(&mut cur));
                in_space = false;
            }
            if cell.0 == ' ' {
                in_space = true;
            }
            cur.push(cell);
        }
        words.push(cur);
        let mut line: Vec<(char, Style)> = Vec::new();
        for word in words {
            let bare = word.iter().rposition(|c| c.0 != ' ').map_or(0, |i| i + 1);
            if !line.is_empty() && line.len() + bare > width {
                while line.last().is_some_and(|c| c.0 == ' ') {
                    line.pop();
                }
                out.push(to_line(&std::mem::take(&mut line)));
            }
            for cell in word {
                if line.len() >= width {
                    if cell.0 == ' ' {
                        continue;
                    }
                    out.push(to_line(&std::mem::take(&mut line)));
                }
                line.push(cell);
            }
        }
        out.push(to_line(&line));
    }
    out
}

fn to_line(cells: &[(char, Style)]) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut text = String::new();
    let mut style = None;
    for (c, s) in cells {
        if style != Some(*s) && !text.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut text),
                style.unwrap_or_default(),
            ));
        }
        style = Some(*s);
        text.push(*c);
    }
    if !text.is_empty() {
        spans.push(Span::styled(text, style.unwrap_or_default()));
    }
    Line::from(spans)
}

/// Draws `lines` from `y`, at most `h` rows.
fn put_lines(buf: &mut Buffer, x: u16, y: u16, w: u16, h: u16, lines: &[Line<'_>]) {
    for (i, l) in lines.iter().take(usize::from(h)).enumerate() {
        put(buf, x, y + w16(i), w, l);
    }
}

fn hline(buf: &mut Buffer, p: &Pal, x: u16, y: u16, w: u16) {
    put(
        buf,
        x,
        y,
        w,
        &Line::from(Span::styled("─".repeat(usize::from(w)), p.fg_(p.panel))),
    );
}

fn vline(buf: &mut Buffer, p: &Pal, x: u16, y: u16, h: u16) {
    for dy in 0..h {
        put(
            buf,
            x,
            y + dy,
            1,
            &Line::from(Span::styled("│", p.fg_(p.panel))),
        );
    }
}

/// Keeps `sel` inside a window of `h` rows starting at `offset`.
fn scrolled(offset: &std::cell::Cell<usize>, sel: usize, h: usize, total: usize) -> usize {
    let mut o = offset.get();
    if sel < o {
        o = sel;
    } else if h > 0 && sel >= o + h {
        o = sel + 1 - h;
    }
    o = o.min(total.saturating_sub(h));
    offset.set(o);
    o
}

pub fn draw(f: &mut Frame, app: &App) {
    let full = f.area();
    let buf = f.buffer_mut();
    let p = &app.pal;
    fill(buf, full, Style::new().bg(p.bg).fg(p.fg));
    *app.hits.borrow_mut() = Hits::default();
    let area = if app.help && full.width > HELP_W + 20 {
        Rect {
            width: full.width - HELP_W,
            ..full
        }
    } else {
        full
    };
    let footer_y = area.bottom().saturating_sub(1);
    match &app.screen {
        Screen::Main => match app.run() {
            Some(run) => main_screen(buf, app, area, run),
            None => welcome(buf, app, area),
        },
        Screen::Scan(scan) => scan_screen(buf, app, area, scan),
        Screen::History(sel) => history(buf, app, area, *sel),
        Screen::Log(lines, scroll, title) => log(buf, app, area, lines, *scroll, title),
        Screen::Ai(v) => ai_view(buf, app, area, v),
    }
    widgets::footer(
        buf,
        p,
        Rect {
            y: footer_y,
            height: 1,
            ..area
        },
        &footer_keys(app),
    );
    if app.help && area.width < full.width {
        help_panel(
            buf,
            app,
            Rect {
                x: area.right(),
                width: full.width - area.width,
                ..full
            },
        );
    }
    if let Some(m) = &app.modal {
        dim(buf, p, full);
        match m {
            Modal::Confirm(c) => confirm(buf, app, full, c),
            Modal::Chooser(i) => chooser(buf, app, full, *i),
            Modal::Palette(q, i) => palette(buf, app, full, q, *i),
            Modal::AiSetup(setup) => ai_setup(buf, app, full, setup),
        }
    }
    toasts(buf, app, full);
}

/// `background: $background 60%` over the screen behind a modal.
fn dim(buf: &mut Buffer, p: &Pal, area: Rect) {
    if !p.color {
        return;
    }
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            if let Some(c) = buf.cell_mut((x, y)) {
                let fg = if c.fg == Color::Reset { p.fg } else { c.fg };
                let bg = if c.bg == Color::Reset { p.bg } else { c.bg };
                c.fg = blend(p.bg, fg, 0.6);
                c.bg = blend(p.bg, bg, 0.6);
            }
        }
    }
}

fn footer_keys(app: &App) -> Vec<Key> {
    let k = |key, label, enabled| Key {
        key,
        label,
        enabled,
    };
    if app.modal.is_some() {
        return vec![k("?", "Keys", true), k("q", "Quit", true)];
    }
    match &app.screen {
        Screen::Ai(v) => {
            let mut keys = vec![
                k("esc", "Back", true),
                k("^r", "Regenerate", true),
                k("^o", "Copy answer", true),
            ];
            // The input takes printable keys; with the answer focused, ? and q work.
            if !v.input_focus {
                keys.push(k("?", "Keys", true));
                keys.push(k("q", "Quit", true));
            }
            keys
        }
        Screen::Scan(_) => vec![
            k("esc", "Cancel scan", true),
            k("?", "Keys", true),
            k("q", "Quit", true),
        ],
        Screen::History(_) => vec![
            k("esc", "Back", true),
            k("del", "Delete scan", true),
            k("?", "Keys", true),
            k("q", "Quit", true),
        ],
        Screen::Log(..) => vec![k("esc", "Back", true), k("?", "Keys", true)],
        Screen::Main if app.focus == Focus::Search => vec![k("/", "Search", true)],
        Screen::Main => vec![
            k("s", "Scan", true),
            k("x", "Fix", app.can_run_fix()),
            k("space", "Select", app.can_mark()),
            k("a", "Accept risk", app.can_accept()),
            k("i", "Explain (AI)", app.can_explain()),
            k("/", "Search", app.current.is_some()),
            k("h", "History", !app.runs.is_empty()),
            k("?", "Keys", true),
            k("q", "Quit", true),
        ],
    }
}

// -- welcome ----------------------------------------------------------------

fn welcome(buf: &mut Buffer, app: &App, area: Rect) {
    let p = &app.pal;
    widgets::topbar(buf, p, area, &app.host, &app.user, "");
    let bw = 80.min(area.width);
    let inner = usize::from(bw.saturating_sub(8));
    let mut lines = vec![
        Line::from(Span::styled("No scans yet", p.bold(p.fg))),
        Line::from(""),
    ];
    lines.extend(wrap_spans(
        &[Span::styled(
            "Security Audit checks packages, boot and disk encryption, kernel hardening, firewall, network exposure, SSH, accounts, permissions, persistence, containers, secrets in your home folder and rootkit indicators.",
            p.fg_(p.fg),
        )],
        inner,
    ));
    lines.push(Line::from(""));
    lines.extend(wrap_spans(
        &[Span::styled(
            "Everything is read-only. A full scan uses sudo only to read root-owned files.",
            p.fg_(p.muted),
        )],
        inner,
    ));
    let bh = w16(lines.len()) + 2 + 2 + 1 + 3;
    let body = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(2),
        ..area
    };
    let x = body.x + body.width.saturating_sub(bw) / 2;
    let y = body.y + body.height.saturating_sub(bh) / 2;
    rounded(
        buf,
        p,
        Rect {
            x,
            y,
            width: bw,
            height: bh,
        },
        p.accent,
        None,
    );
    put_lines(buf, x + 4, y + 2, w16(inner), w16(lines.len()), &lines);
    // Buttons: 16 wide, 1 apart, centred.
    let by = y + 2 + w16(lines.len()) + 1;
    let total = 16 * 3 + 2;
    let mut bx = x + 4 + w16(inner).saturating_sub(total) / 2;
    let focus = if let Focus::Welcome(i) = app.focus {
        Some(i)
    } else {
        None
    };
    for (i, (label, id)) in [
        ("Full scan", "btn-full"),
        ("Quick scan", "btn-quick"),
        ("Deep scan", "btn-deep"),
    ]
    .into_iter()
    .enumerate()
    {
        widgets::button(
            buf,
            p,
            Rect {
                x: bx,
                y: by,
                width: 16,
                height: 3,
            },
            label,
            i == 0,
            false,
            focus == Some(i),
        );
        let mut hits = app.hits.borrow_mut();
        for dy in 0..3 {
            hits.buttons.push((bx, by + dy, 16, id));
        }
        bx += 17;
    }
}

/// A round border box, optionally filled.
fn rounded(buf: &mut Buffer, p: &Pal, r: Rect, edge: Color, bg: Option<Color>) {
    if r.width < 2 || r.height < 2 {
        return;
    }
    let st = bg.map_or_else(|| p.fg_(edge), |b| p.fg_(edge).bg(b));
    if let Some(b) = bg {
        fill(buf, r, Style::new().bg(b).fg(p.fg));
    }
    let w = usize::from(r.width - 2);
    put(
        buf,
        r.x,
        r.y,
        r.width,
        &Line::from(Span::styled(format!("╭{}╮", "─".repeat(w)), st)),
    );
    for dy in 1..r.height - 1 {
        put(buf, r.x, r.y + dy, 1, &Line::from(Span::styled("│", st)));
        put(
            buf,
            r.right() - 1,
            r.y + dy,
            1,
            &Line::from(Span::styled("│", st)),
        );
    }
    put(
        buf,
        r.x,
        r.bottom() - 1,
        r.width,
        &Line::from(Span::styled(format!("╰{}╯", "─".repeat(w)), st)),
    );
}

// -- main screen ------------------------------------------------------------

fn main_screen(buf: &mut Buffer, app: &App, area: Rect, run: &Run) {
    let p = &app.pal;
    let narrow = app.size.0 < NARROW;
    let wide = app.size.0 >= WIDE;
    let latest = app.runs.first().is_some_and(|r| r.path == run.path);
    let when = format!(
        "{} · {} scan · {}",
        local_time(run.meta.started, "%a %-d %b %H:%M"),
        run.meta.mode,
        ago(run.meta.started)
    );
    let right = if latest {
        when
    } else {
        format!("{when}  (older scan, h for history)")
    };
    widgets::topbar(buf, p, area, &app.host, &app.user, &right);
    overview(buf, app, area, run, narrow);

    let body_top = area.y + 7;
    let footer_y = area.bottom().saturating_sub(1);
    if footer_y <= body_top + 1 {
        return;
    }
    hline(buf, p, area.x, body_top, area.width);
    let top = body_top + 1;
    let bh = footer_y - top;
    let side_w = if narrow { 26 } else { 32 }.min(area.width);
    vline(buf, p, area.x + side_w - 1, top, bh);
    sidebar(
        buf,
        app,
        Rect {
            x: area.x,
            y: top,
            width: side_w - 1,
            height: bh,
        },
        run,
        narrow,
    );
    center(
        buf,
        app,
        Rect {
            x: area.x + side_w,
            y: top,
            width: area.width - side_w,
            height: bh,
        },
        run,
        narrow,
        wide,
    );
}

fn overview(buf: &mut Buffer, app: &App, area: Rect, run: &Run, narrow: bool) {
    let p = &app.pal;
    let c = run.counts();
    let (score, grade) = (c.score(), c.grade());
    let gc = p.grade(grade);
    let (x, y) = (area.x + 2, area.y + 2);
    for (i, row) in widgets::digits(&score.to_string()).iter().enumerate() {
        put(
            buf,
            x,
            y + w16(i),
            18,
            &Line::from(Span::styled(row.clone(), p.fg_(gc))),
        );
    }
    let filled = usize::try_from((12 * score + 50) / 100)
        .unwrap_or(0)
        .min(12);
    let meter = Line::from(vec![
        Span::styled("━".repeat(filled), p.fg_(gc)),
        Span::styled("─".repeat(12 - filled), p.fg_(p.muted)),
        Span::raw("  "),
        Span::styled(grade.to_string(), p.bold(gc)),
    ]);
    put(buf, x, y + 4, 18, &meter);
    let trend_w = if narrow { 0 } else { 36 };
    let stats_w = area.width.saturating_sub(4 + 20 + trend_w);
    let lines = wrap_spans(
        &stats_text(app, run),
        usize::from(stats_w.saturating_sub(4)),
    );
    put_lines(buf, x + 22, y, stats_w.saturating_sub(4), 5, &lines);
    if !narrow && area.width > 40 {
        trendbox(
            buf,
            app,
            Rect {
                x: area.right() - 2 - 36,
                y,
                width: 36,
                height: 5,
            },
            run,
        );
    }
}

fn trendbox(buf: &mut Buffer, app: &App, r: Rect, run: &Run) {
    let p = &app.pal;
    put(
        buf,
        r.x,
        r.y,
        r.width,
        &Line::from(Span::styled("SCORE TREND", p.bold(p.text_muted))),
    );
    let chron: Vec<&Run> = {
        let v: Vec<&Run> = app
            .runs
            .iter()
            .rev()
            .filter(|x| x.meta.started <= run.meta.started)
            .collect();
        if v.is_empty() { vec![run] } else { v }
    };
    let mut cap: Vec<Span> = Vec::new();
    let spark = chron.len() >= 4;
    if !spark {
        for (i, x) in chron.iter().enumerate() {
            if i > 0 {
                cap.push(Span::styled(" → ", p.fg_(p.muted)));
            }
            let c = x.counts();
            cap.push(Span::styled(
                c.score().to_string(),
                p.bold(p.grade(c.grade())),
            ));
        }
        cap.push(Span::raw("\n"));
    }
    let first = chron.first().map_or(run.meta.started, |x| x.meta.started);
    let n = chron.len();
    cap.push(Span::styled(
        format!(
            "{n} scan{} · {} → {}",
            if n == 1 { "" } else { "s" },
            local_time(first, "%-d %b"),
            local_time(run.meta.started, "%-d %b")
        ),
        p.fg_(p.muted),
    ));
    if spark {
        let data: Vec<u32> = chron.iter().map(|x| x.counts().score()).collect();
        let lines = widgets::sparkline(&data, usize::from(r.width), 2, p.red, p.green);
        put_lines(buf, r.x, r.y + 2, r.width, 2, &lines);
        // The caption sits below the box's 5 rows, as in the original.
        return;
    }
    let lines = wrap_spans(&cap, usize::from(r.width));
    put_lines(buf, r.x, r.y + 2, r.width, 3, &lines);
}

/// `stats_text()`.
fn stats_text(app: &App, run: &Run) -> Vec<Span<'static>> {
    let p = &app.pal;
    let c = run.counts();
    let mut t = Vec::new();
    for (sev, n, icon, label) in [
        ("fail", c.fail, "✗", "fail"),
        ("warn", c.warn, "!", "warn"),
        ("pass", c.pass, "✓", "pass"),
    ] {
        let st = if n > 0 {
            p.bold(p.sev(sev)).bg(p.panel)
        } else {
            p.fg_(p.muted).bg(p.panel)
        };
        t.push(Span::styled(format!(" {icon} {n} {label} "), st));
        t.push(Span::raw("  "));
    }
    if c.accepted > 0 {
        t.push(Span::styled(
            format!(" ≈ {} accepted ", c.accepted),
            p.fg_(p.muted).bg(p.panel),
        ));
    }
    t.push(Span::raw("\n"));
    let mut mode = run.meta.mode.clone();
    if let Some(f) = mode.get_mut(0..1) {
        f.make_ascii_uppercase();
    }
    let host = if run.meta.host.is_empty() {
        app.host.clone()
    } else {
        run.meta.host.clone()
    };
    let mut meta = format!("{mode} scan of {host}");
    if let Some(fin) = run.meta.finished
        && run.meta.started > 0
    {
        let _ = write!(
            meta,
            " · took {}",
            mmss(fin.saturating_sub(run.meta.started))
        );
    }
    let _ = writeln!(meta, " · {} notes · {} skipped", c.info, c.skip);
    t.push(Span::styled(meta, p.fg_(p.muted)));
    if let Some(prev) = app.previous_of_run(run) {
        let (a, b) = (run.issue_keys(), prev.issue_keys());
        let (new, fixed) = (a.difference(&b).count(), b.difference(&a).count());
        let since = if prev.meta.mode == run.meta.mode {
            String::new()
        } else {
            format!(" ({})", prev.meta.mode)
        };
        t.push(Span::styled(
            format!("Since previous scan{since}: "),
            p.fg_(p.muted),
        ));
        t.push(Span::styled(
            format!("{new} new"),
            if new > 0 {
                p.bold(p.red)
            } else {
                p.fg_(p.muted)
            },
        ));
        t.push(Span::styled(" · ", p.fg_(p.muted)));
        t.push(Span::styled(
            format!("{fixed} resolved"),
            if fixed > 0 {
                p.bold(p.green)
            } else {
                p.fg_(p.muted)
            },
        ));
        let delta = i64::from(run.counts().score()) - i64::from(prev.counts().score());
        if delta != 0 {
            t.push(Span::styled(
                format!(" · score {}{delta}", if delta > 0 { "+" } else { "" }),
                p.fg_(if delta > 0 { p.green } else { p.red }),
            ));
        }
        t.push(Span::raw("\n"));
    } else {
        t.push(Span::styled("First scan on record\n", p.fg_(p.muted)));
    }
    let it = p.fg_(p.muted).add_modifier(Modifier::ITALIC);
    let key = p.bold(p.accent);
    if run.meta.mode == "quick" {
        t.push(Span::styled("Root-only checks skipped. Press ", it));
        t.push(Span::styled("s", key));
        t.push(Span::styled(" for a full scan.", it));
    } else {
        let applied = run.applied();
        let unfixed = run
            .items
            .iter()
            .filter(|f| {
                f.is_issue() && !f.accepted && !f.finding.fix.is_empty() && !applied.contains(&f.n)
            })
            .count();
        if unfixed > 0 {
            t.push(Span::styled(
                format!(
                    "{unfixed} issue{} can be fixed automatically. Press ",
                    if unfixed == 1 { "" } else { "s" }
                ),
                it,
            ));
            t.push(Span::styled("*", key));
            t.push(Span::styled(" then ", it));
            t.push(Span::styled("x", key));
            t.push(Span::styled(" to fix them together.", it));
        }
    }
    t
}

fn sidebar(buf: &mut Buffer, app: &App, r: Rect, run: &Run, narrow: bool) {
    let p = &app.pal;
    // padding: 1 1 0 1
    let (x, w) = (r.x + 1, r.width.saturating_sub(2));
    put(
        buf,
        x,
        r.y + 1,
        w,
        &Line::from(Span::styled("SECTIONS", p.bold(p.text_muted))),
    );
    let list_top = r.y + 3;
    let mut list_bottom = r.bottom();
    if !narrow && r.height > 16 {
        // border, padding 1 1 1 1, seven tips
        let ty = r.bottom() - 10;
        hline(buf, p, x, ty, w);
        let keys = [
            ("space", "select"),
            ("x", "fix selected"),
            ("*", "select all fixable"),
            ("a", "accept risk"),
            ("i", "explain with AI"),
            ("I", "AI action plan"),
            ("r", "rescan"),
        ];
        for (i, (k, what)) in keys.iter().enumerate() {
            let l = Line::from(vec![
                Span::styled(format!("{k:>5} "), p.bold(p.accent)),
                Span::styled((*what).to_owned(), p.fg_(p.muted)),
            ]);
            put(buf, x + 1, ty + 2 + w16(i), w.saturating_sub(1), &l);
        }
        list_bottom = ty;
    }
    let lh = usize::from(list_bottom.saturating_sub(list_top));
    let sections = app.sections();
    let total = sections.len() + 1;
    let scroll = total > lh;
    let text_w = usize::from(w).saturating_sub(2 + if scroll { 2 } else { 0 });
    let label_w = usize::from(if narrow { 26u16 } else { 32 }) - 1 - 2 - 2 - 1 - 2 - 3;
    let offset = scrolled(&app.sec_offset, app.section_cursor, lh, total);
    let c = run.counts();
    for row in 0..lh.min(total - offset) {
        let i = offset + row;
        let y = list_top + w16(row);
        let mut spans: Vec<Span> = if i == 0 {
            vec![
                Span::styled("◆ ", p.fg_(p.accent)),
                Span::styled(pad("All sections", label_w), p.bold(p.fg)),
                Span::styled(format!("{:>3}", c.fail + c.warn), p.fg_(p.muted)),
            ]
        } else {
            let sec = sections.get(i - 1).map_or("", String::as_str);
            let in_sec = |sev: &str| {
                run.items
                    .iter()
                    .filter(|f| {
                        f.finding.section == sec && sev_name(f.finding.sev) == sev && !f.accepted
                    })
                    .count()
            };
            let (fails, warns) = (in_sec("fail"), in_sec("warn"));
            let (icon, col) = if fails > 0 {
                ("✗", p.red)
            } else if warns > 0 {
                ("!", p.yellow)
            } else {
                ("✓", p.green)
            };
            let n = fails + warns;
            vec![
                Span::styled(format!("{icon} "), p.bold(col)),
                Span::styled(pad(&clip(sec, label_w), label_w), p.fg_(p.fg)),
                Span::styled(
                    if n > 0 {
                        format!("{n:>3}")
                    } else {
                        "   ".into()
                    },
                    p.fg_(col),
                ),
            ]
        };
        // text-overflow: ellipsis at the item's width.
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        if text.chars().count() > text_w {
            let mut left = text_w.saturating_sub(1);
            let mut cut = Vec::new();
            for s in spans {
                let n = s.content.chars().count();
                if left == 0 {
                    break;
                }
                let take: String = s.content.chars().take(left).collect();
                left -= n.min(left);
                cut.push(Span::styled(take, s.style));
            }
            cut.push(Span::styled("…", p.fg_(p.fg)));
            spans = cut;
        }
        if i == app.section_cursor {
            let bg = if app.focus == Focus::Sections {
                p.accent
            } else {
                p.highlight
            };
            fill(
                buf,
                Rect {
                    x,
                    y,
                    width: w16(text_w + 2),
                    height: 1,
                },
                Style::new().bg(bg),
            );
            spans = spans
                .into_iter()
                .map(|s| Span::styled(s.content, s.style.bg(bg)))
                .collect();
            if app.focus == Focus::Sections {
                spans = spans
                    .into_iter()
                    .map(|s| Span::styled(s.content, p.cursor()))
                    .collect();
            }
        }
        put(buf, x + 1, y, w16(text_w), &Line::from(spans));
    }
    if scroll {
        widgets::scrollbar(buf, p, x + w - 2, list_top, w16(lh), total, offset);
    }
    app.hits.borrow_mut().sections = Some((x, list_top, w16(lh), offset));
}

fn center(buf: &mut Buffer, app: &App, r: Rect, run: &Run, narrow: bool, wide: bool) {
    let p = &app.pal;
    let search_w = if narrow { 22 } else { 36 };
    let sx = r.right().saturating_sub(1 + search_w);
    let c = run.counts();
    let labels = [
        format!("Issues {}", c.fail + c.warn),
        format!("Accepted {}", c.accepted),
        format!("Notes {}", c.info + c.skip),
        format!("Passed {}", c.pass),
        format!("All {}", run.items.len()),
    ];
    let active = Tab::ALL.iter().position(|t| *t == app.tab).unwrap_or(0);
    let tabs_area = Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: sx.saturating_sub(r.x + 1),
        height: 2,
    };
    let ranges = widgets::tabs(buf, p, tabs_area, &labels, active, app.focus == Focus::Tabs);
    widgets::input(
        buf,
        p,
        Rect {
            x: sx,
            y: r.y,
            width: search_w,
            height: 3,
        },
        &app.search,
        "Search findings…  /",
        app.focus == Focus::Search,
    );
    {
        let mut hits = app.hits.borrow_mut();
        hits.tabs = ranges
            .iter()
            .filter(|(a, _)| *a < sx)
            .map(|(a, b)| (*a, (*b).min(sx), r.y + 1))
            .collect();
        hits.search = Some((sx, r.y, search_w));
    }
    let mut y = r.y + 3;
    if !app.marked.is_empty() {
        let n = app.marked.len();
        let bg = blend(p.accent, p.bg, 0.25);
        fill(
            buf,
            Rect {
                x: r.x,
                y,
                width: r.width,
                height: 1,
            },
            Style::new().bg(bg),
        );
        let k = p.bold(p.accent).bg(bg);
        let t = p.fg_(blend(Color::Rgb(255, 255, 255), bg, 0.87)).bg(bg);
        let l = Line::from(vec![
            Span::styled("◉ ", k),
            Span::styled(format!("{n} selected"), t.add_modifier(Modifier::BOLD)),
            Span::styled("   ", t),
            Span::styled("x", k),
            Span::styled(format!(" fix {}{n}   ", if n > 1 { "all " } else { "" }), t),
            Span::styled("space", k),
            Span::styled(" toggle   ", t),
            Span::styled("*", k),
            Span::styled(" all   ", t),
            Span::styled("esc", k),
            Span::styled(" clear", t),
        ]);
        put(buf, r.x + 1, y, r.width.saturating_sub(2), &l);
        y += 1;
    }
    let sh = r.bottom().saturating_sub(y);
    let rows = app.visible();
    if wide {
        let tw = r.width * 3 / 5;
        table(
            buf,
            app,
            Rect {
                x: r.x,
                y,
                width: tw,
                height: sh,
            },
            &rows,
        );
        vline(buf, p, r.x + tw, y, sh);
        detail(
            buf,
            app,
            Rect {
                x: r.x + tw + 3,
                y,
                width: (r.width - tw).saturating_sub(5),
                height: sh,
            },
            run,
        );
    } else {
        // 3fr / 2fr: the table gets floor(3/5), the detail the rest, 9..=18.
        let dh = (sh - sh * 3 / 5).clamp(9, 18).min(sh);
        let th = sh - dh;
        table(
            buf,
            app,
            Rect {
                x: r.x,
                y,
                width: r.width,
                height: th,
            },
            &rows,
        );
        if dh > 0 {
            hline(buf, p, r.x, y + th, r.width);
            detail(
                buf,
                app,
                Rect {
                    x: r.x + 2,
                    y: y + th + 1,
                    width: r.width.saturating_sub(4),
                    height: dh - 1,
                },
                run,
            );
        }
    }
}

/// The findings `DataTable`.
fn table(buf: &mut Buffer, app: &App, r: Rect, rows: &[&Item]) {
    let p = &app.pal;
    if r.height == 0 {
        return;
    }
    // Columns are sized on terminal resize only (`on_resize`), so the keys
    // panel narrows the table without re-sizing them.
    let avail = usize::from((r.width + if app.help { HELP_W } else { 0 }).max(60));
    let sec_w = if app.section.is_some() { 0 } else { 22 };
    let ncols = if sec_w > 0 { 5 } else { 4 };
    let msg_w = 24.max(avail.saturating_sub(1 + 7 + sec_w + 8 + ncols * 2 + 2));
    let mut cols = vec![(1, ""), (7, "")];
    if sec_w > 0 {
        cols.push((sec_w, "Section"));
    }
    cols.push((msg_w, "Finding"));
    cols.push((8, "Fix"));
    let content_w: usize = cols.iter().map(|c| c.0 + 2).sum();
    let mut body_h = usize::from(r.height) - 1;
    let vscroll = rows.len() > body_h;
    let view_w = usize::from(r.width) - if vscroll { 2 } else { 0 };
    let hscroll = content_w > view_w && body_h > 1;
    if hscroll {
        body_h -= 1;
    }
    let header = cols.iter().fold(String::new(), |mut h, (w, l)| {
        let _ = write!(h, " {} ", pad(l, *w));
        h
    });
    let focused = app.focus == Focus::Table;
    let hs = p.bold(p.fg).bg(if focused { p.header_bg } else { p.panel });
    put(
        buf,
        r.x,
        r.y,
        w16(view_w),
        &Line::from(Span::styled(pad(&header, view_w), hs)),
    );
    let offset = scrolled(&app.row_offset, app.row, body_h, rows.len());
    let applied = app.run().map(Run::applied).unwrap_or_default();
    for (i, f) in rows.iter().enumerate().skip(offset).take(body_h) {
        let y = r.y + 1 + w16(i - offset);
        let fd = &f.finding;
        let sev = sev_name(fd.sev);
        let fix = if f.accepted {
            Span::styled("accepted", p.fg_(p.muted))
        } else if applied.contains(&f.n) {
            Span::styled("✓ done", p.fg_(p.green))
        } else if !fd.fix.is_empty() {
            Span::styled("● fix", p.fg_(p.accent))
        } else if !fd.hint.is_empty() && f.is_issue() {
            Span::styled("advice", p.fg_(p.muted))
        } else {
            Span::raw("")
        };
        let mark = if app.marked.contains(&f.n) {
            Span::styled("◉", p.bold(p.accent))
        } else {
            Span::raw("")
        };
        let mut b = badge(p, sev);
        if f.accepted {
            b.style = p.fg_(p.muted).add_modifier(Modifier::DIM | Modifier::BOLD);
        }
        let mut cells = vec![(mark, 1), (b, 7)];
        if sec_w > 0 {
            cells.push((
                Span::styled(clip(&fd.section, sec_w), p.fg_(p.muted)),
                sec_w,
            ));
        }
        cells.push((
            Span::styled(
                clip(f.title(), msg_w),
                if f.accepted {
                    p.fg_(p.muted)
                } else {
                    p.fg_(p.fg)
                },
            ),
            msg_w,
        ));
        cells.push((fix, 8));
        let mut spans = Vec::new();
        for (s, w) in cells {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(pad(&s.content, w), s.style));
            spans.push(Span::raw(" "));
        }
        if i == app.row {
            // Blurred cursor: plain text colour on the highlight.
            let st = if focused {
                p.cursor()
            } else {
                p.fg_(p.fg).bg(p.highlight)
            };
            spans = spans
                .into_iter()
                .map(|s| {
                    let keep_bold = s.style.add_modifier.contains(Modifier::BOLD);
                    let blurred = if keep_bold {
                        st.add_modifier(Modifier::BOLD)
                    } else {
                        st
                    };
                    Span::styled(s.content, if focused { st } else { blurred })
                })
                .collect();
            fill(
                buf,
                Rect {
                    x: r.x,
                    y,
                    width: w16(view_w),
                    height: 1,
                },
                st,
            );
        }
        put(buf, r.x, y, w16(view_w), &Line::from(spans));
    }
    if vscroll {
        widgets::scrollbar(
            buf,
            p,
            r.right() - 2,
            r.y,
            r.height - u16::from(hscroll),
            rows.len() + 1,
            offset,
        );
    }
    if hscroll {
        widgets::hscrollbar(buf, p, r.x, r.y + 1 + w16(body_h), w16(view_w), content_w);
    }
    app.hits.borrow_mut().table = Some((r.x, r.y + 1, w16(body_h), offset));
}

/// The detail pane (`show_detail()`), as lines; buttons get their row.
/// Detail lines, the row of the action buttons, and each button's (x, width, id).
type DetailLines = (
    Vec<Line<'static>>,
    Option<usize>,
    Vec<(u16, u16, &'static str)>,
);

fn detail_lines(app: &App, run: &Run, width: usize) -> DetailLines {
    let p = &app.pal;
    let mut out: Vec<Line<'static>> = vec![Line::from("")];
    let Some(f) = app.selected() else {
        let (msg, sub) = if app.tab == Tab::Accepted && app.search.is_empty() {
            (
                Span::styled("No accepted risks", p.bold(p.fg)),
                "Press a on an issue you've decided to live with; it stops counting against your score.",
            )
        } else if app.tab == Tab::Issues && app.search.is_empty() {
            (
                Span::styled("✓ No issues here", p.bold(p.green)),
                "Nothing to fix in this view.",
            )
        } else {
            (
                Span::styled("No matching findings", p.bold(p.fg)),
                "Try another tab, section, or search.",
            )
        };
        out.extend(wrap_spans(&[msg], width));
        out.extend(wrap_spans(&[Span::styled(sub, p.fg_(p.text_muted))], width));
        return (out, None, Vec::new());
    };
    let fd = &f.finding;
    let sev = sev_name(fd.sev);
    out.extend(wrap_spans(
        &[
            badge(p, sev),
            Span::raw("  "),
            Span::styled(f.title().to_owned(), p.bold(p.fg)),
        ],
        width,
    ));
    let mut meta = vec![Span::styled(fd.section.clone(), p.fg_(p.muted))];
    if f.accepted {
        let since = app
            .accepted
            .get(&f.accept_key())
            .map(|a| a.at)
            .filter(|at| *at > 0)
            .map(|at| format!(" since {}", local_time(at, "%-d %b")))
            .unwrap_or_default();
        meta.push(Span::styled("  ·  ", p.fg_(p.muted)));
        meta.push(Span::styled(
            format!("accepted risk{since}, not scored"),
            p.fg_(p.muted).add_modifier(Modifier::ITALIC),
        ));
    } else if run.applied().contains(&f.n) {
        meta.push(Span::styled("  ·  ", p.fg_(p.muted)));
        meta.push(Span::styled(
            "✓ fix applied, rescan (r) to verify",
            p.fg_(p.green),
        ));
    }
    if sev == "pass" && fd.detail.is_empty() {
        meta.push(Span::styled("\nThis check passed.", p.fg_(p.muted)));
    }
    out.extend(wrap_spans(&meta, width));
    let actions_row;
    let mut buttons = Vec::new();
    // The actions row is always there now (it holds Explain).
    {
        out.push(Line::from(""));
        let mut spans = Vec::new();
        let mut x = 0u16;
        let mut add =
            |label: String, primary: bool, id: &'static str, spans: &mut Vec<Span<'static>>| {
                let w = w16(label.chars().count() + 4).max(14);
                spans.push(widgets::compact(p, &label, primary));
                spans.push(Span::raw("  "));
                buttons.push((x, w, id));
                x += w + 2;
            };
        if !fd.fix.is_empty() && !f.accepted {
            add("Run fix  x".into(), true, "btn-runfix", &mut spans);
        }
        if !fd.fix.is_empty() || !fd.hint.is_empty() {
            add(
                if fd.fix.is_empty() {
                    "Copy advice  c"
                } else {
                    "Copy fix  c"
                }
                .into(),
                false,
                "btn-copy",
                &mut spans,
            );
        }
        if f.is_issue() {
            add(
                if f.accepted {
                    "Un-accept  a"
                } else {
                    "Accept risk  a"
                }
                .into(),
                false,
                "btn-accept",
                &mut spans,
            );
        }
        if sev_name(fd.sev) != "pass" {
            add("Explain  i".into(), false, "btn-explain", &mut spans);
        }
        actions_row = Some(out.len());
        out.push(Line::from(spans));
        out.push(Line::from(""));
    }
    let cap = |t: &str| Line::from(Span::styled(t.to_owned(), p.bold(p.text_muted)));
    if !fd.detail.is_empty() {
        out.push(cap("DETAILS"));
        out.extend(
            wrap(&fd.detail, width)
                .into_iter()
                .map(|l| Line::from(Span::styled(l, p.fg_(p.fg)))),
        );
        out.push(Line::from(""));
    }
    if !fd.fix.is_empty() {
        out.push(cap("SUGGESTED FIX"));
        // Round border in $panel around a $boost box, text padded by 1.
        let inner = width.saturating_sub(2);
        let edge = p.fg_(p.panel);
        out.push(Line::from(Span::styled(
            format!("╭{}╮", "─".repeat(inner)),
            edge,
        )));
        for l in wrap(&fd.fix, inner.saturating_sub(2)) {
            out.push(Line::from(vec![
                Span::styled("│", edge.bg(p.boost)),
                Span::styled(pad(&format!(" {l}"), inner), p.fg_(p.accent).bg(p.boost)),
                Span::styled("│", edge.bg(p.boost)),
            ]));
        }
        out.push(Line::from(Span::styled(
            format!("╰{}╯", "─".repeat(inner)),
            edge,
        )));
        out.push(Line::from(""));
    }
    if !fd.hint.is_empty() {
        out.push(cap("HOW TO ADDRESS"));
        out.extend(
            wrap(&fd.hint, width)
                .into_iter()
                .map(|l| Line::from(Span::styled(l, p.fg_(p.fg).add_modifier(Modifier::ITALIC)))),
        );
        out.push(Line::from(""));
    }
    (out, actions_row, buttons)
}

fn detail(buf: &mut Buffer, app: &App, r: Rect, run: &Run) {
    let p = &app.pal;
    let h = usize::from(r.height);
    // Textual decides at the narrower width: text that fits only without
    // the scrollbar keeps it.
    let (narrow, narrow_arow, narrow_buttons) =
        detail_lines(app, run, usize::from(r.width.saturating_sub(2)));
    let scroll = narrow.len() > h;
    let (lines, arow, buttons) = if scroll {
        (narrow, narrow_arow, narrow_buttons)
    } else {
        detail_lines(app, run, usize::from(r.width))
    };
    let max = lines.len().saturating_sub(h);
    let off = app.detail_scroll.min(max);
    put_lines(
        buf,
        r.x,
        r.y,
        r.width,
        r.height,
        lines.get(off..).unwrap_or_default(),
    );
    if scroll {
        // Textual keeps it inside the right padding.
        widgets::scrollbar(buf, p, r.right() - 2, r.y, r.height, lines.len(), off);
    }
    if let Some(ar) = arow
        && ar >= off
        && ar - off < h
    {
        let y = r.y + w16(ar - off);
        let mut hits = app.hits.borrow_mut();
        for (bx, bw, id) in buttons {
            hits.buttons.push((r.x + bx, y, bw, id));
        }
    }
}

// -- scan -------------------------------------------------------------------

fn scan_screen(buf: &mut Buffer, app: &App, area: Rect, scan: &Scan) {
    let p = &app.pal;
    let who = if scan.mode == "quick" {
        app.user.as_str()
    } else {
        "root"
    };
    widgets::topbar(
        buf,
        p,
        area,
        &app.host,
        &app.user,
        &format!("{} scan · as {who}", scan.mode),
    );
    let bw = 80.min(area.width);
    let x = area.x + (area.width - bw) / 2;
    let mut y = area.y + 2;
    let title = if scan.failed.is_some() {
        Span::styled("Scan failed", p.bold(p.red))
    } else {
        Span::styled(format!("Scanning {}…", app.host), p.bold(p.fg))
    };
    put(buf, x, y, bw, &Line::from(title));
    y += 2;
    // ProgressBar: bar then the percentage.
    let total = scan.rows.len().max(1);
    let done = if scan.done_at.is_some() {
        total
    } else {
        scan.done()
    };
    let bar_w = usize::from(bw.saturating_sub(5));
    let filled = bar_w * done / total;
    let complete = done == total;
    let col = if complete { p.green } else { p.accent };
    let mut bar = vec![Span::styled("━".repeat(filled), p.fg_(col))];
    if filled < bar_w {
        bar.push(Span::styled(
            if filled > 0 { "╺" } else { "━" },
            p.fg_(p.bar_dim),
        ));
        bar.push(Span::styled(
            "━".repeat(bar_w - filled - 1),
            p.fg_(p.bar_dim),
        ));
    }
    bar.push(Span::styled(
        format!("{:>4}%", 100 * done / total),
        p.fg_(p.fg),
    ));
    put(buf, x, y, bw, &Line::from(bar));
    y += 1;
    let secs = scan.started.elapsed().as_secs();
    let elapsed = if scan.failed.is_some() {
        Span::styled("press escape to go back", p.fg_(p.muted))
    } else if scan.done_at.is_some() {
        Span::styled(format!("Finished in {}", mmss(secs)), p.fg_(p.green))
    } else {
        Span::styled(
            format!("{} elapsed · press escape to cancel", mmss(secs)),
            p.fg_(p.muted),
        )
    };
    put(buf, x, y, bw, &Line::from(elapsed));
    y += 2;
    let frame = usize::try_from(scan.started.elapsed().as_millis() / 100).unwrap_or(0);
    for row in &scan.rows {
        let l = match row.state {
            RowState::Done => {
                let (icon, c) = if row.fail > 0 {
                    ("✗", p.red)
                } else if row.warn > 0 {
                    ("!", p.yellow)
                } else {
                    ("✓", p.green)
                };
                let mut s = vec![
                    Span::raw(" "),
                    Span::styled(icon, p.bold(c)),
                    Span::raw("  "),
                    Span::raw(format!("{:<34}", row.name)),
                ];
                if row.fail > 0 {
                    s.push(Span::styled(format!("{} fail  ", row.fail), p.fg_(p.red)));
                }
                if row.warn > 0 {
                    s.push(Span::styled(format!("{} warn", row.warn), p.fg_(p.yellow)));
                }
                Line::from(s)
            }
            RowState::Running => Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    FRAMES.get(frame % FRAMES.len()).copied().unwrap_or("⠋"),
                    p.bold(p.accent),
                ),
                Span::raw("  "),
                Span::styled(format!("{:<34}", row.name), p.bold(p.fg)),
                Span::styled("checking…", p.fg_(p.muted).add_modifier(Modifier::ITALIC)),
            ]),
            RowState::Waiting => Line::from(vec![
                Span::raw(" "),
                Span::styled("·", p.fg_(p.muted)),
                Span::raw("  "),
                Span::styled(row.name, p.fg_(p.muted)),
            ]),
        };
        put(buf, x, y, bw, &l);
        y += 1;
    }
    y += 1;
    put(
        buf,
        x,
        y,
        bw,
        &Line::from(Span::styled("LIVE FINDINGS", p.bold(p.text_muted))),
    );
    y += 1;
    rounded(
        buf,
        p,
        Rect {
            x,
            y,
            width: bw,
            height: 9,
        },
        p.panel,
        None,
    );
    let inner = usize::from(bw.saturating_sub(2));
    let mut lines = Vec::new();
    for l in &scan.log {
        match l {
            LogLine::Finding(fail, sec, msg) => lines.extend(wrap_spans(
                &[
                    badge(p, if *fail { "fail" } else { "warn" }),
                    Span::raw("  "),
                    Span::styled(format!("{sec}: "), p.fg_(p.muted)),
                    Span::styled(msg.clone(), p.fg_(p.fg)),
                ],
                inner,
            )),
            LogLine::Failure(reason) => lines.extend(wrap_spans(
                &[Span::styled(reason.clone(), p.fg_(p.red))],
                inner,
            )),
        }
    }
    let start = lines.len().saturating_sub(7);
    put_lines(
        buf,
        x + 1,
        y + 1,
        w16(inner),
        7,
        lines.get(start..).unwrap_or_default(),
    );
}

// -- history ----------------------------------------------------------------

fn history(buf: &mut Buffer, app: &App, area: Rect, sel: usize) {
    let p = &app.pal;
    widgets::topbar(buf, p, area, &app.host, &app.user, "history");
    let footer_y = area.bottom().saturating_sub(1);
    let (x, y) = (area.x + 2, area.y + 2);
    let sum_x = area.right().saturating_sub(2 + 42);
    let trend_w = sum_x.saturating_sub(x);
    put(
        buf,
        x,
        y,
        trend_w,
        &Line::from(Span::styled(
            "SCORE TREND · oldest → newest",
            p.bold(p.text_muted),
        )),
    );
    let scores: Vec<u32> = app.runs.iter().rev().map(|r| r.counts().score()).collect();
    let data = if scores.is_empty() { vec![0] } else { scores };
    put_lines(
        buf,
        x,
        y + 2,
        trend_w,
        3,
        &widgets::sparkline(&data, usize::from(trend_w), 3, p.red, p.green),
    );
    if let Some(latest) = app.runs.first() {
        let best = app
            .runs
            .iter()
            .max_by_key(|r| r.counts().score())
            .unwrap_or(latest);
        let (lc, bc) = (latest.counts(), best.counts());
        let lines = [
            Line::from(vec![
                Span::styled("Latest  ", p.fg_(p.muted)),
                Span::styled(
                    format!("{} {}", lc.score(), lc.grade()),
                    p.bold(p.grade(lc.grade())),
                ),
            ]),
            Line::from(vec![
                Span::styled("Best    ", p.fg_(p.muted)),
                Span::styled(
                    format!("{} {}", bc.score(), bc.grade()),
                    p.bold(p.grade(bc.grade())),
                ),
                Span::styled(
                    format!("  {}", local_time(best.meta.started, "%-d %b")),
                    p.fg_(p.muted),
                ),
            ]),
            Line::from(vec![
                Span::styled("Scans   ", p.fg_(p.muted)),
                Span::styled(
                    format!("{} kept (max {KEEP_RUNS})", app.runs.len()),
                    p.fg_(p.fg),
                ),
            ]),
        ];
        put_lines(buf, sum_x + 3, y, 39, 3, &lines);
    }
    // The comparison below the table.
    let diff = history_diff(app, sel, usize::from(area.width.saturating_sub(4)));
    let dh = w16(diff.len().min(14)) + 3;
    let dy = footer_y.saturating_sub(dh);
    hline(buf, p, area.x, dy, area.width);
    put_lines(
        buf,
        area.x + 2,
        dy + 2,
        area.width.saturating_sub(4),
        dh - 3,
        &diff,
    );
    // Table: margin 1 1 0 1.
    let t = Rect {
        x: area.x + 1,
        y: area.y + 8,
        width: area.width.saturating_sub(2),
        height: dy.saturating_sub(area.y + 8),
    };
    if t.height == 0 {
        return;
    }
    let cols = [
        (18, "When"),
        (9, ""),
        (6, "Mode"),
        (7, "Score"),
        (5, "Fail"),
        (5, "Warn"),
        (24, "Change vs previous"),
    ];
    let header = cols.iter().fold(String::new(), |mut h, (w, l)| {
        let _ = write!(h, " {} ", pad(l, *w));
        h
    });
    put(
        buf,
        t.x,
        t.y,
        t.width,
        &Line::from(Span::styled(
            pad(&header, usize::from(t.width)),
            p.bold(p.fg).bg(p.header_bg),
        )),
    );
    let body_h = usize::from(t.height - 1);
    let offset = scrolled(&app.hist_offset, sel, body_h, app.runs.len());
    for (i, r) in app.runs.iter().enumerate().skip(offset).take(body_h) {
        let c = r.counts();
        let change: Vec<Span> = match app.runs.get(i + 1) {
            None => vec![Span::styled("first scan", p.fg_(p.muted))],
            Some(prev) => {
                let (a, b) = (r.issue_keys(), prev.issue_keys());
                let (new, fixed) = (a.difference(&b).count(), b.difference(&a).count());
                vec![
                    Span::styled(
                        format!("+{new} new"),
                        p.fg_(if new > 0 { p.red } else { p.muted }),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        format!("−{fixed} resolved"),
                        p.fg_(if fixed > 0 { p.green } else { p.muted }),
                    ),
                ]
            }
        };
        let cells: Vec<(Vec<Span>, usize)> = vec![
            (
                vec![Span::styled(
                    local_time(r.meta.started, "%a %-d %b %H:%M"),
                    p.fg_(p.fg),
                )],
                18,
            ),
            (vec![Span::styled(ago(r.meta.started), p.fg_(p.muted))], 9),
            (vec![Span::styled(r.meta.mode.clone(), p.fg_(p.fg))], 6),
            (
                vec![Span::styled(
                    format!("{:>3} {}", c.score(), c.grade()),
                    p.bold(p.grade(c.grade())),
                )],
                7,
            ),
            (
                vec![Span::styled(
                    c.fail.to_string(),
                    p.fg_(if c.fail > 0 { p.red } else { p.muted }),
                )],
                5,
            ),
            (
                vec![Span::styled(
                    c.warn.to_string(),
                    p.fg_(if c.warn > 0 { p.yellow } else { p.muted }),
                )],
                5,
            ),
            (change, 24),
        ];
        let mut spans = Vec::new();
        for (cell, w) in cells {
            spans.push(Span::raw(" "));
            let n: usize = cell.iter().map(|s| s.content.chars().count()).sum();
            spans.extend(cell);
            spans.push(Span::raw(" ".repeat(w.saturating_sub(n) + 1)));
        }
        let ry = t.y + 1 + w16(i - offset);
        if i == sel {
            let st = p.cursor();
            fill(
                buf,
                Rect {
                    x: t.x,
                    y: ry,
                    width: t.width,
                    height: 1,
                },
                st,
            );
            spans = spans
                .into_iter()
                .map(|s| Span::styled(s.content, st))
                .collect();
        }
        put(buf, t.x, ry, t.width, &Line::from(spans));
    }
    app.hits.borrow_mut().history = Some((t.y + 1, w16(body_h), offset));
}

/// `HistoryScreen.highlighted()`: what changed against the previous scan.
fn history_diff(app: &App, sel: usize, width: usize) -> Vec<Line<'static>> {
    let p = &app.pal;
    let Some(r) = app.runs.get(sel) else {
        return vec![Line::from(Span::styled("No scans yet.", p.fg_(p.muted)))];
    };
    let Some(prev) = app.runs.get(sel + 1) else {
        return vec![Line::from(Span::styled(
            "First scan on record; nothing to compare against.",
            p.fg_(p.muted),
        ))];
    };
    let (a, b) = (r.issue_keys(), prev.issue_keys());
    let mut new: Vec<_> = a.difference(&b).collect();
    let mut fixed: Vec<_> = b.difference(&a).collect();
    new.sort_by_key(|k| sev_order(k.0));
    fixed.sort_by_key(|k| sev_order(k.0));
    let mut t: Vec<Span> = vec![
        Span::styled(
            format!(
                "Compared with {} ({} scan)\n",
                local_time(prev.meta.started, "%a %-d %b %H:%M"),
                prev.meta.mode
            ),
            p.fg_(p.muted),
        ),
        Span::styled("New      ", p.bold(p.fg)),
        Span::styled(if new.is_empty() { "none\n" } else { "\n" }, p.fg_(p.muted)),
    ];
    for (sev, sec, msg) in new.iter().take(6) {
        t.push(Span::raw("  "));
        t.push(badge(p, sev_name(*sev)));
        t.push(Span::styled(format!("  {sec}: "), p.fg_(p.muted)));
        t.push(Span::styled(
            format!("{}\n", msg.trim_end_matches(':')),
            p.fg_(p.fg),
        ));
    }
    t.push(Span::styled("Resolved ", p.bold(p.fg)));
    t.push(Span::styled(
        if fixed.is_empty() { "none\n" } else { "\n" },
        p.fg_(p.muted),
    ));
    for (_, sec, msg) in fixed.iter().take(6) {
        t.push(Span::styled("  ✓ ", p.fg_(p.green)));
        t.push(Span::styled(format!("{sec}: "), p.fg_(p.muted)));
        t.push(Span::styled(
            format!("{}\n", msg.trim_end_matches(':')),
            p.fg_(p.fg),
        ));
    }
    if prev.meta.mode != r.meta.mode {
        t.push(Span::styled(
            "Different scan modes: some differences come from coverage, not changes.",
            p.fg_(p.muted).add_modifier(Modifier::ITALIC),
        ));
    }
    let mut lines = wrap_spans(&t, width);
    while lines.last().is_some_and(|l| l.width() == 0) {
        lines.pop();
    }
    lines
}

// -- log --------------------------------------------------------------------

fn log(buf: &mut Buffer, app: &App, area: Rect, lines: &[String], scroll: usize, title: &str) {
    let p = &app.pal;
    widgets::topbar(
        buf,
        p,
        area,
        &app.host,
        &app.user,
        &format!("full output · {title}"),
    );
    let footer_y = area.bottom().saturating_sub(1);
    let r = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: footer_y.saturating_sub(area.y + 1),
    };
    let widest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let hscroll = widest > usize::from(r.width) && r.height > 1;
    let h = r.height - u16::from(hscroll);
    let vscroll = lines.len() > usize::from(h);
    let w = r.width - if vscroll { 2 } else { 0 };
    for (i, l) in lines.iter().skip(scroll).take(usize::from(h)).enumerate() {
        let shown: String = l.chars().take(usize::from(w)).collect();
        put(
            buf,
            r.x,
            r.y + w16(i),
            w,
            &Line::from(Span::styled(shown, p.fg_(p.fg))),
        );
    }
    if vscroll {
        widgets::scrollbar(buf, p, r.x + w, r.y, h, lines.len(), scroll);
    }
    if hscroll {
        widgets::hscrollbar(buf, p, r.x, r.y + h, w, widest);
    }
}

// -- overlays ---------------------------------------------------------------

/// `#dialog`: width 84, round accent border, padding 1 2, on `$surface`.
fn dialog(buf: &mut Buffer, p: &Pal, full: Rect, content_h: u16) -> Rect {
    let w = 84.min(full.width);
    let h = (content_h + 4).min(full.height);
    let r = Rect {
        x: full.x + (full.width - w) / 2,
        y: full.y + (full.height - h) / 2,
        width: w,
        height: h,
    };
    rounded(buf, p, r, p.accent, Some(p.surface));
    Rect {
        x: r.x + 3,
        y: r.y + 2,
        width: w.saturating_sub(6),
        height: h.saturating_sub(4),
    }
}

fn confirm(buf: &mut Buffer, app: &App, full: Rect, c: &super::app::Confirm) {
    let p = &app.pal;
    let width = 78.min(usize::from(full.width.saturating_sub(6)));
    let mut lines = wrap_spans(&[Span::styled(c.title.clone(), p.bold(p.fg))], width);
    lines.push(Line::from(""));
    if !c.code.is_empty() {
        let mut code = Vec::new();
        for l in &c.code {
            let st = if l.starts_with("$ ") {
                p.fg_(p.accent)
            } else {
                p.bold(p.fg)
            };
            for wl in wrap(l, width.saturating_sub(2)) {
                code.push(Line::from(Span::styled(
                    pad(&format!(" {wl}"), width),
                    st.bg(p.boost),
                )));
            }
        }
        code.truncate(16);
        lines.extend(code);
    }
    if !c.note.is_empty() {
        lines.push(Line::from(""));
        lines.extend(wrap_spans(
            &[Span::styled(c.note.clone(), p.fg_(p.muted_on(p.surface)))],
            width,
        ));
    }
    let inner = dialog(buf, p, full, w16(lines.len()) + 4);
    put_lines(buf, inner.x, inner.y, inner.width, inner.height, &lines);
    let by = inner.y + w16(lines.len()) + 1;
    let ok = format!("{}  y", c.ok);
    let (w1, w2) = (16u16, w16(ok.chars().count() + 4).max(16));
    let x2 = inner.right().saturating_sub(w2);
    let x1 = x2.saturating_sub(1 + w1);
    widgets::button(
        buf,
        p,
        Rect {
            x: x1,
            y: by,
            width: w1,
            height: 3,
        },
        "Cancel  n",
        false,
        false,
        !c.on_ok,
    );
    widgets::button(
        buf,
        p,
        Rect {
            x: x2,
            y: by,
            width: w2,
            height: 3,
        },
        &ok,
        !c.danger,
        c.danger,
        c.on_ok,
    );
    let mut hits = app.hits.borrow_mut();
    for dy in 0..3 {
        hits.buttons.push((x1, by + dy, w1, "no"));
        hits.buttons.push((x2, by + dy, w2, "yes"));
    }
}

fn chooser(buf: &mut Buffer, app: &App, full: Rect, sel: usize) {
    let p = &app.pal;
    let inner = dialog(buf, p, full, 2 + 6 + 2);
    put(
        buf,
        inner.x,
        inner.y,
        inner.width,
        &Line::from(Span::styled("Start a scan", p.bold(p.fg))),
    );
    let mut y = inner.y + 2;
    for (i, (_, name, key, desc)) in CHOOSER.iter().enumerate() {
        // The focused list's highlight: accent background, cursor text.
        let on = i == sel;
        let (name_st, key_st, desc_st) = if on {
            (
                p.cursor(),
                p.fg_(p.accent).bg(p.accent).add_modifier(Modifier::BOLD),
                p.bold(p.muted).bg(p.accent),
            )
        } else {
            (p.bold(p.fg), p.fg_(p.accent), p.fg_(p.muted))
        };
        if on {
            fill(
                buf,
                Rect {
                    x: inner.x + 1,
                    y,
                    width: inner.width.saturating_sub(2),
                    height: 2,
                },
                p.cursor(),
            );
        }
        put(
            buf,
            inner.x + 2,
            y,
            inner.width - 3,
            &Line::from(vec![
                Span::styled(*name, name_st),
                Span::styled(format!("  {key}"), key_st),
            ]),
        );
        put(
            buf,
            inner.x + 2,
            y + 1,
            inner.width - 3,
            &Line::from(Span::styled(*desc, desc_st)),
        );
        app.hits.borrow_mut().options.push((y, 2, i));
        y += 2;
    }
    put(
        buf,
        inner.x,
        y + 1,
        inner.width,
        &Line::from(Span::styled(
            "All scans are read-only. sudo is only used to read root-owned files.",
            p.fg_(p.muted_on(p.surface)),
        )),
    );
}

/// Textual's command palette.
fn palette(buf: &mut Buffer, app: &App, full: Rect, q: &str, sel: usize) {
    let p = &app.pal;
    let cmds = app.commands(q);
    let y0 = full.y + 3;
    let max_rows = usize::from(full.height.saturating_sub(y0 + 7)) / 2;
    let shown = cmds.len().min(max_rows);
    let h = w16(shown * 2) + 6;
    let r = Rect {
        x: full.x,
        y: y0,
        width: full.width,
        height: h.min(full.height.saturating_sub(y0)),
    };
    fill(buf, r, Style::new().bg(p.surface).fg(p.fg));
    let w = usize::from(r.width);
    put(
        buf,
        r.x,
        r.y,
        r.width,
        &Line::from(Span::styled("▔".repeat(w), p.fg_(p.accent).bg(p.surface))),
    );
    let prompt = if q.is_empty() {
        Span::styled("Search for commands…", p.fg_(p.muted))
    } else {
        Span::styled(format!("{q}▏"), p.fg_(p.fg))
    };
    put(
        buf,
        r.x + 2,
        r.y + 2,
        r.width.saturating_sub(4),
        &Line::from(vec![Span::raw("🔎 "), prompt]),
    );
    let mut y = r.y + 5;
    let start = sel.saturating_sub(shown.saturating_sub(1));
    for (i, (name, help)) in cmds.iter().enumerate().skip(start).take(shown) {
        let bg = if i == sel { p.highlight } else { p.surface };
        fill(
            buf,
            Rect {
                x: r.x,
                y,
                width: r.width,
                height: 2,
            },
            Style::new().bg(bg),
        );
        put(
            buf,
            r.x + 2,
            y,
            r.width.saturating_sub(4),
            &Line::from(Span::styled(*name, p.bold(p.fg).bg(bg))),
        );
        put(
            buf,
            r.x + 2,
            y + 1,
            r.width.saturating_sub(4),
            &Line::from(Span::styled(*help, p.fg_(p.muted).bg(bg))),
        );
        y += 2;
    }
    if r.bottom() > 0 {
        put(
            buf,
            r.x,
            r.bottom() - 1,
            r.width,
            &Line::from(Span::styled("▁".repeat(w), p.fg_(p.accent).bg(p.bg))),
        );
    }
}

/// Textual's `HelpPanel`: the focused widget's keys, then the app's.
fn help_panel(buf: &mut Buffer, app: &App, r: Rect) {
    let p = &app.pal;
    for dy in 0..r.height {
        put(
            buf,
            r.x,
            r.y + dy,
            1,
            &Line::from(Span::styled("▏", p.fg_(p.panel))),
        );
    }
    let table: &[(&str, &str)] = &[
        ("↑", "Cursor up"),
        ("↓", "Cursor down"),
        ("←", "Cursor left"),
        ("→", "Cursor right"),
        ("home", "Home"),
        ("end", "End"),
        ("pgup", "Page up"),
        ("pgdn", "Page down"),
        ("^pgup", "Page Left"),
        ("^pgdn", "Page Right"),
        ("⏎", "Select"),
        ("^home", "Top"),
        ("^end", "Bottom"),
    ];
    let widget: &[(&str, &str)] = match (&app.screen, app.focus) {
        (Screen::Main, Focus::Search) => &[
            ("", "Input"),
            ("←", "Move cursor left"),
            ("→", "Move cursor right or accept the completion suggestion"),
            ("home ^a", "Go to start"),
            ("end ^e", "Go to end"),
            ("⌫", "Delete character left"),
            ("del ^d", "Delete character right"),
            ("⏎", "Submit"),
            ("^w", "Delete left to start of word"),
            ("^u super+⌫", "Delete all to the left"),
        ],
        (Screen::Main, Focus::Sections) => {
            &[("↑", "Cursor up"), ("↓", "Cursor down"), ("⏎", "Select")]
        }
        (Screen::Main, Focus::Tabs) => &[("←", "Previous tab"), ("→", "Next tab")],
        (Screen::Main, Focus::Detail) | (Screen::Log(..), _) => &[
            ("↑", "Scroll Up"),
            ("↓", "Scroll Down"),
            ("pgup", "Page Up"),
            ("pgdn", "Page Down"),
            ("home", "Scroll Home"),
            ("end", "Scroll End"),
        ],
        (Screen::Main, Focus::Welcome(_)) => &[("⏎", "Press button")],
        _ => table,
    };
    let common: &[(&str, &str)] = &[
        ("", ""),
        ("tab", "Focus Next"),
        ("shift+tab", "Focus Previous"),
        ("^c super+c", "Copy selected text"),
        ("", ""),
        (
            "^q q",
            "Quit Quit the app and return to the command prompt.",
        ),
    ];
    let app_keys: &[(&str, &str)] = match app.screen {
        Screen::Main => &[
            ("s", "Scan"),
            ("x", "Fix"),
            ("space", "Select"),
            ("a", "Accept risk"),
            ("i", "Explain (AI)"),
            ("I", "AI action plan"),
            ("/", "Search"),
            ("h", "History"),
            ("r", "Rescan"),
            ("*", "Select all fixable"),
            ("c", "Copy fix"),
            ("o", "Full output"),
            ("e", "Export report"),
            ("1", "Issues tab"),
            ("2", "Accepted tab"),
            ("3", "Notes tab"),
            ("4", "Passed tab"),
            ("5", "All tab"),
            ("esc", "Clear selection/search"),
            ("?", "Keys"),
        ],
        Screen::Scan(_) => &[("esc", "Cancel scan"), ("?", "Keys")],
        Screen::History(_) => &[
            ("esc", "Back"),
            ("⏎", "Open scan"),
            ("del d", "Delete scan"),
            ("?", "Keys"),
        ],
        Screen::Log(..) => &[("esc q", "Back"), ("?", "Keys")],
        Screen::Ai(_) => &[
            ("esc", "Back"),
            ("^r", "Regenerate"),
            ("^o", "Copy answer"),
            ("?", "Keys"),
        ],
    };
    let desc_w = usize::from(r.width.saturating_sub(15));
    let mut rows: Vec<Line> = Vec::new();
    for (k, d) in widget.iter().chain(common).chain(app_keys) {
        for (i, l) in wrap(d, desc_w).iter().enumerate() {
            let key = if i == 0 {
                format!("{k:>12}")
            } else {
                " ".repeat(12)
            };
            rows.push(Line::from(vec![
                Span::styled(key, p.bold(p.accent)),
                Span::raw(" "),
                Span::styled(l.clone(), p.fg_(p.fg)),
            ]));
        }
    }
    let h = r.height.saturating_sub(1);
    put_lines(buf, r.x + 1, r.y + 1, r.width - 1, h, &rows);
    if rows.len() > usize::from(h) {
        widgets::scrollbar(buf, p, r.right() - 2, r.y + 1, h, rows.len(), 0);
    }
}

fn toasts(buf: &mut Buffer, app: &App, full: Rect) {
    let p = &app.pal;
    let mut bottom = full.bottom().saturating_sub(1);
    for t in app.toasts.iter().rev() {
        let edge = match t.severity {
            Severity::Information => p.green,
            Severity::Warning => p.yellow,
            Severity::Error => p.red,
        };
        let h = widgets::toast(
            buf,
            p,
            Rect {
                height: bottom.saturating_sub(full.y),
                ..full
            },
            t.title.as_deref(),
            &t.body,
            edge,
        );
        bottom = bottom.saturating_sub(h + 1);
        if bottom <= full.y + h {
            break;
        }
    }
}

// -- AI (the original's AIScreen and AISetupScreen) ----------------------------

/// `AIScreen`: title, note, the answer in a bordered scroll box, status,
/// follow-up input.
fn ai_view(buf: &mut Buffer, app: &App, area: Rect, v: &super::ai_screen::AiView) {
    use super::ai_screen::{Part, Status};
    let p = &app.pal;
    widgets::topbar(
        buf,
        p,
        area,
        &app.host,
        &app.user,
        &format!("AI · {} · {}", v.subject, v.via()),
    );
    let footer_y = area.bottom().saturating_sub(1);
    // #ai-box: padding 1 2 0 2
    let x = area.x + 2;
    let w = area.width.saturating_sub(4);
    put(buf, x, area.y + 2, w, &Line::from(v.title.clone()));
    put(
        buf,
        x,
        area.y + 3,
        w,
        &Line::from(Span::styled(
            "AI-generated. Review commands before running them; secaudit's own fixes are on the main screen.",
            p.fg_(p.muted).add_modifier(Modifier::ITALIC),
        )),
    );
    // Scroll box: from row 5 to above status (1), input (3), margin (1), footer.
    let top = area.y + 5;
    let bottom = footer_y.saturating_sub(5);
    if bottom <= top + 2 {
        return;
    }
    // Textual keeps a 2-column scrollbar gutter outside the scroll box.
    let r = Rect {
        x,
        y: top,
        width: w.saturating_sub(2),
        height: bottom - top,
    };
    rounded(buf, p, r, p.panel, None);
    // Inside: padding 0 1, the Markdown's own padding 0 2, a scrollbar.
    let content_x = r.x + 4;
    let content_w = usize::from(r.width.saturating_sub(10));
    let h = usize::from(r.height - 2);
    let mut lines: Vec<Line<'static>> = Vec::new();
    let question = |q: &str, lines: &mut Vec<Line<'static>>| {
        lines.push(Line::from(""));
        for (i, l) in wrap_spans(&[Span::styled(q.to_owned(), p.fg_(p.fg))], content_w + 2)
            .into_iter()
            .enumerate()
        {
            let mut spans = if i == 0 {
                vec![Span::styled("You  ", p.bold(p.accent))]
            } else {
                vec![]
            };
            spans.extend(l.spans);
            lines.push(Line::from(spans).patch_style(Style::default()));
        }
        lines.push(Line::from(""));
    };
    let indent = |l: Line<'static>| {
        let mut spans = vec![Span::raw("  ")];
        spans.extend(l.spans);
        Line::from(spans)
    };
    for part in &v.parts {
        match part {
            Part::Question(q) => question(q, &mut lines),
            Part::Answer(a) => lines.extend(
                super::markdown::render(p, a, content_w)
                    .into_iter()
                    .map(indent),
            ),
        }
    }
    if let Some(text) = v.streaming().filter(|t| !t.is_empty()) {
        lines.extend(
            super::markdown::render(p, text, content_w)
                .into_iter()
                .map(indent),
        );
    }
    let max = lines.len().saturating_sub(h);
    let off = if v.follow { max } else { v.scroll.min(max) };
    put_lines(
        buf,
        content_x - 2,
        r.y + 1,
        r.width.saturating_sub(6),
        w16(h),
        lines.get(off..).unwrap_or_default(),
    );
    if lines.len() > h {
        // Textual counts one more row in the scroll height than it draws.
        let virtual_h = lines.len() + 1;
        widgets::scrollbar(buf, p, r.right() - 4, r.y + 1, w16(h), virtual_h, off);
    }
    // Status (margin 0 1).
    let muted = p.fg_(p.muted);
    let status = match &v.status {
        Status::Thinking => Line::from(Span::styled(
            "Claude is thinking…",
            muted.add_modifier(Modifier::ITALIC),
        )),
        Status::Writing => Line::from(Span::styled(
            "Writing…",
            muted.add_modifier(Modifier::ITALIC),
        )),
        Status::Saved => Line::from(vec![
            Span::styled("saved answer", muted),
            Span::styled("  ·  ctrl+r", p.bold(p.accent)),
            Span::styled(" to ask again", muted),
        ]),
        Status::Note(note, warn) => {
            let mut s = vec![Span::styled(note.clone(), muted)];
            if let Some(wn) = warn {
                s.push(Span::styled(wn.clone(), p.fg_(p.yellow)));
            }
            Line::from(s)
        }
        Status::Error(msg, retry) => Line::from(vec![
            Span::styled("✗ ", p.bold(p.red)),
            Span::styled(msg.clone(), p.fg_(p.red)),
            Span::styled(if *retry { "  ·  ctrl+r to retry" } else { "" }, muted),
        ]),
        Status::Empty => Line::from(""),
    };
    put(buf, x + 1, bottom, w.saturating_sub(2), &status);
    widgets::input(
        buf,
        p,
        Rect {
            x,
            y: bottom + 1,
            width: w,
            height: 3,
        },
        &v.input,
        "Ask a follow-up, e.g. \"what breaks if I do this?\"  (enter to send)",
        !v.busy && v.input_focus,
    );
}

/// `AISetupScreen`: one-time consent, Claude Code or an API key.
fn ai_setup(buf: &mut Buffer, app: &App, full: Rect, s: &super::ai_screen::AiSetup) {
    let p = &app.pal;
    let width = 78.min(usize::from(full.width.saturating_sub(6)));
    let muted = p.fg_(p.muted);
    let plain = p.fg_(p.fg);
    let mut body: Vec<Span<'static>> = if s.mode == "claude-code" {
        vec![
            Span::styled("Explanations come from ", plain),
            Span::styled("Claude Code", p.bold(p.fg)),
            Span::styled(
                " using your existing sign-in, so no API key is needed. It runs with every tool, MCP server and hook turned off, so it can only answer.\n\n",
                plain,
            ),
            Span::styled(
                "When you ask, the finding (section, message, details and suggested fix) is sent to Claude. Your username, hostname and home path are replaced first, and the scan never collects secret values.\n\n",
                plain,
            ),
            Span::styled(
                "Nothing is sent until you press i or I. Usage counts toward your Claude plan.",
                muted,
            ),
        ]
    } else {
        vec![
            Span::styled(
                "When you ask, the finding (section, message, details and suggested fix) is sent to Anthropic's API so Claude can explain it. Your username, hostname and home path are replaced first, and the scan never collects secret values.\n\n",
                plain,
            ),
            Span::styled("Nothing is sent until you press i or I. ", muted),
            Span::styled(
                format!(
                    "Model: {}. API usage is billed to your Anthropic account.",
                    crate::ai::model()
                ),
                muted,
            ),
        ]
    };
    let asks = s.asks_key();
    if asks {
        let home = std::env::var("HOME").unwrap_or_default();
        body.push(Span::styled("\n\nPaste an API key from ", plain));
        body.push(Span::styled("console.anthropic.com", p.bold(p.accent)));
        body.push(Span::styled(" → API keys. It's saved to ", plain));
        body.push(Span::styled(
            crate::ai::key_file()
                .display()
                .to_string()
                .replacen(&home, "~", 1),
            p.fg_(p.accent),
        ));
        body.push(Span::styled(
            " readable only by you. Setting ANTHROPIC_API_KEY instead also works.",
            plain,
        ));
    }
    let mut lines = vec![
        Line::from(Span::styled("Explain findings with Claude", p.bold(p.fg))),
        Line::from(""),
    ];
    lines.extend(wrap_spans(&body, width));
    let text_h = lines.len();
    // key field (margin-top 1, 3 rows), error (margin-top 1), buttons (margin-top 1, 3 rows)
    let key_h = if asks { 4 } else { 0 };
    let inner = dialog(buf, p, full, w16(text_h + key_h + 2 + 4));
    put_lines(buf, inner.x, inner.y, inner.width, inner.height, &lines);
    let mut y = inner.y + w16(text_h);
    if asks {
        let masked: String = "•".repeat(s.key.chars().count());
        widgets::input(
            buf,
            p,
            Rect {
                x: inner.x,
                y: y + 1,
                width: inner.width,
                height: 3,
            },
            &masked,
            "sk-ant-…",
            s.focus == 0,
        );
        y += 4;
    }
    put(
        buf,
        inner.x,
        y + 1,
        inner.width,
        &Line::from(Span::styled(s.error.clone(), p.fg_(p.red))),
    );
    let by = y + 3;
    let buttons = s.buttons();
    let widths: Vec<u16> = buttons
        .iter()
        .map(|(_, l, _)| w16(l.chars().count() + 2).max(16))
        .collect();
    let total: u16 = widths.iter().sum::<u16>() + w16(buttons.len().saturating_sub(1));
    let mut bx = inner.right().saturating_sub(total);
    let first_button = usize::from(asks);
    let mut hits = app.hits.borrow_mut();
    for (i, ((id, label, primary), bw)) in buttons.iter().zip(&widths).enumerate() {
        widgets::button(
            buf,
            p,
            Rect {
                x: bx,
                y: by,
                width: *bw,
                height: 3,
            },
            label,
            *primary,
            false,
            s.focus == first_button + i,
        );
        for dy in 0..3 {
            hits.buttons.push((bx, by + dy, *bw, id));
        }
        bx += bw + 1;
    }
}

#[cfg(test)]
mod tests {
    use ratatui::text::Span;

    #[test]
    fn wraps_like_rich() {
        let l = super::wrap_spans(
            &[Span::raw(
                "Deep scan of omarchy-laptop-1 · took 1:26 · 25 notes · 0 skipped",
            )],
            56,
        );
        let t: Vec<String> = l.iter().map(ToString::to_string).collect();
        assert_eq!(
            t,
            [
                "Deep scan of omarchy-laptop-1 · took 1:26 · 25 notes · 0",
                "skipped"
            ]
        );
    }
}
