//! Backup search: one fuzzy query over every category's items, ticked from the results.

use phx_core::app::review_key;
use phx_core::fuzzy::{self, Match};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::State;

/// A search result: category index, item index within it, and how it matched `category:id`.
pub struct Hit {
    pub cat: usize,
    pub item: usize,
    pub key: String,
    pub m: Match,
}

impl State<'_> {
    /// Items matching the search, best first.
    pub fn backup_hits(&self) -> Vec<Hit> {
        let Some(choices) = &self.choices else {
            return Vec::new();
        };
        let all: Vec<(usize, usize, String)> = choices
            .iter()
            .enumerate()
            .flat_map(|(c, (choice, _))| {
                choice
                    .items
                    .iter()
                    .enumerate()
                    .map(move |(i, (id, _))| (c, i, review_key(&choice.category, id)))
            })
            .collect();
        fuzzy::rank(&self.search, &all, |(_, _, k)| k.clone())
            .into_iter()
            .filter_map(|(i, m)| {
                all.get(i).map(|(cat, item, key)| Hit {
                    cat: *cat,
                    item: *item,
                    key: key.clone(),
                    m,
                })
            })
            .collect()
    }

    fn toggle_hit(&mut self, cat: usize, item: usize) {
        if let Some((_, on)) = self
            .choices
            .as_mut()
            .and_then(|c| c.get_mut(cat))
            .and_then(|(c, _)| c.items.get_mut(item))
        {
            *on = !*on;
            self.dirty = true;
        }
    }

    fn toggle_selected_hit(&mut self) {
        let sel = self
            .backup_hits()
            .get(self.search_sel)
            .map(|h| (h.cat, h.item));
        if let Some((cat, item)) = sel {
            self.toggle_hit(cat, item);
        }
    }

    fn move_hit(&mut self, down: bool) {
        let n = self.backup_hits().len();
        self.search_sel = if down {
            (self.search_sel + 1).min(n.saturating_sub(1))
        } else {
            self.search_sel.saturating_sub(1)
        };
    }

    /// Keys while typing the query; true when the key was for the results, not the query.
    pub(super) fn backup_search_typing_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Down => self.move_hit(true),
            KeyCode::Up => self.move_hit(false),
            KeyCode::Char('n') if ctrl => self.move_hit(true),
            KeyCode::Char('p') if ctrl => self.move_hit(false),
            KeyCode::Tab => self.toggle_selected_hit(),
            _ => return false,
        }
        true
    }

    /// Keys on the results after Enter; true when handled.
    pub(super) fn backup_search_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.move_hit(true),
            KeyCode::Up | KeyCode::Char('k') => self.move_hit(false),
            KeyCode::Char(' ') | KeyCode::Tab => self.toggle_selected_hit(),
            KeyCode::Char('a') => {
                let hits: Vec<(usize, usize)> =
                    self.backup_hits().iter().map(|h| (h.cat, h.item)).collect();
                let all_on = hits.iter().all(|&(c, i)| {
                    self.choices
                        .as_ref()
                        .and_then(|ch| ch.get(c))
                        .and_then(|(ch, _)| ch.items.get(i))
                        .is_some_and(|(_, on)| *on)
                });
                for (c, i) in hits {
                    if let Some((_, on)) = self
                        .choices
                        .as_mut()
                        .and_then(|ch| ch.get_mut(c))
                        .and_then(|(ch, _)| ch.items.get_mut(i))
                    {
                        *on = !all_on;
                    }
                }
                self.dirty = true;
            }
            _ => return false,
        }
        true
    }
}
