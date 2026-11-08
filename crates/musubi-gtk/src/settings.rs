//! Anmeldung und Einstellungen.

use musubi_core::oauth;
use musubi_core::{AuthStatus, Config};
use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};
use secrecy::SecretString;

use crate::app::{App, Msg};
use crate::util;

pub struct SettingsPage {
    pub root: adw::ToolbarView,
    jellyfin_status: adw::ActionRow,
    url: adw::EntryRow,
    anilist_status: adw::ActionRow,
    client_id: adw::EntryRow,
    redirect: adw::ActionRow,
    auto_sync: adw::SwitchRow,
    threshold: adw::SpinRow,
    mpv_path: adw::EntryRow,
    hwdec: adw::EntryRow,
    autoplay: adw::SwitchRow,
}

pub fn button_row(
    group: &adw::PreferencesGroup,
    label: &str,
    on_click: impl Fn() + 'static,
) -> gtk::Button {
    let row = adw::ActionRow::new();
    let button = gtk::Button::with_label(label);
    button.set_valign(gtk::Align::Center);
    button.connect_clicked(move |_| on_click());
    row.add_suffix(&button);
    group.add(&row);
    button
}

pub fn jellyfin_status_text(auth: &AuthStatus) -> String {
    match (&auth.jellyfin_user, &auth.jellyfin_url) {
        (Some(user), Some(url)) => format!("{user} on {url}"),
        _ => "Not logged in".to_owned(),
    }
}

pub fn anilist_status_text(auth: &AuthStatus) -> String {
    match (&auth.anilist_user, auth.anilist_logged_in) {
        (Some(user), _) => user.clone(),
        (None, true) => "Logged in".to_owned(),
        _ => "Not logged in".to_owned(),
    }
}

impl SettingsPage {
    pub fn new(sender: &ComponentSender<App>) -> Self {
        let (page, content) = util::pref_page();

        let setup_group = adw::PreferencesGroup::builder()
            .title("Setup")
            .description("Walk through Jellyfin, AniList and sync step by step.")
            .build();
        {
            let input = sender.input_sender().clone();
            button_row(&setup_group, "Run setup", move || {
                input.emit(Msg::OpenSetup)
            });
        }
        content.append(&setup_group);

        // Jellyfin
        let jellyfin_group = adw::PreferencesGroup::builder().title("Jellyfin").build();
        let jellyfin_status = adw::ActionRow::builder()
            .title("Status")
            .subtitle("Not logged in")
            .build();
        let url = adw::EntryRow::builder().title("Server URL").build();
        let user = adw::EntryRow::builder().title("Username").build();
        let password = adw::PasswordEntryRow::builder()
            .title("Password (never stored)")
            .build();
        let api_key = adw::PasswordEntryRow::builder().title("or API key").build();
        jellyfin_group.add(&jellyfin_status);
        jellyfin_group.add(&url);
        jellyfin_group.add(&user);
        jellyfin_group.add(&password);
        jellyfin_group.add(&api_key);
        {
            let input = sender.input_sender().clone();
            let (url, user, password, api_key) =
                (url.clone(), user.clone(), password.clone(), api_key.clone());
            button_row(&jellyfin_group, "Log in", move || {
                let key = api_key.text().to_string();
                let (server, name) = (url.text().to_string(), user.text().to_string());
                input.emit(if key.is_empty() {
                    Msg::LoginJellyfin(
                        server,
                        name,
                        SecretString::from(password.text().to_string()),
                    )
                } else {
                    Msg::LoginJellyfinKey(server, name, SecretString::from(key))
                });
                password.set_text("");
                api_key.set_text("");
            });
        }
        {
            let input = sender.input_sender().clone();
            button_row(&jellyfin_group, "Log out", move || {
                input.emit(Msg::LogoutJellyfin)
            });
        }
        content.append(&jellyfin_group);

        // AniList
        let anilist_group = adw::PreferencesGroup::builder()
            .title("AniList")
            .description("Create an app at anilist.co/settings/developer and enter its client ID.")
            .build();
        let anilist_status = adw::ActionRow::builder()
            .title("Status")
            .subtitle("Not logged in")
            .build();
        let client_id = adw::EntryRow::builder().title("Client ID").build();
        let redirect = adw::ActionRow::builder()
            .title("Redirect URL for your AniList app")
            .subtitle_selectable(true)
            .build();
        let token = adw::PasswordEntryRow::builder()
            .title("Token or redirect URL (PIN flow)")
            .build();
        anilist_group.add(&anilist_status);
        anilist_group.add(&client_id);
        anilist_group.add(&redirect);
        anilist_group.add(&token);
        {
            let input = sender.input_sender().clone();
            let client_id = client_id.clone();
            button_row(&anilist_group, "Log in with browser", move || {
                input.emit(Msg::LoginAniListBrowser(client_id.text().trim().to_owned()));
            });
        }
        {
            let input = sender.input_sender().clone();
            let token = token.clone();
            button_row(&anilist_group, "Use pasted token", move || {
                input.emit(Msg::LoginAniListToken(token.text().to_string()));
                token.set_text("");
            });
        }
        {
            let input = sender.input_sender().clone();
            button_row(&anilist_group, "Log out", move || {
                input.emit(Msg::LogoutAniList)
            });
        }
        content.append(&anilist_group);

        // Sync und Player
        let sync_group = adw::PreferencesGroup::builder().title("Sync").build();
        let auto_sync = adw::SwitchRow::builder()
            .title("Update AniList automatically while watching")
            .build();
        let threshold = adw::SpinRow::with_range(50.0, 100.0, 1.0);
        threshold.set_title("Count as watched at (%)");
        sync_group.add(&auto_sync);
        sync_group.add(&threshold);
        content.append(&sync_group);

        let player_group = adw::PreferencesGroup::builder().title("Player").build();
        let mpv_path = adw::EntryRow::builder().title("mpv binary").build();
        let hwdec = adw::EntryRow::builder()
            .title("Hardware decoding (--hwdec, empty = mpv.conf)")
            .build();
        let autoplay = adw::SwitchRow::builder()
            .title("Play next episode automatically")
            .build();
        player_group.add(&mpv_path);
        player_group.add(&hwdec);
        player_group.add(&autoplay);
        content.append(&player_group);

        let save = util::suggested_button("Save settings");
        let input = sender.input_sender().clone();
        save.connect_clicked(move |_| input.emit(Msg::SaveSettings));
        content.append(&save);

        let root = adw::ToolbarView::new();
        root.add_top_bar(&adw::HeaderBar::new());
        root.set_content(Some(&page));
        Self {
            root,
            jellyfin_status,
            url,
            anilist_status,
            client_id,
            redirect,
            auto_sync,
            threshold,
            mpv_path,
            hwdec,
            autoplay,
        }
    }

    pub fn load(&self, config: &Config) {
        self.url
            .set_text(config.jellyfin.url.as_deref().unwrap_or(""));
        self.client_id
            .set_text(config.anilist.client_id.as_deref().unwrap_or(""));
        self.redirect
            .set_subtitle(&oauth::loopback_redirect(config.anilist.loopback_port));
        self.auto_sync.set_active(config.sync.auto_sync);
        self.threshold
            .set_value(config.sync.watched_threshold * 100.0);
        self.mpv_path.set_text(&config.player.mpv_path);
        self.hwdec.set_text(&config.player.hwdec);
        self.autoplay.set_active(config.player.autoplay_next);
    }

    /// Überträgt die Eingaben in die Konfiguration.
    pub fn apply(&self, config: &mut Config) {
        config.sync.auto_sync = self.auto_sync.is_active();
        config.sync.watched_threshold = self.threshold.value() / 100.0;
        config.player.mpv_path = self.mpv_path.text().trim().to_owned();
        config.player.hwdec = self.hwdec.text().trim().to_owned();
        config.player.autoplay_next = self.autoplay.is_active();
        let client_id = self.client_id.text().trim().to_owned();
        config.anilist.client_id = (!client_id.is_empty()).then_some(client_id);
    }

    pub fn set_auth(&self, auth: &AuthStatus) {
        self.jellyfin_status
            .set_subtitle(&gtk::glib::markup_escape_text(&jellyfin_status_text(auth)));
        self.anilist_status
            .set_subtitle(&gtk::glib::markup_escape_text(&anilist_status_text(auth)));
    }
}
