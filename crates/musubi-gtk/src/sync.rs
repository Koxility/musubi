//! Manueller Abgleich, Warteschlange und Protokoll.

use std::cell::RefCell;
use std::rc::Rc;

use musubi_core::{OutboxItem, SyncDirection, SyncLogEntry, SyncRow};
use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};

use crate::app::{App, Msg};
use crate::util::{self, boxed_list, clear_list};

const DIRECTIONS: [(&str, SyncDirection); 3] = [
    ("To AniList", SyncDirection::Push),
    ("To Jellyfin", SyncDirection::Pull),
    ("Skip", SyncDirection::Skip),
];

pub struct SyncPage {
    pub root: adw::ToolbarView,
    diff: gtk::ListBox,
    log: gtk::ListBox,
    rows: Rc<RefCell<Vec<SyncRow>>>,
}

impl SyncPage {
    pub fn new(sender: &ComponentSender<App>) -> Self {
        let (page, content) = util::pref_page();
        let rows: Rc<RefCell<Vec<SyncRow>>> = Rc::default();

        let heading = |text: &str| {
            let label = gtk::Label::new(Some(text));
            label.add_css_class("title-4");
            label.set_xalign(0.0);
            label
        };
        let diff = boxed_list();
        diff.set_placeholder(Some(
            &gtk::Label::builder()
                .label("Jellyfin and AniList agree.")
                .margin_top(12)
                .margin_bottom(12)
                .build(),
        ));
        content.append(&heading("Differences"));
        content.append(&diff);
        let log = boxed_list();
        content.append(&heading("Queue and log"));
        content.append(&log);

        let header = adw::HeaderBar::new();
        let apply = gtk::Button::with_label("Apply");
        apply.add_css_class("suggested-action");
        let input = sender.input_sender().clone();
        let selected = rows.clone();
        apply.connect_clicked(move |_| {
            let chosen: Vec<SyncRow> = selected
                .borrow()
                .iter()
                .filter(|r| r.direction != SyncDirection::Skip)
                .cloned()
                .collect();
            if !chosen.is_empty() {
                input.emit(Msg::ApplySync(chosen));
            }
        });
        header.pack_start(&apply);

        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Compare again"));
        let input = sender.input_sender().clone();
        refresh.connect_clicked(move |_| input.emit(Msg::ReloadSync));
        header.pack_end(&refresh);

        let retry = gtk::Button::with_label("Retry queue");
        let input = sender.input_sender().clone();
        retry.connect_clicked(move |_| input.emit(Msg::FlushOutbox));
        header.pack_end(&retry);

        let root = adw::ToolbarView::new();
        root.add_top_bar(&header);
        root.set_content(Some(&page));
        Self {
            root,
            diff,
            log,
            rows,
        }
    }

    pub fn set_diff(&self, rows: Vec<SyncRow>) {
        clear_list(&self.diff);
        *self.rows.borrow_mut() = rows;
        let count = self.rows.borrow().len();
        for index in 0..count {
            let row = self.rows.borrow()[index].clone();
            let action = adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(&row.media_title).as_str())
                .subtitle(format!(
                    "Jellyfin {}  ·  AniList {}/{}",
                    row.jellyfin_progress,
                    row.anilist_progress,
                    row.total.map_or("?".to_owned(), |t| t.to_string())
                ))
                .build();
            let labels: Vec<&str> = DIRECTIONS.iter().map(|(l, _)| *l).collect();
            let dropdown = gtk::DropDown::from_strings(&labels);
            dropdown.set_valign(gtk::Align::Center);
            let selected = DIRECTIONS
                .iter()
                .position(|(_, d)| *d == row.direction)
                .unwrap_or(2);
            dropdown.set_selected(selected as u32);
            let rows = self.rows.clone();
            dropdown.connect_selected_notify(move |d| {
                if let Some(slot) = rows.borrow_mut().get_mut(index) {
                    slot.direction = DIRECTIONS[d.selected() as usize].1;
                }
            });
            action.add_suffix(&dropdown);
            self.diff.append(&action);
        }
    }

    pub fn set_status(&self, pending: Vec<OutboxItem>, log: Vec<SyncLogEntry>) {
        clear_list(&self.log);
        for item in pending {
            let row = adw::ActionRow::builder()
                .title(format!("Pending write for media {}", item.media_id))
                .subtitle(
                    gtk::glib::markup_escape_text(&format!(
                        "{} attempts{}",
                        item.attempts,
                        item.last_error
                            .map(|e| format!(": {e}"))
                            .unwrap_or_default()
                    ))
                    .as_str(),
                )
                .build();
            self.log.append(&row);
        }
        for entry in log {
            let when = chrono_label(entry.at);
            let row = adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(&entry.message).as_str())
                .subtitle(when)
                .build();
            if !entry.ok {
                row.add_css_class("error");
            }
            self.log.append(&row);
        }
    }
}

fn chrono_label(timestamp: i64) -> String {
    gtk::glib::DateTime::from_unix_local(timestamp)
        .ok()
        .and_then(|t| t.format("%F %R").ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}
