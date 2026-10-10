//! What a snapshot captures, per category and item.

use std::collections::BTreeSet;

use phx_contract::selection::Choice;
use phx_core::App as Engine;
use phx_core::app::review_key;
use slint::{ComponentHandle, SharedString};

use crate::activity;
use crate::run::background;
use crate::state::{Shared, model, quiet, ui_engine, ui_state};
use crate::ui::{App, CatRow, ItemRow};

pub(crate) fn refresh_backup(app: &App, state: &Shared) {
    let s = state.borrow();
    app.set_cats(model(
        s.choices
            .iter()
            .map(|(c, _)| CatRow {
                name: c.category.as_str().into(),
                selected: i32::try_from(c.items.iter().filter(|(_, on)| *on).count()).unwrap_or(0),
                total: i32::try_from(c.items.len()).unwrap_or(0),
                note: new_note(&s.unreviewed, c),
            })
            .collect(),
    ));
    let sel = usize::try_from(app.get_cat_sel()).ok();
    let items = sel
        .and_then(|i| s.choices.get(i))
        .map_or_else(Vec::new, |(c, notes)| {
            c.items
                .iter()
                .zip(notes)
                .map(|((id, on), note)| ItemRow {
                    id: id.as_str().into(),
                    on: *on,
                    note: note.as_str().into(),
                    new: s.unreviewed.contains(&review_key(&c.category, id)),
                })
                .collect()
        });
    app.set_items(model(items));
}

fn new_note(unreviewed: &BTreeSet<String>, c: &Choice) -> SharedString {
    let n = c
        .items
        .iter()
        .filter(|(id, _)| unreviewed.contains(&review_key(&c.category, id)))
        .count();
    if n == 0 {
        SharedString::new()
    } else {
        format!("● {n} new").into()
    }
}

pub(crate) fn load_choices(app: &App) {
    if !ui_state().borrow_mut().jobs.insert("choices") {
        return;
    }
    activity::refresh(app);
    background(
        app,
        || {
            Engine::from_env(&quiet)
                .and_then(|e| e.choices())
                .map_err(|e| e.to_string())
        },
        |app, r| match r {
            Ok(c) => {
                let state = ui_state();
                {
                    let mut s = state.borrow_mut();
                    s.unreviewed = ui_engine().unreviewed(&c);
                    s.choices = c;
                    s.jobs.remove("choices");
                }
                refresh_backup(app, &state);
                activity::refresh(app);
            }
            Err(e) => {
                ui_state().borrow_mut().jobs.remove("choices");
                activity::refresh(app);
                app.set_toast(e.into());
            }
        },
    );
}

pub(crate) fn wire(app: &App) {
    let state = ui_state();

    app.on_select_cat({
        let (weak, state) = (app.as_weak(), state.clone());
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            app.set_cat_sel(i);
            refresh_backup(&app, &state);
        }
    });

    app.on_toggle_cat({
        let (weak, state) = (app.as_weak(), state.clone());
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            if let Ok(idx) = usize::try_from(i) {
                let mut s = state.borrow_mut();
                if let Some((c, _)) = s.choices.get_mut(idx) {
                    let all = c.items.iter().all(|(_, on)| *on);
                    c.items.iter_mut().for_each(|(_, on)| *on = !all);
                }
            }
            app.set_backup_dirty(true);
            refresh_backup(&app, &state);
        }
    });

    app.on_toggle_item({
        let (weak, state) = (app.as_weak(), state.clone());
        move |i| {
            let Some(app) = weak.upgrade() else { return };
            let (Ok(cat), Ok(i)) = (usize::try_from(app.get_cat_sel()), usize::try_from(i)) else {
                return;
            };
            if let Some((_, on)) = state
                .borrow_mut()
                .choices
                .get_mut(cat)
                .and_then(|(c, _)| c.items.get_mut(i))
            {
                *on = !*on;
            }
            app.set_backup_dirty(true);
            refresh_backup(&app, &state);
        }
    });

    app.on_save_backup({
        let weak = app.as_weak();
        move || {
            let Some(app) = weak.upgrade() else { return };
            let plain: Vec<Choice> = state
                .borrow()
                .choices
                .iter()
                .map(|(c, _)| c.clone())
                .collect();
            match ui_engine().save_choices(&plain) {
                Ok(()) => {
                    state.borrow_mut().unreviewed.clear();
                    refresh_backup(&app, &state);
                    activity::refresh(&app);
                    app.set_backup_dirty(false);
                    app.set_toast("Saved: the next snapshot captures exactly this".into());
                }
                Err(e) => app.set_toast(format!("Save failed: {e}").into()),
            }
        }
    });
}
