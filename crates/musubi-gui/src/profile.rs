//! AniList-Profil mit Statistik.

use iced::{Element, Task};

use crate::app::{App, Message};
use crate::icons::Icon;
use crate::ui;

#[derive(Default)]
pub struct State {}

#[derive(Debug, Clone)]
pub enum Msg {}

pub fn load(_: &mut App) -> Task<Message> {
    Task::none()
}

pub fn update(_: &mut App, msg: Msg) -> Task<Message> {
    match msg {}
}

pub fn view(_: &App) -> Element<'_, Message> {
    ui::empty_state(Icon::Profile, "Profile", "Statistics appear here.")
}
