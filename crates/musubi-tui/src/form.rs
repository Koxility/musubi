//! Allgemeines Formular für Login, Eintragsbearbeitung, Filter und Zuordnung.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Secret,
    Choice(Vec<String>),
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: &'static str,
    pub label: String,
    pub kind: FieldKind,
    pub value: String,
}

impl Field {
    pub fn text(key: &'static str, label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key,
            label: label.into(),
            kind: FieldKind::Text,
            value: value.into(),
        }
    }

    pub fn secret(key: &'static str, label: impl Into<String>) -> Self {
        Self {
            key,
            label: label.into(),
            kind: FieldKind::Secret,
            value: String::new(),
        }
    }

    pub fn choice(
        key: &'static str,
        label: impl Into<String>,
        options: Vec<String>,
        value: impl Into<String>,
    ) -> Self {
        Self {
            key,
            label: label.into(),
            kind: FieldKind::Choice(options),
            value: value.into(),
        }
    }

    pub fn toggle(key: &'static str, label: impl Into<String>, value: bool) -> Self {
        Self {
            key,
            label: label.into(),
            kind: FieldKind::Toggle,
            value: if value { "yes" } else { "no" }.to_owned(),
        }
    }

    pub fn is_on(&self) -> bool {
        self.value == "yes"
    }

    fn cycle(&mut self, forward: bool) {
        match &self.kind {
            FieldKind::Choice(options) if !options.is_empty() => {
                let current = options.iter().position(|o| *o == self.value).unwrap_or(0);
                let next = if forward {
                    (current + 1) % options.len()
                } else {
                    (current + options.len() - 1) % options.len()
                };
                self.value = options[next].clone();
            }
            FieldKind::Toggle => {
                self.value = if self.is_on() { "no" } else { "yes" }.to_owned();
            }
            _ => {}
        }
    }

    /// Text für die Anzeige. Geheime Felder werden maskiert.
    pub fn display(&self) -> String {
        match self.kind {
            FieldKind::Secret => "•".repeat(self.value.chars().count()),
            FieldKind::Choice(_) => format!("‹ {} ›", self.value),
            FieldKind::Toggle => format!("[{}]", if self.is_on() { "x" } else { " " }),
            FieldKind::Text => self.value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormOutcome {
    Editing,
    Submit,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub error: Option<String>,
    pub hint: Option<String>,
}

impl Form {
    pub fn new(title: impl Into<String>, fields: Vec<Field>) -> Self {
        Self {
            title: title.into(),
            fields,
            focus: 0,
            error: None,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn value(&self, key: &str) -> &str {
        self.field(key).map(|f| f.value.as_str()).unwrap_or("")
    }

    pub fn field(&self, key: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.key == key)
    }

    pub fn is_on(&self, key: &str) -> bool {
        self.field(key).is_some_and(Field::is_on)
    }

    fn move_focus(&mut self, forward: bool) {
        let count = self.fields.len();
        if count > 0 {
            self.focus = if forward {
                (self.focus + 1) % count
            } else {
                (self.focus + count - 1) % count
            };
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> FormOutcome {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Esc => return FormOutcome::Cancel,
            KeyCode::Enter => return FormOutcome::Submit,
            KeyCode::Tab | KeyCode::Down => self.move_focus(true),
            KeyCode::BackTab | KeyCode::Up => self.move_focus(false),
            _ => {
                let Some(field) = self.fields.get_mut(self.focus) else {
                    return FormOutcome::Editing;
                };
                match (&field.kind, key.code) {
                    (
                        FieldKind::Choice(_) | FieldKind::Toggle,
                        KeyCode::Right | KeyCode::Char(' '),
                    ) => {
                        field.cycle(true);
                    }
                    (FieldKind::Choice(_) | FieldKind::Toggle, KeyCode::Left) => field.cycle(false),
                    (FieldKind::Text | FieldKind::Secret, KeyCode::Backspace) => {
                        field.value.pop();
                    }
                    (FieldKind::Text | FieldKind::Secret, KeyCode::Char('u')) if ctrl => {
                        field.value.clear();
                    }
                    (FieldKind::Text | FieldKind::Secret, KeyCode::Char(c)) if !ctrl => {
                        field.value.push(c);
                    }
                    _ => {}
                }
            }
        }
        FormOutcome::Editing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn form() -> Form {
        Form::new(
            "t",
            vec![
                Field::text("name", "Name", "ab"),
                Field::secret("pw", "Password"),
                Field::choice("c", "Choice", vec!["x".into(), "y".into()], "x"),
                Field::toggle("t", "Toggle", false),
            ],
        )
    }

    #[test]
    fn types_and_moves_between_fields() {
        let mut f = form();
        f.on_key(key(KeyCode::Char('c')));
        assert_eq!(f.value("name"), "abc");
        f.on_key(key(KeyCode::Backspace));
        assert_eq!(f.value("name"), "ab");
        f.on_key(key(KeyCode::Tab));
        f.on_key(key(KeyCode::Char('s')));
        assert_eq!(f.value("pw"), "s");
        assert_eq!(f.fields[1].display(), "•");
    }

    #[test]
    fn choices_and_toggles_cycle() {
        let mut f = form();
        f.focus = 2;
        f.on_key(key(KeyCode::Right));
        assert_eq!(f.value("c"), "y");
        f.on_key(key(KeyCode::Right));
        assert_eq!(f.value("c"), "x");
        f.on_key(key(KeyCode::Left));
        assert_eq!(f.value("c"), "y");
        f.focus = 3;
        f.on_key(key(KeyCode::Char(' ')));
        assert!(f.is_on("t"));
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let mut f = form();
        assert_eq!(f.on_key(key(KeyCode::Enter)), FormOutcome::Submit);
        assert_eq!(f.on_key(key(KeyCode::Esc)), FormOutcome::Cancel);
    }
}
