//! A minimal password field for the lock screen.
//!
//! This is deliberately not a styled widget: it masks the input, shows
//! "verifying" while PAM is thinking, and shows the error after a refusal.
//! The compositor never sees the password; the secret is handed to the lock's
//! PAM thread and zeroized when that side drops it.

use yuki_design::{
    LOCK, LOCK_FIELD_HEIGHT, LOCK_FIELD_LINE, LOCK_FIELD_MARGIN, LOCK_FIELD_MIN_WIDTH,
    LOCK_FIELD_WIDTH, LOCK_LABEL_FONT_SIZE, LOCK_LABEL_OFFSET, LOCK_MASK_FONT_SIZE,
    LOCK_MASK_OFFSET, LOCK_STATUS_FONT_SIZE, LOCK_STATUS_OFFSET, LOCK_TITLE_SCALE,
    LOCK_TITLE_Y_DIVISOR, Rgba,
};
use zeroize::{Zeroize, Zeroizing};

use crate::text::Text;

/// What the field is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Waiting for input.
    Idle,
    /// An attempt is in flight; input is ignored.
    Verifying,
    /// The last attempt failed. The message is PAM's own wording.
    Error(String),
}

/// The secret input and its state.
pub struct PasswordField {
    value: Zeroizing<String>,
    state: State,
}

impl Default for PasswordField {
    fn default() -> Self {
        Self {
            value: Zeroizing::new(String::new()),
            state: State::Idle,
        }
    }
}

impl PasswordField {
    /// Adds one printable character. Typing after an error starts a new entry.
    pub fn insert(&mut self, ch: char) {
        if self.state == State::Verifying || ch.is_control() {
            return;
        }
        if matches!(self.state, State::Error(_)) {
            self.state = State::Idle;
        }
        self.value.push(ch);
    }

    /// Removes the last character.
    pub fn backspace(&mut self) {
        if self.state == State::Verifying {
            return;
        }
        if matches!(self.state, State::Error(_)) {
            self.state = State::Idle;
        }
        self.value.pop();
    }

    /// Takes the secret and enters `Verifying`. `None` for an empty field.
    pub fn take(&mut self) -> Option<Zeroizing<String>> {
        if self.state == State::Verifying || self.value.is_empty() {
            return None;
        }
        self.state = State::Verifying;
        Some(std::mem::replace(
            &mut self.value,
            Zeroizing::new(String::new()),
        ))
    }

    /// Shows PAM's refusal and wipes the entered secret.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.value.zeroize();
        self.value.clear();
        self.state = State::Error(message.into());
    }

    /// Back to an empty, idle field.
    pub fn reset(&mut self) {
        self.value.zeroize();
        self.value.clear();
        self.state = State::Idle;
    }

    /// The field state.
    pub fn state(&self) -> &State {
        &self.state
    }

    /// One bullet per typed character.
    pub fn masked(&self) -> String {
        "•".repeat(self.value.chars().count())
    }
}

/// The lock screen's password UI: title, label, masked field and status.
pub struct PasswordPrompt {
    field: PasswordField,
    text: Text,
    label: String,
    font_size: f32,
}

impl PasswordPrompt {
    /// Creates a prompt with the given font size.
    pub fn new(font_size: f32) -> Self {
        Self {
            field: PasswordField::default(),
            text: Text::new(font_size),
            label: "Contraseña".to_string(),
            font_size,
        }
    }

    /// Sets the label PAM asked for (terminal punctuation removed).
    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = label.into();
    }

    /// Adds a typed character.
    pub fn insert(&mut self, ch: char) {
        self.field.insert(ch);
    }

    /// Removes the last character.
    pub fn backspace(&mut self) {
        self.field.backspace();
    }

    /// Takes the secret, if any, and enters `Verifying`.
    pub fn take(&mut self) -> Option<Zeroizing<String>> {
        self.field.take()
    }

    /// Shows a refusal.
    pub fn set_error(&mut self, message: impl Into<String>) {
        self.field.set_error(message);
    }

    /// Clears the field and the status.
    pub fn reset(&mut self) {
        self.field.reset();
    }

    /// Draws the prompt into a top-down RGBA8 frame.
    pub fn draw(&mut self, frame: &mut [u8], stride: usize, w: u32, h: u32) {
        let (w, h) = (w as i32, h as i32);
        if w <= 0 || h <= 0 {
            return;
        }
        fill_rect(frame, stride, (0, 0, w, h), LOCK.bg_surface);

        self.text
            .set_text("Bloqueado", self.font_size * LOCK_TITLE_SCALE, None);
        let (tw, _) = self.text.measure();
        self.text.draw(
            frame,
            stride,
            w,
            h,
            ((w - tw.round() as i32) / 2, h / LOCK_TITLE_Y_DIVISOR),
            LOCK.ink,
        );

        let field_w = LOCK_FIELD_WIDTH.min((w - LOCK_FIELD_MARGIN).max(LOCK_FIELD_MIN_WIDTH));
        let field_h = LOCK_FIELD_HEIGHT;
        let field_x = (w - field_w) / 2;
        let field_y = h / 2;
        fill_rect(
            frame,
            stride,
            (field_x, field_y, field_w, field_h),
            LOCK.bg_surface,
        );
        fill_rect(
            frame,
            stride,
            (
                field_x,
                field_y + field_h - LOCK_FIELD_LINE,
                field_w,
                LOCK_FIELD_LINE,
            ),
            LOCK.accent,
        );

        self.text.set_text(&self.label, LOCK_LABEL_FONT_SIZE, None);
        let (lw, _) = self.text.measure();
        self.text.draw(
            frame,
            stride,
            w,
            h,
            ((w - lw.round() as i32) / 2, field_y - LOCK_LABEL_OFFSET),
            LOCK.ink_muted,
        );

        let masked = self.field.masked();
        if !masked.is_empty() {
            self.text.set_text(&masked, LOCK_MASK_FONT_SIZE, None);
            let (mw, _) = self.text.measure();
            self.text.draw(
                frame,
                stride,
                w,
                h,
                ((w - mw.round() as i32) / 2, field_y + LOCK_MASK_OFFSET),
                LOCK.ink,
            );
        }

        let (status, color) = match self.field.state() {
            State::Idle => (String::new(), LOCK.ink_muted),
            State::Verifying => ("Verificando…".to_string(), LOCK.ink_muted),
            State::Error(message) => (message.clone(), LOCK.danger),
        };
        if !status.is_empty() {
            self.text.set_text(
                &status,
                LOCK_STATUS_FONT_SIZE,
                Some((w - LOCK_FIELD_MARGIN) as f32),
            );
            let (sw, _) = self.text.measure();
            self.text.draw(
                frame,
                stride,
                w,
                h,
                (
                    (w - sw.round() as i32) / 2,
                    field_y + field_h + LOCK_STATUS_OFFSET,
                ),
                color,
            );
        }
    }
}

/// Fills a rectangle with an opaque colour.
fn fill_rect(frame: &mut [u8], stride: usize, rect: (i32, i32, i32, i32), color: Rgba) {
    let color = color.to_rgba8();
    let (x, y, rw, rh) = rect;
    let width = stride / 4;
    for row in y..(y + rh) {
        if row < 0 {
            continue;
        }
        for col in x..(x + rw) {
            if col < 0 || col as usize >= width {
                continue;
            }
            let pi = row as usize * stride + col as usize * 4;
            if pi + 4 <= frame.len() {
                frame[pi..pi + 4].copy_from_slice(&color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{PasswordField, State};

    #[test]
    fn the_field_masks_one_character_at_a_time() {
        let mut field = PasswordField::default();
        for ch in "hola".chars() {
            field.insert(ch);
        }
        assert_eq!(field.masked(), "••••");
        field.backspace();
        assert_eq!(field.masked(), "•••");
    }

    #[test]
    fn taking_the_secret_clears_the_field_and_enters_verifying() {
        let mut field = PasswordField::default();
        field.insert('x');
        let secret = field.take().expect("non-empty field");
        assert_eq!(secret.as_str(), "x");
        assert_eq!(field.state(), &State::Verifying);
        assert!(field.masked().is_empty());
        assert!(
            field.take().is_none(),
            "a second take while verifying is ignored"
        );
    }

    #[test]
    fn an_error_wipes_the_secret_and_typing_starts_again() {
        let mut field = PasswordField::default();
        field.insert('x');
        field.set_error("no");
        assert!(field.masked().is_empty());
        assert_eq!(field.state(), &State::Error("no".to_string()));
        field.insert('y');
        assert_eq!(field.state(), &State::Idle);
        assert_eq!(field.masked(), "•");
    }

    #[test]
    fn control_characters_are_ignored() {
        let mut field = PasswordField::default();
        field.insert('\n');
        field.insert('\t');
        assert!(field.masked().is_empty());
    }
}
