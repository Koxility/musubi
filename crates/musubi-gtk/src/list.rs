//! AniList-Verwaltung: Liste, Eintragsdialog, Hinzufügen und Custom Lists.

use musubi_core::domain::{
    EntryPatch, FuzzyDate, ListEntry, ListStatus, Media, MediaKind, ParseDateError, ScoreFormat,
    Viewer,
};
use relm4::adw::prelude::*;
use relm4::typed_view::list::{RelmListItem, TypedListView};
use relm4::{ComponentSender, adw, gtk};

use crate::app::{App, Msg};
use crate::util::{self, MarginExt, boxed_list, clear_list, dropdown_text, present};

pub struct EntryItem {
    pub entry: ListEntry,
    format: ScoreFormat,
}

pub struct EntryWidgets {
    title: gtk::Label,
    status: gtk::Label,
    progress: gtk::Label,
    score: gtk::Label,
}

impl RelmListItem for EntryItem {
    type Root = gtk::Box;
    type Widgets = EntryWidgets;

    fn setup(_item: &gtk::ListItem) -> (gtk::Box, EntryWidgets) {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        root.set_margin_all(6);
        let title = gtk::Label::new(None);
        title.set_hexpand(true);
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let fixed = |chars: i32| {
            let label = gtk::Label::new(None);
            label.set_width_chars(chars);
            label.set_xalign(1.0);
            label.add_css_class("numeric");
            label
        };
        let (status, progress, score) = (fixed(11), fixed(9), fixed(6));
        root.append(&title);
        root.append(&status);
        root.append(&progress);
        root.append(&score);
        (
            root,
            EntryWidgets {
                title,
                status,
                progress,
                score,
            },
        )
    }

    fn bind(&mut self, widgets: &mut EntryWidgets, _root: &mut gtk::Box) {
        let e = &self.entry;
        widgets.title.set_text(e.media.title.preferred());
        widgets.status.set_text(e.status.label(e.media.kind));
        widgets.progress.set_text(&format!(
            "{}/{}",
            e.progress,
            e.media
                .total_units()
                .map_or("?".to_owned(), |t| t.to_string())
        ));
        widgets.score.set_text(&self.format.display(e.score_raw));
    }
}

pub struct ListPage {
    pub root: adw::ToolbarView,
    pub view: TypedListView<EntryItem, gtk::SingleSelection>,
    pub search: gtk::SearchEntry,
    status: gtk::DropDown,
    kind: gtk::DropDown,
}

impl ListPage {
    pub fn new(sender: &ComponentSender<App>) -> Self {
        let view = TypedListView::<EntryItem, gtk::SingleSelection>::new();
        view.view.set_single_click_activate(true);
        view.view.add_css_class("navigation-sidebar");
        let input = sender.input_sender().clone();
        view.view
            .connect_activate(move |_, position| input.emit(Msg::ActivateEntry(position)));
        let scroller = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .child(&view.view)
            .build();

        let search = gtk::SearchEntry::builder()
            .placeholder_text("Filter list")
            .hexpand(true)
            .build();
        let input = sender.input_sender().clone();
        search.connect_search_changed(move |_| input.emit(Msg::ListFilterChanged));

        let kind = gtk::DropDown::from_strings(&["Anime", "Manga"]);
        let input = sender.input_sender().clone();
        kind.connect_selected_notify(move |_| input.emit(Msg::KindChanged));

        let mut statuses = vec!["All"];
        statuses.extend(ListStatus::ALL.iter().map(|s| s.label(MediaKind::Anime)));
        let status = gtk::DropDown::from_strings(&statuses);
        let input = sender.input_sender().clone();
        status.connect_selected_notify(move |_| input.emit(Msg::ListFilterChanged));

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&search));
        header.pack_start(&kind);
        header.pack_start(&status);

        let add = gtk::Button::from_icon_name("list-add-symbolic");
        add.set_tooltip_text(Some("Add to list"));
        let input = sender.input_sender().clone();
        add.connect_clicked(move |_| input.emit(Msg::OpenAddDialog));
        header.pack_end(&add);

        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Reload from AniList"));
        let input = sender.input_sender().clone();
        refresh.connect_clicked(move |_| input.emit(Msg::RefreshList));
        header.pack_end(&refresh);

        let lists = gtk::Button::from_icon_name("view-list-bullet-symbolic");
        lists.set_tooltip_text(Some("Custom lists"));
        let input = sender.input_sender().clone();
        lists.connect_clicked(move |_| input.emit(Msg::OpenListsDialog));
        header.pack_end(&lists);

        let root = adw::ToolbarView::new();
        root.add_top_bar(&header);
        root.set_content(Some(&scroller));
        Self {
            root,
            view,
            search,
            status,
            kind,
        }
    }

    pub fn kind(&self) -> MediaKind {
        if dropdown_text(&self.kind).as_deref() == Some("Manga") {
            MediaKind::Manga
        } else {
            MediaKind::Anime
        }
    }

    pub fn set_entries(&mut self, entries: Vec<ListEntry>, format: ScoreFormat) {
        let kind = MediaKind::Anime;
        let status = dropdown_text(&self.status);
        let needle = self.search.text().to_lowercase();
        self.view.clear();
        self.view.extend_from_iter(
            entries
                .into_iter()
                .filter(|e| {
                    status
                        .as_deref()
                        .is_none_or(|s| s == "All" || s == e.status.label(kind))
                })
                .filter(|e| {
                    needle.is_empty()
                        || e.media
                            .title
                            .all()
                            .any(|t| t.to_lowercase().contains(&needle))
                })
                .map(|entry| EntryItem { entry, format }),
        );
    }

    pub fn entry_at(&self, position: u32) -> Option<ListEntry> {
        let item = self.view.get_visible(position)?;
        let item = item.borrow();
        Some(item.entry.clone())
    }
}

fn status_row(kind: MediaKind, current: ListStatus) -> adw::ComboRow {
    let labels: Vec<String> = ListStatus::ALL
        .iter()
        .map(|s| s.label(kind).to_owned())
        .collect();
    let row = adw::ComboRow::builder()
        .title("Status")
        .model(&util::string_list(&labels))
        .build();
    let index = ListStatus::ALL
        .iter()
        .position(|s| *s == current)
        .unwrap_or(0);
    row.set_selected(index as u32);
    row
}

fn number_row(title: &str, value: u32, max: Option<u32>) -> adw::SpinRow {
    let upper = f64::from(max.unwrap_or(100_000).max(value));
    let row = adw::SpinRow::with_range(0.0, upper, 1.0);
    row.set_title(title);
    row.set_value(f64::from(value));
    row
}

fn text_row(title: &str, value: &str) -> adw::EntryRow {
    let row = adw::EntryRow::builder().title(title).build();
    row.set_text(value);
    row
}

fn parse_date(row: &adw::EntryRow) -> Result<FuzzyDate, String> {
    row.text()
        .parse()
        .map_err(|e: ParseDateError| e.to_string())
}

/// Dialog mit allen bearbeitbaren Feldern eines Eintrags.
pub fn open_entry_dialog(entry: ListEntry, viewer: Option<Viewer>, sender: &ComponentSender<App>) {
    let format = viewer.as_ref().map(|v| v.score_format).unwrap_or_default();
    let kind = entry.media.kind;
    let total = entry.media.total_units();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_margin_all(18);

    let main = adw::PreferencesGroup::builder()
        .title(entry.media.title.preferred())
        .build();
    let status = status_row(kind, entry.status);
    let shown = if entry.score_raw == 0 {
        String::new()
    } else {
        format.display(entry.score_raw)
    };
    let score = text_row(&format!("Score ({})", format.hint()), &shown);
    let progress = number_row(
        if kind == MediaKind::Anime {
            "Episodes"
        } else {
            "Chapters"
        },
        entry.progress,
        total,
    );
    let volumes = number_row("Volumes", entry.progress_volumes, entry.media.volumes);
    let repeat = number_row("Repeats", entry.repeat, None);
    let priority = number_row("Priority", entry.priority, None);
    let started = text_row("Started (YYYY-MM-DD)", &entry.started_at.to_string());
    let completed = text_row("Completed (YYYY-MM-DD)", &entry.completed_at.to_string());
    main.add(&status);
    main.add(&score);
    main.add(&progress);
    if kind == MediaKind::Manga {
        main.add(&volumes);
    }
    main.add(&repeat);
    main.add(&priority);
    main.add(&started);
    main.add(&completed);
    content.append(&main);

    let visibility = adw::PreferencesGroup::builder()
        .title("Privacy and notes")
        .build();
    let private = adw::SwitchRow::builder()
        .title("Private")
        .active(entry.private)
        .build();
    let hidden = adw::SwitchRow::builder()
        .title("Hide from status lists")
        .active(entry.hidden_from_status_lists)
        .build();
    let notes = text_row("Notes", &entry.notes);
    visibility.add(&private);
    visibility.add(&hidden);
    visibility.add(&notes);
    content.append(&visibility);

    let mut advanced_rows: Vec<(String, adw::SpinRow)> = Vec::new();
    let categories = viewer
        .as_ref()
        .map(|v| v.advanced_scoring(kind).to_vec())
        .unwrap_or_default();
    if !categories.is_empty() {
        let group = adw::PreferencesGroup::builder()
            .title("Advanced scoring")
            .build();
        for name in categories {
            let current = entry
                .advanced_scores
                .iter()
                .find(|(n, _)| *n == name)
                .map_or(0.0, |(_, value)| *value);
            let row = adw::SpinRow::with_range(0.0, 100.0, 0.5);
            row.set_digits(1);
            row.set_title(gtk::glib::markup_escape_text(&name).as_str());
            row.set_value(current);
            group.add(&row);
            advanced_rows.push((name, row));
        }
        content.append(&group);
    }

    let mut list_rows: Vec<(String, adw::SwitchRow)> = Vec::new();
    let names = viewer
        .as_ref()
        .map(|v| v.custom_lists(kind).to_vec())
        .unwrap_or_default();
    if !names.is_empty() {
        let group = adw::PreferencesGroup::builder()
            .title("Custom lists")
            .build();
        for name in names {
            let row = adw::SwitchRow::builder()
                .title(gtk::glib::markup_escape_text(&name).as_str())
                .active(entry.custom_lists.contains(&name))
                .build();
            group.add(&row);
            list_rows.push((name, row));
        }
        content.append(&group);
    }

    let error = gtk::Label::new(None);
    error.add_css_class("error");
    error.set_wrap(true);
    content.append(&error);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::Center);
    let delete = gtk::Button::with_label("Remove from list");
    delete.add_css_class("destructive-action");
    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    buttons.append(&delete);
    buttons.append(&save);
    content.append(&buttons);

    let dialog = util::dialog("Edit entry", 520, 680, &content);

    let input = sender.input_sender().clone();
    let (target, window) = (entry.clone(), dialog.clone());
    delete.connect_clicked(move |_| {
        let confirm = adw::AlertDialog::new(
            Some("Remove from list?"),
            Some(target.media.title.preferred()),
        );
        confirm.add_responses(&[("cancel", "Cancel"), ("remove", "Remove")]);
        confirm.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
        confirm.set_close_response("cancel");
        let (input, target, window) = (input.clone(), target.clone(), window.clone());
        confirm.connect_response(None, move |_, response| {
            if response == "remove" {
                input.emit(Msg::DeleteEntry(target.clone()));
                window.close();
            }
        });
        present(&confirm);
    });

    let input = sender.input_sender().clone();
    let window = dialog.clone();
    save.connect_clicked(move |_| {
        let build = || -> Result<EntryPatch, String> {
            let mut patch = EntryPatch::new(entry.media.id);
            let chosen = ListStatus::ALL[status.selected() as usize];
            patch.status = (chosen != entry.status).then_some(chosen);
            let text = score.text();
            if text.trim() != format.display(entry.score_raw) {
                let raw = format.parse(&text).map_err(|e| e.to_string())?;
                patch.score_raw = (raw != entry.score_raw).then_some(raw);
            }
            let number =
                |row: &adw::SpinRow, old: u32| Some(row.value() as u32).filter(|v| *v != old);
            patch.progress = number(&progress, entry.progress);
            if kind == MediaKind::Manga {
                patch.progress_volumes = number(&volumes, entry.progress_volumes);
            }
            patch.repeat = number(&repeat, entry.repeat);
            patch.priority = number(&priority, entry.priority);
            let start = parse_date(&started)?;
            patch.started_at = (start != entry.started_at).then_some(start);
            let end = parse_date(&completed)?;
            patch.completed_at = (end != entry.completed_at).then_some(end);
            patch.private = Some(private.is_active()).filter(|v| *v != entry.private);
            patch.hidden_from_status_lists =
                Some(hidden.is_active()).filter(|v| *v != entry.hidden_from_status_lists);
            if notes.text() != entry.notes {
                patch.notes = Some(notes.text().to_string());
            }
            let changed = advanced_rows.iter().any(|(name, row)| {
                let old = entry
                    .advanced_scores
                    .iter()
                    .find(|(n, _)| n == name)
                    .map_or(0.0, |(_, value)| *value);
                (row.value() - old).abs() > f64::EPSILON
            });
            if changed {
                patch.advanced_scores =
                    Some(advanced_rows.iter().map(|(_, row)| row.value()).collect());
            }
            let enabled: Vec<String> = list_rows
                .iter()
                .filter(|(_, row)| row.is_active())
                .map(|(name, _)| name.clone())
                .collect();
            let mut before = entry.custom_lists.clone();
            before.sort();
            let mut after = enabled.clone();
            after.sort();
            if before != after {
                patch.custom_lists = Some(enabled);
            }
            Ok(patch)
        };
        match build() {
            Ok(patch) => {
                input.emit(Msg::SaveEntry(patch));
                window.close();
            }
            Err(message) => error.set_text(&message),
        }
    });
    present(&dialog);
}

/// Suche auf AniList. Die Ergebnisse kommen über [`AddDialog::show_results`].
pub struct AddDialog {
    results: gtk::ListBox,
    status: adw::ComboRow,
    dialog: adw::Dialog,
    kind: MediaKind,
}

impl AddDialog {
    pub fn open(kind: MediaKind, sender: &ComponentSender<App>) -> Self {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        content.set_margin_all(18);
        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search AniList")
            .build();
        let input = sender.input_sender().clone();
        search.connect_activate(move |entry| input.emit(Msg::SearchAdd(entry.text().to_string())));
        content.append(&search);

        let group = adw::PreferencesGroup::new();
        let status = status_row(kind, ListStatus::Planning);
        group.add(&status);
        content.append(&group);

        let results = boxed_list();
        content.append(&results);

        let dialog = util::dialog(&format!("Add {}", kind.label()), 520, 600, &content);
        present(&dialog);
        search.grab_focus();
        Self {
            results,
            status,
            dialog,
            kind,
        }
    }

    pub fn show_results(&self, media: Vec<Media>, sender: &ComponentSender<App>) {
        clear_list(&self.results);
        for item in media {
            let row = adw::ActionRow::builder()
                .title(gtk::glib::markup_escape_text(item.title.preferred()).as_str())
                .subtitle(format!(
                    "{} · {}",
                    item.format.as_deref().unwrap_or("?"),
                    item.season_year.map_or("?".to_owned(), |y| y.to_string())
                ))
                .activatable(true)
                .build();
            let input = sender.input_sender().clone();
            let (status, dialog) = (self.status.clone(), self.dialog.clone());
            row.connect_activated(move |_| {
                let chosen = ListStatus::ALL[status.selected() as usize];
                input.emit(Msg::AddMedia(Box::new(item.clone()), chosen));
                dialog.close();
            });
            self.results.append(&row);
        }
    }

    pub fn kind(&self) -> MediaKind {
        self.kind
    }
}

/// Custom Lists anlegen, umbenennen und löschen.
pub fn open_lists_dialog(kind: MediaKind, names: Vec<String>, sender: &ComponentSender<App>) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_all(18);
    let group = adw::PreferencesGroup::builder()
        .title(format!("{} custom lists", kind.label()))
        .build();
    let dialog = util::dialog("Custom lists", 460, 480, &content);

    for name in names {
        let row = adw::EntryRow::builder()
            .title("Name")
            .show_apply_button(true)
            .build();
        row.set_text(&name);
        let input = sender.input_sender().clone();
        let old = name.clone();
        row.connect_apply(move |row| {
            input.emit(Msg::RenameList(old.clone(), row.text().to_string()))
        });

        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.add_css_class("flat");
        remove.set_valign(gtk::Align::Center);
        let input = sender.input_sender().clone();
        let window = dialog.clone();
        remove.connect_clicked(move |_| {
            input.emit(Msg::DeleteList(name.clone()));
            window.close();
        });
        row.add_suffix(&remove);
        group.add(&row);
    }

    let add = adw::EntryRow::builder()
        .title("New list")
        .show_apply_button(true)
        .build();
    let input = sender.input_sender().clone();
    let window = dialog.clone();
    add.connect_apply(move |row| {
        input.emit(Msg::CreateList(row.text().to_string()));
        window.close();
    });
    group.add(&add);
    content.append(&group);
    present(&dialog);
}
