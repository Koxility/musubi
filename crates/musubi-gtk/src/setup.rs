//! Geführte Einrichtung: Jellyfin, AniList und Sync in drei Schritten.

use std::cell::{Cell, RefCell};

use musubi_core::oauth;
use musubi_core::{AuthStatus, Config};
use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};
use secrecy::SecretString;

use crate::app::{App, Msg};
use crate::settings::{anilist_status_text, button_row, jellyfin_status_text};
use crate::util::{self, MarginExt};

const STEP_TITLES: [&str; 3] = ["Jellyfin", "AniList", "Sync"];
const JELLYFIN: usize = 0;
const ANILIST: usize = 1;
const SYNC: usize = 2;
const DEVELOPER_URL: &str = "https://anilist.co/settings/developer";

pub struct SetupDialog {
    dialog: adw::Dialog,
    title: adw::WindowTitle,
    stack: gtk::Stack,
    back: gtk::Button,
    next: gtk::Button,
    message: gtk::Label,
    jellyfin_status: [adw::ActionRow; 2],
    anilist_status: [adw::ActionRow; 2],
    auto_sync: adw::SwitchRow,
    step: Cell<usize>,
    auth: RefCell<AuthStatus>,
}

fn show_message(label: &gtk::Label, text: &str, error: bool) {
    label.set_text(text);
    label.set_visible(!text.is_empty());
    if error {
        label.add_css_class("error");
    } else {
        label.remove_css_class("error");
    }
}

fn status_row(title: &str) -> adw::ActionRow {
    adw::ActionRow::builder().title(title).build()
}

fn set_status(row: &adw::ActionRow, text: &str) {
    row.set_subtitle(&gtk::glib::markup_escape_text(text));
}

impl SetupDialog {
    pub fn open(
        config: &Config,
        auth: &AuthStatus,
        sender: &ComponentSender<App>,
        parent: &adw::ApplicationWindow,
    ) -> Self {
        let message = gtk::Label::builder()
            .wrap(true)
            .xalign(0.0)
            .visible(false)
            .build();
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        stack.set_vexpand(true);

        // Jellyfin
        let (page, content) = util::pref_page();
        let group = adw::PreferencesGroup::builder()
            .title("Connect Jellyfin")
            .description("Enter the server address with its port, for example 192.168.1.10:8096.")
            .build();
        let jellyfin_status_row = status_row("Status");
        let url = adw::EntryRow::builder().title("Server URL").build();
        url.set_text(config.jellyfin.url.as_deref().unwrap_or(""));
        let user = adw::EntryRow::builder().title("Username").build();
        user.set_text(auth.jellyfin_user.as_deref().unwrap_or(""));
        let password = adw::PasswordEntryRow::builder()
            .title("Password (never stored)")
            .build();
        let api_key = adw::PasswordEntryRow::builder().title("or API key").build();
        group.add(&jellyfin_status_row);
        group.add(&url);
        group.add(&user);
        group.add(&password);
        group.add(&api_key);
        let login = {
            let input = sender.input_sender().clone();
            let (password, api_key, message) = (password.clone(), api_key.clone(), message.clone());
            button_row(&group, "Log in", move || {
                let server = url.text().trim().to_owned();
                let name = user.text().trim().to_owned();
                if server.is_empty() || name.is_empty() {
                    show_message(&message, "Server URL and username are required.", true);
                    return;
                }
                show_message(&message, "Logging in…", false);
                let key = api_key.text().to_string();
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
            })
        };
        login.add_css_class("suggested-action");
        password.connect_entry_activated(move |_| login.emit_clicked());
        content.append(&group);
        stack.add_named(&page, Some(STEP_TITLES[JELLYFIN]));

        // AniList
        let (page, content) = util::pref_page();
        let group = adw::PreferencesGroup::builder()
            .title("Connect AniList")
            .description(
                "Musubi has no built-in AniList app. Create your own once, the client secret \
                 is not needed.",
            )
            .build();
        let anilist_status_row = status_row("Status");
        group.add(&anilist_status_row);
        {
            let message = message.clone();
            button_row(&group, "Open AniList developer page", move || {
                let message = message.clone();
                gtk::UriLauncher::new(DEVELOPER_URL).launch(
                    None::<&gtk::Window>,
                    None::<&gtk::gio::Cancellable>,
                    move |result| {
                        if result.is_err() {
                            show_message(
                                &message,
                                &format!("Could not open a browser, visit {DEVELOPER_URL}"),
                                true,
                            );
                        }
                    },
                );
            });
        }
        let redirect = adw::ActionRow::builder()
            .title("Redirect URL for your AniList app")
            .subtitle(oauth::loopback_redirect(config.anilist.loopback_port))
            .subtitle_selectable(true)
            .build();
        group.add(&redirect);
        let client_id = adw::EntryRow::builder().title("Client ID").build();
        client_id.set_text(config.anilist.client_id.as_deref().unwrap_or(""));
        group.add(&client_id);
        let login = {
            let input = sender.input_sender().clone();
            let message = message.clone();
            button_row(&group, "Log in with browser", move || {
                let id = client_id.text().trim().to_owned();
                if id.is_empty() {
                    show_message(&message, "Enter the client ID of your AniList app.", true);
                    return;
                }
                show_message(&message, "Waiting for the browser…", false);
                input.emit(Msg::LoginAniListBrowser(id));
            })
        };
        login.add_css_class("suggested-action");
        let token = adw::PasswordEntryRow::builder()
            .title("Token or redirect URL (without browser login)")
            .build();
        group.add(&token);
        {
            let input = sender.input_sender().clone();
            let message = message.clone();
            button_row(&group, "Use pasted token", move || {
                show_message(&message, "Checking the token…", false);
                input.emit(Msg::LoginAniListToken(token.text().to_string()));
                token.set_text("");
            });
        }
        content.append(&group);
        stack.add_named(&page, Some(STEP_TITLES[ANILIST]));

        // Sync
        let (page, content) = util::pref_page();
        let group = adw::PreferencesGroup::builder()
            .title("Almost done")
            .description("You can preview and apply changes on the Sync page at any time.")
            .build();
        let jellyfin_summary = status_row("Jellyfin");
        let anilist_summary = status_row("AniList");
        let auto_sync = adw::SwitchRow::builder()
            .title("Update AniList automatically while watching")
            .subtitle("When off, AniList only changes when you apply a sync.")
            .active(config.sync.auto_sync)
            .build();
        group.add(&jellyfin_summary);
        group.add(&anilist_summary);
        group.add(&auto_sync);
        content.append(&group);
        stack.add_named(&page, Some(STEP_TITLES[SYNC]));

        // Rahmen
        let title = adw::WindowTitle::new("Set up Musubi", "");
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        let back = gtk::Button::with_label("Back");
        let next = gtk::Button::with_label("Continue");
        let actions = gtk::ActionBar::new();
        actions.pack_start(&back);
        actions.pack_end(&next);
        {
            let input = sender.input_sender().clone();
            back.connect_clicked(move |_| input.emit(Msg::SetupBack));
        }
        {
            let input = sender.input_sender().clone();
            next.connect_clicked(move |_| input.emit(Msg::SetupNext));
        }

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        message.set_margin_all(12);
        body.append(&stack);
        body.append(&message);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.add_bottom_bar(&actions);
        toolbar.set_content(Some(&body));

        let dialog = adw::Dialog::builder()
            .title("Set up Musubi")
            .content_width(600)
            .content_height(640)
            .child(&toolbar)
            .build();
        {
            let input = sender.input_sender().clone();
            dialog.connect_closed(move |_| input.emit(Msg::SetupClosed));
        }

        let first = if auth.jellyfin_user.is_some() && !auth.anilist_logged_in {
            ANILIST
        } else {
            JELLYFIN
        };
        let setup = Self {
            dialog,
            title,
            stack,
            back,
            next,
            message,
            jellyfin_status: [jellyfin_status_row, jellyfin_summary],
            anilist_status: [anilist_status_row, anilist_summary],
            auto_sync,
            step: Cell::new(first),
            auth: RefCell::new(AuthStatus::default()),
        };
        setup.set_auth(auth);
        setup
            .stack
            .set_transition_type(gtk::StackTransitionType::None);
        setup.show_step(first);
        setup
            .stack
            .set_transition_type(gtk::StackTransitionType::SlideLeftRight);
        setup.dialog.present(Some(parent));
        setup
    }

    fn set_auth(&self, auth: &AuthStatus) {
        let jellyfin = jellyfin_status_text(auth);
        let anilist = anilist_status_text(auth);
        self.jellyfin_status
            .iter()
            .for_each(|row| set_status(row, &jellyfin));
        self.anilist_status
            .iter()
            .for_each(|row| set_status(row, &anilist));
        *self.auth.borrow_mut() = auth.clone();
    }

    fn show_step(&self, step: usize) {
        self.step.set(step);
        self.stack.set_visible_child_name(STEP_TITLES[step]);
        self.title.set_subtitle(&format!(
            "Step {} of {}: {}",
            step + 1,
            STEP_TITLES.len(),
            STEP_TITLES[step]
        ));
        self.back.set_sensitive(step > JELLYFIN);
        self.update_next_button();
    }

    fn update_next_button(&self) {
        let auth = self.auth.borrow();
        let (label, primary) = match self.step.get() {
            JELLYFIN if auth.jellyfin_user.is_none() => ("Skip", false),
            ANILIST if !auth.anilist_logged_in => ("Skip", false),
            SYNC => ("Finish", true),
            _ => ("Continue", true),
        };
        self.next.set_label(label);
        if primary {
            self.next.add_css_class("suggested-action");
        } else {
            self.next.remove_css_class("suggested-action");
        }
    }

    /// Geht einen Schritt weiter. `true` bedeutet, dass der letzte Schritt bestätigt wurde.
    pub fn advance(&self) -> bool {
        let step = self.step.get();
        if step == SYNC {
            return true;
        }
        show_message(&self.message, "", false);
        self.show_step(step + 1);
        false
    }

    pub fn go_back(&self) {
        let step = self.step.get();
        if step > JELLYFIN {
            show_message(&self.message, "", false);
            self.show_step(step - 1);
        }
    }

    /// Zeigt den neuen Status und wechselt nach einer gelungenen Anmeldung zum nächsten Schritt.
    pub fn logged_in(&self, auth: &AuthStatus) {
        self.set_auth(auth);
        let finished_step = match self.step.get() {
            JELLYFIN => auth.jellyfin_user.is_some(),
            ANILIST => auth.anilist_logged_in,
            _ => false,
        };
        if finished_step {
            self.advance();
        } else {
            self.update_next_button();
        }
    }

    pub fn show_error(&self, text: &str) {
        show_message(&self.message, text, true);
    }

    pub fn auto_sync(&self) -> bool {
        self.auto_sync.is_active()
    }

    pub fn close(&self) {
        self.dialog.close();
    }
}
