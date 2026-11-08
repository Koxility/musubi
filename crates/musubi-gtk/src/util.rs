//! Kleine Hilfen für Widgets und Hintergrundaufgaben.

use std::future::Future;

use relm4::adw::prelude::*;
use relm4::{ComponentSender, adw, gtk};

use crate::app::{App, Cmd};

/// Führt Arbeit im Hintergrund aus und liefert das Ergebnis an `update_cmd`.
pub fn task<T>(
    sender: &ComponentSender<App>,
    future: impl Future<Output = musubi_core::Result<T>> + Send + 'static,
    wrap: impl FnOnce(Result<T, String>) -> Cmd + Send + 'static,
) where
    T: Send + 'static,
{
    sender.oneshot_command(async move { wrap(future.await.map_err(|e| e.to_string())) });
}

/// Zeigt einen Dialog über dem aktiven Fenster.
pub fn present(dialog: &impl IsA<adw::Dialog>) {
    let parent = relm4::main_application().active_window();
    dialog.present(parent.as_ref());
}

/// Dialog mit Kopfleiste und scrollbarem Inhalt.
pub fn dialog(title: &str, width: i32, height: i32, child: &impl IsA<gtk::Widget>) -> adw::Dialog {
    let dialog = adw::Dialog::builder()
        .title(title)
        .content_width(width)
        .content_height(height)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(child)
        .build();
    toolbar.set_content(Some(&scroller));
    dialog.set_child(Some(&toolbar));
    dialog
}

pub fn string_list(items: &[String]) -> gtk::StringList {
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    gtk::StringList::new(&refs)
}

/// Text des gewählten Eintrags einer Auswahlliste.
pub fn dropdown_text(dropdown: &gtk::DropDown) -> Option<String> {
    dropdown
        .selected_item()
        .and_then(|item| item.downcast::<gtk::StringObject>().ok())
        .map(|item| item.string().to_string())
}

/// Scrollfläche, die ihren Inhalt mittig begrenzt wie die GNOME-Einstellungen.
pub fn pref_page() -> (gtk::Widget, gtk::Box) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_margin_all(18);
    let clamp = adw::Clamp::builder()
        .maximum_size(720)
        .child(&content)
        .build();
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    (scroller.upcast(), content)
}

pub trait MarginExt {
    fn set_margin_all(&self, margin: i32);
}

impl<W: IsA<gtk::Widget>> MarginExt for W {
    fn set_margin_all(&self, margin: i32) {
        self.set_margin_top(margin);
        self.set_margin_bottom(margin);
        self.set_margin_start(margin);
        self.set_margin_end(margin);
    }
}

pub fn suggested_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("suggested-action");
    button.add_css_class("pill");
    button.set_halign(gtk::Align::Center);
    button
}

pub fn clear_list(list: &gtk::ListBox) {
    list.remove_all();
}

pub fn boxed_list() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_valign(gtk::Align::Start);
    list
}

pub fn format_time(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let (h, m, s) = (total / 3600, (total % 3600) / 60, total % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}
