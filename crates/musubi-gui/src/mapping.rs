//! Zuordnungen prüfen und bearbeiten.

use iced::widget::{Column, button, column, progress_bar, row, scrollable, text, toggler};
use iced::{Center, Element, Fill, Length, Task};
use musubi_core::ReviewItem;
use musubi_core::domain::{LibraryItem, Mapping, MappingSource};

use crate::app::{App, Message};
use crate::{theme, ui};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MappingForm {
    pub media: String,
    pub season: String,
    pub start: String,
    pub end: String,
    pub offset: String,
}

impl MappingForm {
    pub fn new(mapping: &Mapping) -> Self {
        Self {
            media: mapping.media_id.to_string(),
            season: mapping.season.to_string(),
            start: mapping.ep_start.to_string(),
            end: mapping.ep_end.map(|e| e.to_string()).unwrap_or_default(),
            offset: mapping.offset.to_string(),
        }
    }

    pub fn build(&self, original: &Mapping) -> Result<Mapping, String> {
        let whole = |value: &str, name: &str| {
            value
                .trim()
                .parse::<u32>()
                .map_err(|_| format!("{name} must be a whole number"))
        };
        let last = match self.end.trim() {
            "" => None,
            value => Some(whole(value, "Last episode")?),
        };
        let first = whole(&self.start, "First episode")?;
        if last.is_some_and(|l| l < first) {
            return Err("The last episode must not be before the first.".to_owned());
        }
        Ok(Mapping {
            jellyfin_id: original.jellyfin_id.clone(),
            season: whole(&self.season, "Season")?,
            ep_start: first,
            ep_end: last,
            media_id: self
                .media
                .trim()
                .parse()
                .map_err(|_| "The ID must be a number".to_owned())?,
            offset: self
                .offset
                .trim()
                .parse()
                .map_err(|_| "Offset must be a whole number".to_owned())?,
            source: MappingSource::Manual,
            confidence: 1.0,
            title: original.title.clone(),
        })
    }
}

pub enum Modal {
    Map {
        item: Box<LibraryItem>,
        id: String,
        follow: bool,
        error: Option<String>,
    },
    Edit {
        original: Box<Mapping>,
        form: MappingForm,
        error: Option<String>,
    },
}

#[derive(Default)]
pub struct State {
    reviews: Vec<ReviewItem>,
    mappings: Vec<Mapping>,
    progress: Option<(usize, usize)>,
    expanded: Option<String>,
    modal: Option<Modal>,
}

impl State {
    pub fn close_modal(&mut self) -> bool {
        self.modal.take().is_some()
    }

    pub fn set_progress(&mut self, done: usize, total: usize) {
        self.progress = (done < total).then_some((done, total));
    }
}

#[derive(Debug, Clone)]
pub enum FormEdit {
    Media(String),
    Season(String),
    Start(String),
    End(String),
    Offset(String),
}

#[derive(Debug, Clone)]
pub enum Msg {
    Loaded(Result<(Vec<ReviewItem>, Vec<Mapping>), String>),
    MatchAll,
    Expand(String),
    MapTo(Box<LibraryItem>, i64, bool),
    OpenMap(Box<LibraryItem>),
    MapId(String),
    MapFollow(bool),
    SubmitMap,
    OpenEdit(Box<Mapping>),
    Edit(FormEdit),
    SubmitEdit,
    Unmap(String),
    Close,
}

pub fn load(app: &App) -> Task<Message> {
    let core = app.core.clone();
    ui::perform(
        async move { Ok((core.reviews().await?, core.all_mappings().await?)) },
        |r| Message::Mapping(Msg::Loaded(r)),
    )
}

pub fn open_map_dialog(app: &mut App, item: LibraryItem) {
    app.mapping.modal = Some(Modal::Map {
        item: Box::new(item),
        id: String::new(),
        follow: true,
        error: None,
    });
}

fn map_to(app: &App, item: LibraryItem, media: i64, follow: bool) -> Task<Message> {
    let core = app.core.clone();
    ui::perform(
        async move { core.map_manually(&item, media, follow).await },
        Message::Notify,
    )
}

pub fn update(app: &mut App, msg: Msg) -> Task<Message> {
    let core = app.core.clone();
    match msg {
        Msg::Loaded(Ok((reviews, mappings))) => {
            app.mapping.reviews = reviews;
            app.mapping.mappings = mappings;
        }
        Msg::Loaded(Err(error)) => app.toast(error, false),
        Msg::MatchAll => {
            return ui::perform(async move { core.match_all().await }, Message::Notify);
        }
        Msg::Expand(id) => {
            app.mapping.expanded = (app.mapping.expanded.as_ref() != Some(&id)).then_some(id);
        }
        Msg::MapTo(item, media, follow) => return map_to(app, *item, media, follow),
        Msg::OpenMap(item) => open_map_dialog(app, *item),
        Msg::MapId(value) => {
            if let Some(Modal::Map { id, .. }) = &mut app.mapping.modal {
                *id = value;
            }
        }
        Msg::MapFollow(value) => {
            if let Some(Modal::Map { follow, .. }) = &mut app.mapping.modal {
                *follow = value;
            }
        }
        Msg::SubmitMap => {
            if let Some(Modal::Map {
                item,
                id,
                follow,
                error,
            }) = &mut app.mapping.modal
            {
                match id.trim().parse::<i64>() {
                    Ok(media) => {
                        let (item, follow) = ((**item).clone(), *follow);
                        app.mapping.modal = None;
                        return map_to(app, item, media, follow);
                    }
                    Err(_) => *error = Some("The ID must be a number.".to_owned()),
                }
            }
        }
        Msg::OpenEdit(mapping) => {
            app.mapping.modal = Some(Modal::Edit {
                form: MappingForm::new(&mapping),
                original: mapping,
                error: None,
            });
        }
        Msg::Edit(edit) => {
            if let Some(Modal::Edit { form, .. }) = &mut app.mapping.modal {
                match edit {
                    FormEdit::Media(v) => form.media = v,
                    FormEdit::Season(v) => form.season = v,
                    FormEdit::Start(v) => form.start = v,
                    FormEdit::End(v) => form.end = v,
                    FormEdit::Offset(v) => form.offset = v,
                }
            }
        }
        Msg::SubmitEdit => {
            let siblings = app.mapping.mappings.clone();
            if let Some(Modal::Edit {
                original,
                form,
                error,
            }) = &mut app.mapping.modal
            {
                match form.build(original) {
                    Ok(updated) => {
                        let id = original.jellyfin_id.clone();
                        let mut all: Vec<Mapping> = siblings
                            .into_iter()
                            .filter(|m| m.jellyfin_id == id)
                            .collect();
                        match all.iter_mut().find(|m| {
                            m.season == original.season && m.ep_start == original.ep_start
                        }) {
                            Some(slot) => *slot = updated,
                            None => all.push(updated),
                        }
                        app.mapping.modal = None;
                        return ui::perform(
                            async move { core.set_mappings(&id, all).await },
                            Message::Notify,
                        );
                    }
                    Err(message) => *error = Some(message),
                }
            }
        }
        Msg::Unmap(id) => {
            return ui::perform(async move { core.unmap(&id).await }, Message::Notify);
        }
        Msg::Close => app.mapping.modal = None,
    }
    Task::none()
}

fn item_for<'a>(app: &'a App, id: &str) -> Option<&'a LibraryItem> {
    app.library
        .rows
        .iter()
        .find(|r| r.item.id == id)
        .map(|r| &r.item)
}

fn review_row<'a>(app: &'a App, review: &'a ReviewItem) -> Element<'a, Msg> {
    let open = app.mapping.expanded.as_deref() == Some(review.jellyfin_id.as_str());
    let header = button(
        row![
            text(ui::ellipsize(&review.title, 80)).size(15).width(Fill),
            ui::muted(format!("{} candidates", review.candidates.len())),
        ]
        .align_y(Center),
    )
    .width(Fill)
    .padding(8)
    .style(theme::card_button)
    .on_press(Msg::Expand(review.jellyfin_id.clone()));
    let mut body = column![header].spacing(4);
    if open {
        let item = item_for(app, &review.jellyfin_id);
        for candidate in &review.candidates {
            let details = format!(
                "{} · {} · {} eps · match {:.0}%",
                candidate.format.as_deref().unwrap_or("?"),
                candidate.year.map_or("?".to_owned(), |y| y.to_string()),
                candidate.episodes.map_or("?".to_owned(), |e| e.to_string()),
                candidate.score * 100.0
            );
            let mut line = row![
                column![
                    text(ui::ellipsize(&candidate.title, 70)).size(14),
                    ui::muted(details),
                ]
                .width(Fill)
            ]
            .align_y(Center);
            if let Some(item) = item {
                line = line.push(ui::primary(
                    "Map",
                    Msg::MapTo(Box::new(item.clone()), candidate.media_id, true),
                ));
            }
            body = body.push(iced::widget::container(line).padding([4, 12]));
        }
        if let Some(item) = item {
            body = body.push(
                iced::widget::container(ui::secondary(
                    "Enter an AniList ID…",
                    Msg::OpenMap(Box::new(item.clone())),
                ))
                .padding([4, 12]),
            );
        }
    }
    body.into()
}

fn mapped_row<'a>(app: &'a App, mapping: &'a Mapping) -> Element<'a, Msg> {
    let name = item_for(app, &mapping.jellyfin_id)
        .map_or(mapping.jellyfin_id.as_str(), |i| i.name.as_str());
    let details = format!(
        "S{} E{}-{} ({:+}) → {} [{}]",
        mapping.season,
        mapping.ep_start,
        mapping.ep_end.map_or("…".to_owned(), |e| e.to_string()),
        mapping.offset,
        ui::ellipsize(&mapping.title, 50),
        mapping.source.as_str()
    );
    row![
        button(column![text(ui::ellipsize(name, 70)).size(15), ui::muted(details)].spacing(2))
            .width(Fill)
            .padding(6)
            .style(theme::card_button)
            .on_press(Msg::OpenEdit(Box::new(mapping.clone()))),
        ui::secondary("Remove", Msg::Unmap(mapping.jellyfin_id.clone())),
    ]
    .spacing(8)
    .align_y(Center)
    .into()
}

pub fn view(app: &App) -> Element<'_, Message> {
    let state = &app.mapping;
    let mut body = column![
        row![
            ui::heading("Mapping"),
            ui::spacer(),
            ui::primary("Match all", Msg::MatchAll),
        ]
        .align_y(Center)
    ]
    .spacing(18)
    .padding(iced::padding::right(14));
    if let Some((done, total)) = state.progress {
        body = body.push(
            row![
                progress_bar(0.0..=total.max(1) as f32, done as f32)
                    .length(Length::Fill)
                    .girth(8),
                ui::muted(format!("{done} / {total}")),
            ]
            .spacing(10)
            .align_y(Center),
        );
    }
    let reviews: Vec<Element<'_, Msg>> = state.reviews.iter().map(|r| review_row(app, r)).collect();
    let mapped: Vec<Element<'_, Msg>> = state.mappings.iter().map(|m| mapped_row(app, m)).collect();
    body = body
        .push(text(format!("Needs review ({})", reviews.len())).size(16))
        .push(ui::card(column(reviews).spacing(4)))
        .push(text(format!("Mapped titles ({})", mapped.len())).size(16))
        .push(ui::card(column(mapped).spacing(4)));
    let page: Element<'_, Msg> = scrollable(body).width(Fill).height(Fill).into();
    let page = iced::widget::container(page).padding(24);
    Element::from(page).map(Message::Mapping)
}

fn map_body<'a>(id: &'a str, follow: bool, error: Option<&'a str>) -> Column<'a, Msg> {
    let mut body = column![
        ui::muted("The ID is the number in the anilist.co URL."),
        ui::field("AniList ID", id, Msg::MapId),
        toggler(follow)
            .label("Assign sequels to later seasons")
            .text_size(14)
            .on_toggle(Msg::MapFollow),
    ]
    .spacing(12);
    if let Some(error) = error {
        body = body.push(ui::error_line(error));
    }
    body.push(
        row![
            ui::spacer(),
            ui::secondary("Cancel", Msg::Close),
            ui::primary("Map", Msg::SubmitMap),
        ]
        .spacing(8),
    )
}

fn edit_body<'a>(form: &'a MappingForm, error: Option<&'a str>) -> Column<'a, Msg> {
    let edit = |wrap: fn(String) -> FormEdit| move |v: String| Msg::Edit(wrap(v));
    let mut body = column![
        ui::muted("AniList episode = Jellyfin episode + offset"),
        ui::field("AniList ID", &form.media, edit(FormEdit::Media)),
        ui::field("Season", &form.season, edit(FormEdit::Season)),
        ui::field("First episode", &form.start, edit(FormEdit::Start)),
        ui::field(
            "Last episode (empty = open)",
            &form.end,
            edit(FormEdit::End)
        ),
        ui::field("Offset", &form.offset, edit(FormEdit::Offset)),
    ]
    .spacing(12);
    if let Some(error) = error {
        body = body.push(ui::error_line(error));
    }
    body.push(
        row![
            ui::spacer(),
            ui::secondary("Cancel", Msg::Close),
            ui::primary("Save", Msg::SubmitEdit),
        ]
        .spacing(8),
    )
}

pub fn with_modal<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let Some(modal) = &app.mapping.modal else {
        return base;
    };
    match modal {
        Modal::Map {
            item,
            id,
            follow,
            error,
        } => ui::modal(
            base,
            &item.name,
            460.0,
            Element::from(map_body(id, *follow, error.as_deref())).map(Message::Mapping),
        ),
        Modal::Edit {
            original,
            form,
            error,
        } => ui::modal(
            base,
            &original.title,
            460.0,
            Element::from(edit_body(form, error.as_deref())).map(Message::Mapping),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping() -> Mapping {
        Mapping {
            jellyfin_id: "s".into(),
            season: 1,
            ep_start: 1,
            ep_end: Some(12),
            media_id: 9,
            offset: 0,
            source: MappingSource::Title,
            confidence: 0.8,
            title: "Show".into(),
        }
    }

    #[test]
    fn empty_last_episode_means_open_end_and_marks_manual() {
        let original = mapping();
        let mut form = MappingForm::new(&original);
        form.end.clear();
        let built = form.build(&original).unwrap();
        assert_eq!(built.ep_end, None);
        assert_eq!(built.source, MappingSource::Manual);
    }

    #[test]
    fn last_episode_before_first_is_rejected() {
        let original = mapping();
        let mut form = MappingForm::new(&original);
        form.start = "5".into();
        form.end = "2".into();
        assert!(form.build(&original).is_err());
    }
}
