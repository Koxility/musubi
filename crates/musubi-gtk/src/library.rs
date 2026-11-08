//! Bibliothek: Poster-Raster, Filter und Episodenseite.

use std::cell::RefCell;
use std::rc::Rc;

use musubi_core::domain::{
    ItemKind, Library, LibraryFilters, LibraryItem, LibraryQuery, LibrarySort, ListEntry,
    ListStatus, MediaKind,
};
use musubi_core::{Core, EpisodeRow, LibraryRow};
use relm4::adw::prelude::*;
use relm4::typed_view::grid::{RelmGridItem, TypedGridView};
use relm4::{ComponentSender, adw, gtk};

use crate::app::{App, Msg};
use crate::util::{MarginExt, boxed_list, clear_list, dropdown_text, format_time, string_list};

pub fn badge_class(status: ListStatus) -> &'static str {
    match status {
        ListStatus::Current | ListStatus::Repeating => "watching",
        ListStatus::Completed => "completed",
        ListStatus::Paused => "paused",
        ListStatus::Dropped => "dropped",
        ListStatus::Planning => "planning",
    }
}

const BADGE_CLASSES: [&str; 6] = [
    "watching",
    "completed",
    "paused",
    "dropped",
    "planning",
    "review",
];

pub struct PosterItem {
    row: LibraryRow,
    core: Core,
}

pub struct PosterWidgets {
    picture: gtk::Picture,
    title: gtk::Label,
    subtitle: gtk::Label,
    badge: gtk::Label,
    bound: Rc<RefCell<String>>,
}

impl RelmGridItem for PosterItem {
    type Root = gtk::Box;
    type Widgets = PosterWidgets;

    fn setup(_item: &gtk::ListItem) -> (gtk::Box, PosterWidgets) {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 4);
        root.set_margin_all(6);
        root.set_width_request(150);

        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .width_request(150)
            .height_request(220)
            .build();
        picture.add_css_class("musubi-poster");
        picture.set_overflow(gtk::Overflow::Hidden);

        let badge = gtk::Label::new(None);
        badge.add_css_class("musubi-badge");
        badge.set_halign(gtk::Align::Start);
        badge.set_valign(gtk::Align::Start);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&picture));
        overlay.add_overlay(&badge);

        let title = gtk::Label::new(None);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(18);
        title.set_xalign(0.0);
        let subtitle = gtk::Label::new(None);
        subtitle.add_css_class("dim-label");
        subtitle.add_css_class("caption");
        subtitle.set_xalign(0.0);

        root.append(&overlay);
        root.append(&title);
        root.append(&subtitle);
        let widgets = PosterWidgets {
            picture,
            title,
            subtitle,
            badge,
            bound: Rc::default(),
        };
        (root, widgets)
    }

    fn bind(&mut self, widgets: &mut PosterWidgets, _root: &mut gtk::Box) {
        let item = &self.row.item;
        widgets.title.set_text(&item.name);
        widgets.title.set_tooltip_text(Some(&item.name));
        let mut subtitle = item.year.map(|y| y.to_string()).unwrap_or_default();
        if let Some(entry) = &self.row.entry {
            let total = entry
                .media
                .episodes
                .map_or("?".to_owned(), |t| t.to_string());
            subtitle.push_str(&format!("  {}/{total}", entry.progress));
        }
        widgets.subtitle.set_text(&subtitle);

        for class in BADGE_CLASSES {
            widgets.badge.remove_css_class(class);
        }
        match &self.row.entry {
            Some(entry) => {
                widgets.badge.set_text(entry.status.label(MediaKind::Anime));
                widgets.badge.add_css_class(badge_class(entry.status));
                widgets.badge.set_visible(true);
            }
            None if self.row.needs_review => {
                widgets.badge.set_text("Review");
                widgets.badge.add_css_class("review");
                widgets.badge.set_visible(true);
            }
            None => widgets.badge.set_visible(false),
        }

        // Zellen werden wiederverwendet. Nur das zuletzt gebundene Poster darf ankommen.
        widgets.picture.set_paintable(gtk::gdk::Paintable::NONE);
        *widgets.bound.borrow_mut() = item.id.clone();
        if !item.has_poster {
            return;
        }
        let (core, id) = (self.core.clone(), item.id.clone());
        let (picture, bound, expected) = (
            widgets.picture.clone(),
            widgets.bound.clone(),
            item.id.clone(),
        );
        relm4::spawn_local(async move {
            let texture = relm4::spawn(async move {
                let path = core.poster(&id).await.ok()?;
                tokio::task::spawn_blocking(move || gtk::gdk::Texture::from_filename(path).ok())
                    .await
                    .ok()
                    .flatten()
            })
            .await
            .ok()
            .flatten();
            if *bound.borrow() == expected {
                picture.set_paintable(texture.as_ref());
            }
        });
    }
}

pub struct LibraryPage {
    pub root: adw::NavigationView,
    pub grid: TypedGridView<PosterItem, gtk::SingleSelection>,
    scroller: gtk::ScrolledWindow,
    pub search: gtk::SearchEntry,
    content: gtk::Stack,
    library_drop: gtk::DropDown,
    genre_drop: gtk::DropDown,
    status_drop: gtk::DropDown,
    kind_drop: gtk::DropDown,
    sort_drop: gtk::DropDown,
    year: adw::EntryRow,
    unplayed: adw::SwitchRow,
    libraries: Vec<Library>,
    detail: Option<DetailPage>,
}

struct DetailPage {
    item: LibraryItem,
    list: gtk::ListBox,
    anilist: adw::ActionRow,
}

fn drop_row(title: &str, dropdown: &gtk::DropDown) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    dropdown.set_valign(gtk::Align::Center);
    row.add_suffix(dropdown);
    row
}

fn strings_drop(items: &[&str]) -> gtk::DropDown {
    gtk::DropDown::from_strings(items)
}

impl LibraryPage {
    pub fn new(sender: &ComponentSender<App>) -> Self {
        let grid = TypedGridView::<PosterItem, gtk::SingleSelection>::new();
        grid.view.set_min_columns(2);
        grid.view.set_max_columns(12);
        grid.view.set_single_click_activate(true);
        let input = sender.input_sender().clone();
        grid.view
            .connect_activate(move |_, position| input.emit(Msg::ActivateItem(position)));

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&grid.view)
            .build();

        let empty = adw::StatusPage::builder()
            .icon_name("video-x-generic-symbolic")
            .title("Nothing here yet")
            .description("Log in to Jellyfin in Settings, or adjust the filters.")
            .build();
        let content = gtk::Stack::new();
        content.add_named(&scroller, Some("grid"));
        content.add_named(&empty, Some("empty"));
        content.set_visible_child_name("empty");

        let search = gtk::SearchEntry::builder()
            .placeholder_text("Search library")
            .hexpand(true)
            .build();
        let input = sender.input_sender().clone();
        search.connect_search_changed(move |_| input.emit(Msg::ReloadLibrary));

        let library_drop = strings_drop(&["All"]);
        let genre_drop = strings_drop(&["All"]);
        let status_drop = strings_drop(&["Any", "Continuing", "Ended"]);
        let kind_drop = strings_drop(&["All", "Series", "Movies"]);
        let sort_drop = strings_drop(&["Name", "Year", "Date added", "Rating"]);
        for dropdown in [
            &library_drop,
            &genre_drop,
            &status_drop,
            &kind_drop,
            &sort_drop,
        ] {
            let input = sender.input_sender().clone();
            dropdown.connect_selected_notify(move |_| input.emit(Msg::ReloadLibrary));
        }
        let year = adw::EntryRow::builder()
            .title("Years (comma separated)")
            .show_apply_button(true)
            .build();
        let input = sender.input_sender().clone();
        year.connect_apply(move |_| input.emit(Msg::ReloadLibrary));
        let unplayed = adw::SwitchRow::builder().title("Unplayed only").build();
        let input = sender.input_sender().clone();
        unplayed.connect_active_notify(move |_| input.emit(Msg::ReloadLibrary));

        let filters = adw::PreferencesGroup::builder()
            .title("Filters")
            .width_request(320)
            .build();
        filters.add(&drop_row("Library", &library_drop));
        filters.add(&drop_row("Genre", &genre_drop));
        filters.add(&drop_row("Status", &status_drop));
        filters.add(&drop_row("Type", &kind_drop));
        filters.add(&drop_row("Sort", &sort_drop));
        filters.add(&year);
        filters.add(&unplayed);
        let popover = gtk::Popover::builder().child(&filters).build();
        let filter_button = gtk::MenuButton::builder()
            .label("Filters")
            .tooltip_text("Filters")
            .popover(&popover)
            .build();

        let match_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        match_button.set_tooltip_text(Some("Refresh"));
        let input = sender.input_sender().clone();
        match_button.connect_clicked(move |_| input.emit(Msg::ReloadLibrary));

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&search));
        header.pack_start(&filter_button);
        header.pack_end(&match_button);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&content));

        let root = adw::NavigationView::new();
        root.push(&adw::NavigationPage::new(&toolbar, "Library"));

        Self {
            root,
            grid,
            scroller,
            search,
            content,
            library_drop,
            genre_drop,
            status_drop,
            kind_drop,
            sort_drop,
            year,
            unplayed,
            libraries: vec![],
            detail: None,
        }
    }

    pub fn query(&self) -> LibraryQuery {
        let library = dropdown_text(&self.library_drop);
        let genre = dropdown_text(&self.genre_drop).filter(|g| g != "All");
        let status = dropdown_text(&self.status_drop).filter(|s| s != "Any");
        let search = self.search.text().to_string();
        LibraryQuery {
            library_id: self
                .libraries
                .iter()
                .find(|l| Some(&l.name) == library.as_ref())
                .map(|l| l.id.clone()),
            search: (!search.is_empty()).then_some(search),
            genres: genre.into_iter().collect(),
            years: self
                .year
                .text()
                .split(',')
                .filter_map(|y| y.trim().parse().ok())
                .collect(),
            status,
            kind: match dropdown_text(&self.kind_drop).as_deref() {
                Some("Series") => Some(ItemKind::Series),
                Some("Movies") => Some(ItemKind::Movie),
                _ => None,
            },
            unplayed_only: self.unplayed.is_active(),
            sort: match dropdown_text(&self.sort_drop).as_deref() {
                Some("Year") => LibrarySort::Year,
                Some("Date added") => LibrarySort::DateAdded,
                Some("Rating") => LibrarySort::Rating,
                _ => LibrarySort::Name,
            },
            start: 0,
            limit: None,
        }
    }

    pub fn set_choices(&mut self, filters: LibraryFilters, libraries: Vec<Library>) {
        let mut genres = vec!["All".to_owned()];
        genres.extend(filters.genres);
        self.genre_drop.set_model(Some(&string_list(&genres)));
        let mut names = vec!["All".to_owned()];
        names.extend(libraries.iter().map(|l| l.name.clone()));
        self.library_drop.set_model(Some(&string_list(&names)));
        self.libraries = libraries;
    }

    pub fn set_rows(&mut self, rows: Vec<LibraryRow>, core: &Core) {
        let adjustment = self.scroller.vadjustment();
        let scroll = adjustment.value();
        self.content
            .set_visible_child_name(if rows.is_empty() { "empty" } else { "grid" });
        self.grid.clear();
        self.grid
            .extend_from_iter(rows.into_iter().map(|row| PosterItem {
                row,
                core: core.clone(),
            }));
        gtk::glib::idle_add_local_once(move || adjustment.set_value(scroll));
    }

    pub fn item_at(&self, position: u32) -> Option<LibraryItem> {
        let item = self.grid.get_visible(position)?;
        let item = item.borrow();
        Some(item.row.item.clone())
    }

    pub fn open_detail(&mut self, item: LibraryItem, sender: &ComponentSender<App>) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
        content.set_margin_all(18);

        let description = gtk::Label::new(item.overview.as_deref());
        description.set_wrap(true);
        description.set_xalign(0.0);
        content.append(&description);

        if !item.genres.is_empty() {
            let genres = gtk::Label::new(Some(&item.genres.join(" · ")));
            genres.add_css_class("dim-label");
            genres.set_xalign(0.0);
            content.append(&genres);
        }

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let play = gtk::Button::with_label("Continue");
        play.add_css_class("suggested-action");
        let input = sender.input_sender().clone();
        let target = item.clone();
        play.connect_clicked(move |_| input.emit(Msg::Play(target.clone(), None)));
        actions.append(&play);
        let map = gtk::Button::with_label("Map to AniList…");
        let input = sender.input_sender().clone();
        let target = item.clone();
        map.connect_clicked(move |_| input.emit(Msg::OpenMapDialog(target.clone())));
        actions.append(&map);
        content.append(&actions);

        let anilist_group = boxed_list();
        let anilist = adw::ActionRow::builder()
            .title("AniList")
            .subtitle("Not mapped")
            .activatable(true)
            .build();
        anilist.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let input = sender.input_sender().clone();
        let target = item.clone();
        anilist.connect_activated(move |_| input.emit(Msg::EditEntryFor(target.clone())));
        anilist_group.append(&anilist);
        content.append(&anilist_group);

        let list = boxed_list();
        content.append(&list);

        let clamp = adw::Clamp::builder()
            .maximum_size(860)
            .child(&content)
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&scroller));
        self.root
            .push(&adw::NavigationPage::new(&toolbar, &item.name));
        self.detail = Some(DetailPage {
            item,
            list,
            anilist,
        });
    }

    pub fn current_detail(&self) -> Option<LibraryItem> {
        let visible = self.root.visible_page()?;
        let detail = self.detail.as_ref()?;
        (visible.title() == detail.item.name).then(|| detail.item.clone())
    }

    pub fn set_anilist_summary(&self, entry: Option<&ListEntry>, mapped: bool) {
        let Some(detail) = &self.detail else { return };
        let text = match entry {
            Some(e) => format!(
                "{} · {}/{}",
                e.status.label(MediaKind::Anime),
                e.progress,
                e.media.episodes.map_or("?".to_owned(), |t| t.to_string())
            ),
            None if mapped => "Mapped, not on your list".to_owned(),
            None => "Not mapped".to_owned(),
        };
        detail.anilist.set_subtitle(&text);
    }

    pub fn set_episodes(
        &self,
        for_item: &str,
        episodes: Vec<EpisodeRow>,
        sender: &ComponentSender<App>,
    ) {
        let Some(detail) = self.detail.as_ref().filter(|d| d.item.id == for_item) else {
            return;
        };
        clear_list(&detail.list);
        for row in episodes {
            let e = row.episode;
            let title = format!(
                "S{:02}E{:02} · {}",
                e.season_number,
                e.number,
                gtk::glib::markup_escape_text(&e.name)
            );
            let mut subtitle = e
                .runtime_secs
                .map(|s| format_time(s as f64))
                .unwrap_or_default();
            if e.position_secs > 0 && !e.played {
                subtitle.push_str(&format!(
                    "  ·  resume at {}",
                    format_time(e.position_secs as f64)
                ));
            }
            if row.counted_on_anilist {
                subtitle.push_str("  ·  on AniList");
            }
            let action = adw::ActionRow::builder()
                .title(title)
                .subtitle(subtitle)
                .activatable(true)
                .build();
            let input = sender.input_sender().clone();
            let (target, id) = (detail.item.clone(), e.id.clone());
            action.connect_activated(move |_| {
                input.emit(Msg::Play(target.clone(), Some(id.clone())))
            });

            let seen = gtk::ToggleButton::builder()
                .icon_name("object-select-symbolic")
                .active(e.played)
                .valign(gtk::Align::Center)
                .tooltip_text("Watched")
                .build();
            seen.add_css_class("flat");
            let input = sender.input_sender().clone();
            let id = e.id.clone();
            seen.connect_clicked(move |button| {
                input.emit(Msg::SetPlayed(id.clone(), button.is_active()))
            });
            action.add_suffix(&seen);
            detail.list.append(&action);
        }
    }
}
