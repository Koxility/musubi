//! Zuordnungen prüfen und bearbeiten.

use std::collections::HashMap;

use musubi_core::ReviewItem;
use musubi_core::domain::{LibraryItem, Mapping, MappingSource};
use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};

use crate::app::{App, Msg};
use crate::util::{self, MarginExt, boxed_list, clear_list, present};

pub struct MappingPage {
    pub root: adw::ToolbarView,
    reviews: gtk::ListBox,
    mapped: gtk::ListBox,
    progress: gtk::ProgressBar,
}

impl MappingPage {
    pub fn new(sender: &ComponentSender<App>) -> Self {
        let (page, content) = util::pref_page();

        let progress = gtk::ProgressBar::new();
        progress.set_visible(false);
        progress.set_show_text(true);
        content.append(&progress);

        let review_title = gtk::Label::new(Some("Needs review"));
        review_title.add_css_class("title-4");
        review_title.set_xalign(0.0);
        let reviews = boxed_list();
        content.append(&review_title);
        content.append(&reviews);

        let mapped_title = gtk::Label::new(Some("Mapped titles"));
        mapped_title.add_css_class("title-4");
        mapped_title.set_xalign(0.0);
        let mapped = boxed_list();
        content.append(&mapped_title);
        content.append(&mapped);

        let header = adw::HeaderBar::new();
        let run = gtk::Button::with_label("Match all");
        run.add_css_class("suggested-action");
        let input = sender.input_sender().clone();
        run.connect_clicked(move |_| input.emit(Msg::MatchAll));
        header.pack_start(&run);

        let root = adw::ToolbarView::new();
        root.add_top_bar(&header);
        root.set_content(Some(&page));
        Self {
            root,
            reviews,
            mapped,
            progress,
        }
    }

    pub fn set_progress(&self, done: usize, total: usize) {
        self.progress.set_visible(done < total);
        if total > 0 {
            self.progress.set_fraction(done as f64 / total as f64);
        }
        self.progress.set_text(Some(&format!("{done} / {total}")));
    }

    pub fn set_data(
        &self,
        reviews: Vec<ReviewItem>,
        mappings: Vec<Mapping>,
        items: &HashMap<String, LibraryItem>,
        sender: &ComponentSender<App>,
    ) {
        clear_list(&self.reviews);
        for review in reviews {
            let row = adw::ExpanderRow::builder()
                .title(gtk::glib::markup_escape_text(&review.title).as_str())
                .subtitle(format!("{} candidates", review.candidates.len()))
                .build();
            for candidate in &review.candidates {
                let action = adw::ActionRow::builder()
                    .title(gtk::glib::markup_escape_text(&candidate.title).as_str())
                    .subtitle(format!(
                        "{} · {} · {} eps · match {:.0}%",
                        candidate.format.as_deref().unwrap_or("?"),
                        candidate.year.map_or("?".to_owned(), |y| y.to_string()),
                        candidate.episodes.map_or("?".to_owned(), |e| e.to_string()),
                        candidate.score * 100.0
                    ))
                    .activatable(true)
                    .build();
                if let Some(item) = items.get(&review.jellyfin_id).cloned() {
                    let input = sender.input_sender().clone();
                    let media = candidate.media_id;
                    action.connect_activated(move |_| {
                        input.emit(Msg::MapTo(Box::new(item.clone()), media, true));
                    });
                }
                row.add_row(&action);
            }
            if let Some(item) = items.get(&review.jellyfin_id).cloned() {
                let other = adw::ActionRow::builder()
                    .title("Enter an AniList ID…")
                    .activatable(true)
                    .build();
                let input = sender.input_sender().clone();
                other.connect_activated(move |_| input.emit(Msg::OpenMapDialog(item.clone())));
                row.add_row(&other);
            }
            self.reviews.append(&row);
        }

        clear_list(&self.mapped);
        for mapping in mappings {
            let name = items
                .get(&mapping.jellyfin_id)
                .map_or(mapping.jellyfin_id.clone(), |i| i.name.clone());
            let row = adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(&name).as_str())
                .subtitle(
                    gtk::glib::markup_escape_text(&format!(
                        "S{} E{}-{} ({:+}) → {} [{}]",
                        mapping.season,
                        mapping.ep_start,
                        mapping.ep_end.map_or("…".to_owned(), |e| e.to_string()),
                        mapping.offset,
                        mapping.title,
                        mapping.source.as_str()
                    ))
                    .as_str(),
                )
                .activatable(true)
                .build();
            let input = sender.input_sender().clone();
            let target = mapping.clone();
            row.connect_activated(move |_| {
                input.emit(Msg::OpenMappingEditor(Box::new(target.clone())))
            });

            let remove = gtk::Button::from_icon_name("user-trash-symbolic");
            remove.add_css_class("flat");
            remove.set_valign(gtk::Align::Center);
            remove.set_tooltip_text(Some("Remove all mappings of this title"));
            let input = sender.input_sender().clone();
            let id = mapping.jellyfin_id.clone();
            remove.connect_clicked(move |_| input.emit(Msg::Unmap(id.clone())));
            row.add_suffix(&remove);
            self.mapped.append(&row);
        }
    }
}

/// Weist einem Titel per AniList-ID ein Werk zu.
pub fn open_map_dialog(item: LibraryItem, sender: &ComponentSender<App>) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_all(18);
    let group = adw::PreferencesGroup::builder()
        .title(gtk::glib::markup_escape_text(&item.name).as_str())
        .description("The ID is the number in the anilist.co URL.")
        .build();
    let id = adw::EntryRow::builder().title("AniList ID").build();
    let follow = adw::SwitchRow::builder()
        .title("Assign sequels to later seasons")
        .active(true)
        .build();
    group.add(&id);
    group.add(&follow);
    content.append(&group);

    let error = gtk::Label::new(None);
    error.add_css_class("error");
    content.append(&error);
    let save = util::suggested_button("Map");
    content.append(&save);

    let dialog = util::dialog("Map to AniList", 440, 340, &content);
    let input = sender.input_sender().clone();
    let window = dialog.clone();
    save.connect_clicked(move |_| match id.text().trim().parse::<i64>() {
        Ok(media) => {
            input.emit(Msg::MapTo(
                Box::new(item.clone()),
                media,
                follow.is_active(),
            ));
            window.close();
        }
        Err(_) => error.set_text("The ID must be a number."),
    });
    present(&dialog);
}

/// Bearbeitet Staffel, Bereich und Offset einer Zuordnung exakt.
pub fn open_mapping_editor(
    mapping: Mapping,
    siblings: Vec<Mapping>,
    sender: &ComponentSender<App>,
) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_all(18);
    let group = adw::PreferencesGroup::builder()
        .title(gtk::glib::markup_escape_text(&mapping.title).as_str())
        .description("AniList episode = Jellyfin episode + offset")
        .build();
    let text = |title: &str, value: String| {
        let row = adw::EntryRow::builder().title(title).build();
        row.set_text(&value);
        group.add(&row);
        row
    };
    let media = text("AniList ID", mapping.media_id.to_string());
    let season = text("Season", mapping.season.to_string());
    let start = text("First episode", mapping.ep_start.to_string());
    let end = text(
        "Last episode (empty = open)",
        mapping.ep_end.map(|e| e.to_string()).unwrap_or_default(),
    );
    let offset = text("Offset", mapping.offset.to_string());
    content.append(&group);

    let error = gtk::Label::new(None);
    error.add_css_class("error");
    content.append(&error);
    let save = util::suggested_button("Save");
    content.append(&save);

    let dialog = util::dialog("Edit mapping", 440, 460, &content);
    let input = sender.input_sender().clone();
    let window = dialog.clone();
    save.connect_clicked(move |_| {
        let build = || -> Result<Mapping, String> {
            let number = |row: &adw::EntryRow, name: &str| {
                row.text()
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| format!("{name} must be a whole number"))
            };
            let last = match end.text().trim() {
                "" => None,
                text => Some(
                    text.parse::<u32>()
                        .map_err(|_| "Last episode must be a whole number".to_owned())?,
                ),
            };
            let first = number(&start, "First episode")?;
            if last.is_some_and(|l| l < first) {
                return Err("The last episode must not be before the first.".to_owned());
            }
            Ok(Mapping {
                jellyfin_id: mapping.jellyfin_id.clone(),
                season: number(&season, "Season")?,
                ep_start: first,
                ep_end: last,
                media_id: media
                    .text()
                    .trim()
                    .parse()
                    .map_err(|_| "The ID must be a number".to_owned())?,
                offset: offset
                    .text()
                    .trim()
                    .parse()
                    .map_err(|_| "Offset must be a whole number".to_owned())?,
                source: MappingSource::Manual,
                confidence: 1.0,
                title: mapping.title.clone(),
            })
        };
        match build() {
            Ok(updated) => {
                let mut all = siblings.clone();
                match all
                    .iter_mut()
                    .find(|m| m.season == mapping.season && m.ep_start == mapping.ep_start)
                {
                    Some(slot) => *slot = updated,
                    None => all.push(updated),
                }
                input.emit(Msg::SetMappings(mapping.jellyfin_id.clone(), all));
                window.close();
            }
            Err(message) => error.set_text(&message),
        }
    });
    present(&dialog);
}
