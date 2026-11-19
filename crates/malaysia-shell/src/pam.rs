use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::Path;
use std::sync::mpsc;
use std::{ptr, slice, thread};

use async_channel::Sender;
use zeroize::{Zeroize, Zeroizing};

use crate::localize::tr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Message {
    Prompt(String),
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Authenticated,
    Denied(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Event {
    Said(Message),
    Ended(Outcome),
}

const POLICY_DIR: &str = "/etc/pam.d";

const PAM_SUCCESS: c_int = 0;
const PAM_SYSTEM_ERR: c_int = 4;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_REFRESH_CRED: c_int = 0x0010;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

#[repr(C)]
struct RawMessage {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct RawReply {
    text: *mut c_char,
    retcode: c_int,
}

type Callback =
    unsafe extern "C" fn(c_int, *const *const RawMessage, *mut *mut RawReply, *mut c_void) -> c_int;

#[repr(C)]
struct RawConversation {
    callback: Callback,
    data: *mut c_void,
}

type Step = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conversation: *const RawConversation,
        handle: *mut *mut c_void,
    ) -> c_int;
    fn pam_end(handle: *mut c_void, status: c_int) -> c_int;
    fn pam_authenticate(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_acct_mgmt(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_setcred(handle: *mut c_void, flags: c_int) -> c_int;
    fn pam_strerror(handle: *mut c_void, status: c_int) -> *const c_char;
}

pub(crate) struct Attempt {
    typed: mpsc::Sender<Zeroizing<String>>,
}

impl Attempt {
    pub(crate) fn start(events: Sender<Event>, user: String) -> Attempt {
        let (typed, waiting) = mpsc::channel();
        let failure = events.clone();
        let worker = thread::Builder::new()
            .name("malaysia-pam".into())
            .spawn(move || {
                let dialog = Dialog {
                    events,
                    typed: waiting,
                };
                let outcome = run_transaction(&user, &dialog);
                let _ = dialog.events.send_blocking(Event::Ended(outcome));
            });
        if let Err(error) = worker {
            tracing::error!(%error, "the lock screen has no thread left for PAM");
            let _ = failure.try_send(Event::Ended(Outcome::Denied(tr!(
                "lock-pam-unavailable",
                status = PAM_SYSTEM_ERR
            ))));
        }
        Attempt { typed }
    }

    pub(crate) fn answer(&self, text: Zeroizing<String>) {
        let _ = self.typed.send(text);
    }
}

pub(crate) fn prompt_label(text: &str) -> String {
    let label =
        text.trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '.' | '?' | '!'));
    if label.is_empty() {
        tr!("lock-password").to_string()
    } else {
        label.to_string()
    }
}

struct Dialog {
    events: Sender<Event>,
    typed: mpsc::Receiver<Zeroizing<String>>,
}

impl Dialog {
    fn reply_to(&self, style: c_int, text: String) -> Result<*mut c_char, ()> {
        let (said, needs_answer) = match style {
            PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON => (Message::Prompt(text), true),
            PAM_TEXT_INFO => (Message::Info(text), false),
            PAM_ERROR_MSG => (Message::Error(text), false),
            _ => return Err(()),
        };
        self.events.send_blocking(Event::Said(said)).map_err(drop)?;
        if !needs_answer {
            return Ok(ptr::null_mut());
        }
        let answer = self.typed.recv().map_err(drop)?;
        pam_owned_copy(&answer).ok_or(())
    }
}

fn service_in(policies: &Path) -> Option<&'static CStr> {
    if policies.join("malaysia-lock").exists() {
        return Some(c"malaysia-lock");
    }
    tracing::warn!(dir = %policies.display(), "no malaysia-lock PAM service; falling back to login");
    if policies.join("login").exists() {
        return Some(c"login");
    }
    tracing::warn!(dir = %policies.display(), "no login PAM service either; cannot authenticate");
    None
}

fn run_transaction(user: &str, dialog: &Dialog) -> Outcome {
    let Ok(user) = CString::new(user) else {
        return Outcome::Denied(tr!("lock-invalid-user").to_string());
    };
    let Some(service) = service_in(Path::new(POLICY_DIR)) else {
        return Outcome::Denied(tr!("lock-pam-unavailable", status = PAM_SYSTEM_ERR));
    };
    let conversation = RawConversation {
        callback: on_messages,
        data: ptr::from_ref(dialog).cast_mut().cast(),
    };
    let mut handle = ptr::null_mut();
    let started = unsafe { pam_start(service.as_ptr(), user.as_ptr(), &conversation, &mut handle) };
    if started != PAM_SUCCESS {
        tracing::error!(status = started, "PAM refused to open a transaction");
        return Outcome::Denied(tr!("lock-pam-unavailable", status = started));
    }
    let verdict = verify(handle);
    let (Ok(last) | Err((last, _))) = verdict;
    unsafe { pam_end(handle, last) };
    match verdict {
        Ok(_) => Outcome::Authenticated,
        Err((_, reason)) => Outcome::Denied(reason),
    }
}

fn verify(handle: *mut c_void) -> Result<c_int, (c_int, String)> {
    let gates: [(&str, Step); 2] = [
        ("pam_authenticate", pam_authenticate),
        ("pam_acct_mgmt", pam_acct_mgmt),
    ];
    for (name, gate) in gates {
        let status = unsafe { gate(handle, 0) };
        if status != PAM_SUCCESS {
            return Err((status, log_refusal(handle, name, status)));
        }
    }
    let refreshed = unsafe { pam_setcred(handle, PAM_REFRESH_CRED) };
    if refreshed != PAM_SUCCESS {
        log_refusal(handle, "pam_setcred", refreshed);
    }
    Ok(refreshed)
}

fn log_refusal(handle: *mut c_void, step: &str, status: c_int) -> String {
    let text = unsafe { pam_strerror(handle, status) };
    let reason = if text.is_null() {
        tr!("lock-authentication-failed", status = status).to_string()
    } else {
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    };
    tracing::warn!(step, status, %reason, "PAM said no");
    reason
}

unsafe extern "C" fn on_messages(
    count: c_int,
    messages: *const *const RawMessage,
    replies: *mut *mut RawReply,
    data: *mut c_void,
) -> c_int {
    if replies.is_null() {
        return PAM_CONV_ERR;
    }
    unsafe { *replies = ptr::null_mut() };
    let Ok(count) = usize::try_from(count) else {
        return PAM_CONV_ERR;
    };
    if count == 0 || messages.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    let dialog = unsafe { &*data.cast::<Dialog>() };
    let filled = unsafe { libc::calloc(count, size_of::<RawReply>()) }.cast::<RawReply>();
    if filled.is_null() {
        return PAM_BUF_ERR;
    }
    for index in 0..count {
        let message = unsafe { &**messages.add(index) };
        let text = ptr::NonNull::new(message.text.cast_mut())
            .map(|text| {
                String::from_utf8_lossy(unsafe { CStr::from_ptr(text.as_ptr()) }.to_bytes())
                    .into_owned()
            })
            .unwrap_or_default();
        match dialog.reply_to(message.style, text) {
            Ok(answer) => unsafe { (*filled.add(index)).text = answer },
            Err(()) => {
                unsafe { wipe_replies(filled, count) };
                return PAM_CONV_ERR;
            }
        }
    }
    unsafe { *replies = filled };
    PAM_SUCCESS
}

fn pam_owned_copy(answer: &str) -> Option<*mut c_char> {
    if answer.contains('\0') {
        return None;
    }
    let duplicate = unsafe { libc::strndup(answer.as_ptr().cast(), answer.len()) };
    (!duplicate.is_null()).then_some(duplicate)
}

unsafe fn wipe_replies(filled: *mut RawReply, count: usize) {
    for index in 0..count {
        let text = unsafe { (*filled.add(index)).text };
        if text.is_null() {
            continue;
        }
        unsafe {
            slice::from_raw_parts_mut(text.cast::<u8>(), libc::strlen(text)).zeroize();
            libc::free(text.cast());
        }
    }
    unsafe { libc::free(filled.cast()) };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_labels_lose_terminal_punctuation() {
        assert_eq!(prompt_label("Password: "), "Password");
        assert_eq!(prompt_label("PIN de la tarjeta:"), "PIN de la tarjeta");
        assert_eq!(prompt_label("Token code?"), "Token code");
    }

    #[test]
    fn a_missing_malaysia_lock_falls_back_to_login() {
        let policies = std::env::temp_dir().join(format!("malaysia-pam-{}", std::process::id()));
        std::fs::create_dir_all(&policies).expect("create the policy dir");
        std::fs::write(policies.join("login"), "").expect("write the login policy");
        let chosen = service_in(&policies);
        std::fs::remove_dir_all(&policies).expect("remove the policy dir");
        assert_eq!(chosen, Some(c"login"));
    }

    #[test]
    fn a_failed_conversation_leaves_resp_null() {
        let (events, said) = async_channel::unbounded();
        let (keyboard, typed) = mpsc::channel();
        keyboard
            .send(Zeroizing::new("secret".to_string()))
            .expect("queue the first answer");
        drop(keyboard);
        let dialog = Dialog { events, typed };
        let password = RawMessage {
            style: PAM_PROMPT_ECHO_OFF,
            text: c"Password:".as_ptr(),
        };
        let code = RawMessage {
            style: PAM_PROMPT_ECHO_OFF,
            text: c"Code:".as_ptr(),
        };
        let messages = [&raw const password, &raw const code];
        let mut filled = ptr::NonNull::<RawReply>::dangling().as_ptr();
        let status = unsafe {
            on_messages(
                2,
                messages.as_ptr(),
                &mut filled,
                ptr::from_ref(&dialog).cast_mut().cast(),
            )
        };
        assert_eq!(status, PAM_CONV_ERR);
        assert!(filled.is_null());
        assert_eq!(said.len(), 2);
    }
}
