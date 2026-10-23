//! A minimal password field for the lock screen.
//!
//! This is deliberately not a styled widget: it masks the input, shows
//! "verifying" while PAM is thinking, and shows the error after a refusal.
//! The compositor never sees the password; the secret is handed to the lock's
//! PAM thread and zeroized when that side drops it.

use zeroize::{Zeroize, Zeroizing};

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
