//! Leiste für die laufende Wiedergabe.

use musubi_core::{PlaybackState, PlayerCommand};
use relm4::adw::prelude::*;
use relm4::{ComponentSender, gtk};

use crate::app::{App, Msg};
use crate::util::format_time;

const SEEK_STEP: f64 = 10.0;

pub struct PlayerBar {
    pub root: gtk::ActionBar,
    title: gtk::Label,
    time: gtk::Label,
    scale: gtk::Scale,
    pause: gtk::Button,
}

fn icon_button(
    sender: &ComponentSender<App>,
    icon: &str,
    tooltip: &str,
    command: PlayerCommand,
) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.add_css_class("flat");
    let input = sender.input_sender().clone();
    button.connect_clicked(move |_| input.emit(Msg::Player(command)));
    button
}

impl PlayerBar {
    pub fn new(sender: &ComponentSender<App>) -> Self {
        let root = gtk::ActionBar::new();
        root.set_revealed(false);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        controls.append(&icon_button(
            sender,
            "media-skip-backward-symbolic",
            "Previous episode",
            PlayerCommand::Previous,
        ));
        controls.append(&icon_button(
            sender,
            "media-seek-backward-symbolic",
            "Back 10 s",
            PlayerCommand::SeekRelative(-SEEK_STEP),
        ));
        let pause = icon_button(
            sender,
            "media-playback-pause-symbolic",
            "Play or pause",
            PlayerCommand::TogglePause,
        );
        controls.append(&pause);
        controls.append(&icon_button(
            sender,
            "media-seek-forward-symbolic",
            "Forward 10 s",
            PlayerCommand::SeekRelative(SEEK_STEP),
        ));
        controls.append(&icon_button(
            sender,
            "media-skip-forward-symbolic",
            "Next episode",
            PlayerCommand::Next,
        ));
        controls.append(&icon_button(
            sender,
            "media-playback-stop-symbolic",
            "Stop",
            PlayerCommand::Stop,
        ));
        root.pack_start(&controls);

        let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
        center.set_hexpand(true);
        let title = gtk::Label::new(None);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.add_css_class("heading");
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        scale.set_hexpand(true);
        let input = sender.input_sender().clone();
        scale.connect_change_value(move |_, _, value| {
            input.emit(Msg::Player(PlayerCommand::SeekAbsolute(value)));
            gtk::glib::Propagation::Proceed
        });
        center.append(&title);
        center.append(&scale);
        root.set_center_widget(Some(&center));

        let end = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        let time = gtk::Label::new(None);
        time.add_css_class("numeric");
        end.append(&time);
        end.append(&icon_button(
            sender,
            "media-view-subtitles-symbolic",
            "Cycle subtitles",
            PlayerCommand::CycleSubtitle,
        ));
        end.append(&icon_button(
            sender,
            "audio-x-generic-symbolic",
            "Cycle audio",
            PlayerCommand::CycleAudio,
        ));
        root.pack_end(&end);

        Self {
            root,
            title,
            time,
            scale,
            pause,
        }
    }

    pub fn update(&self, state: &PlaybackState) {
        self.root.set_revealed(true);
        self.title.set_text(&state.title);
        let duration = state.duration.unwrap_or(0.0).max(1.0);
        self.scale.set_range(0.0, duration);
        self.scale.set_value(state.position);
        self.time.set_text(&format!(
            "{} / {}",
            format_time(state.position),
            state.duration.map_or("?".to_owned(), format_time)
        ));
        self.pause.set_icon_name(if state.paused {
            "media-playback-start-symbolic"
        } else {
            "media-playback-pause-symbolic"
        });
    }

    pub fn hide(&self) {
        self.root.set_revealed(false);
    }
}
