//! Mouse clicks and scrolls, resolved against the regions drawing recorded.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use super::{App, CHOOSER, Focus, Modal, Screen, Tab};

impl App {
    pub fn on_mouse(&mut self, m: MouseEvent) {
        let (x, y) = (m.column, m.row);
        if !matches!(
            m.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::ScrollDown
                | MouseEventKind::ScrollUp
        ) {
            return;
        }
        let hits = self.hits.borrow().clone();
        if let MouseEventKind::ScrollDown | MouseEventKind::ScrollUp = m.kind {
            let down = matches!(m.kind, MouseEventKind::ScrollDown);
            let code = if down { KeyCode::Down } else { KeyCode::Up };
            self.on_key(KeyEvent::new(code, KeyModifiers::NONE));
            return;
        }
        for (bx, by, bw, id) in &hits.buttons {
            if y == *by && x >= *bx && x < bx + bw {
                self.click_button(id);
                return;
            }
        }
        if let Some(Modal::Chooser(_)) = self.modal {
            for (oy, oh, i) in &hits.options {
                if y >= *oy && y < oy + oh {
                    self.modal = None;
                    self.start_scan(CHOOSER.get(*i).map_or("full", |c| c.0));
                    return;
                }
            }
            return;
        }
        if self.modal.is_some() {
            return;
        }
        if let Some((hy, hh, first)) = hits.history
            && y >= hy
            && y < hy + hh
            && let Screen::History(_) = self.screen
        {
            self.screen = Screen::History(
                (first + usize::from(y - hy)).min(self.runs.len().saturating_sub(1)),
            );
            return;
        }
        for (i, (tx0, tx1, ty)) in hits.tabs.iter().enumerate() {
            if y == *ty && x >= *tx0 && x < *tx1 {
                if let Some(t) = Tab::ALL.get(i) {
                    self.set_tab(*t);
                }
                self.focus = Focus::Tabs;
                return;
            }
        }
        if let Some((sx, sy, sw)) = hits.search
            && y >= sy
            && y < sy + 3
            && x >= sx
            && x < sx + sw
        {
            self.focus = Focus::Search;
            return;
        }
        if let Some((tx, ty, th, first)) = hits.table
            && y >= ty
            && y < ty + th
            && x >= tx
        {
            let n = self.visible().len();
            self.row = (first + usize::from(y - ty)).min(n.saturating_sub(1));
            self.detail_scroll = 0;
            self.focus = Focus::Table;
            return;
        }
        if let Some((sx, sy, sh, first)) = hits.sections
            && y >= sy
            && y < sy + sh
            && x >= sx
        {
            self.focus = Focus::Sections;
            self.section_cursor = (first + usize::from(y - sy)).min(self.sections().len());
            self.sections_key(KeyEvent::new(KeyCode::Null, KeyModifiers::NONE));
            let s = self
                .section_cursor
                .checked_sub(1)
                .and_then(|i| self.sections().get(i).cloned());
            if s != self.section {
                self.section = s;
                self.clamp_row();
            }
        }
    }

    fn click_button(&mut self, id: &str) {
        match id {
            "btn-full" => self.start_scan("full"),
            "btn-quick" => self.start_scan("quick"),
            "btn-deep" => self.start_scan("deep"),
            "btn-runfix" => self.run_fix(),
            "btn-copy" => self.copy(),
            "btn-accept" => self.accept(),
            "btn-explain" => self.explain(),
            "ai-no" | "ai-yes" | "ai-use-api" | "ai-use-cc" => {
                if let Some(Modal::AiSetup(setup)) = self.modal.take() {
                    self.setup_button(setup, id);
                }
            }
            "yes" | "no" => {
                if let Some(Modal::Confirm(c)) = self.modal.take()
                    && id == "yes"
                {
                    self.confirmed(c);
                }
            }
            _ => {}
        }
    }
}
