//! PAM authentication for the lock screen, off the render thread.
//!
//! Adapted from Otto's `components/otto-auth-ui/src/pam.rs` (MIT,
//! Copyright (c) 2024 Riccardo Canalicchio); see NOTICE. The design is the
//! same: a small FFI to libpam, the blocking conversation on its own thread,
//! prompts and answers over channels, and `Zeroizing` for the password.
//!
//! The compositor never links libpam. Only the lock process, which already
//! runs as the session's user, authenticates that user.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::{
    io::Write,
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender},
    },
};

use zeroize::Zeroizing;

const PAM_SUCCESS: c_int = 0;
const PAM_CONV_ERR: c_int = 19;
const PAM_BUF_ERR: c_int = 5;

const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;
const PAM_REFRESH_CRED: c_int = 0x0010;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

#[repr(C)]
struct PamConv {
    conv: Option<
        unsafe extern "C" fn(
            num_msg: c_int,
            msg: *mut *const PamMessage,
            resp: *mut *mut PamResponse,
            appdata_ptr: *mut c_void,
        ) -> c_int,
    >,
    appdata_ptr: *mut c_void,
}

enum PamHandle {}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(
        service_name: *const c_char,
        user: *const c_char,
        pam_conversation: *const PamConv,
        pamh: *mut *mut PamHandle,
    ) -> c_int;
    fn pam_authenticate(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_acct_mgmt(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_setcred(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut PamHandle, pam_status: c_int) -> c_int;
    fn pam_strerror(pamh: *mut PamHandle, errnum: c_int) -> *const c_char;
}

/// Something PAM said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    /// A question; `secret` is PAM's `ECHO_OFF` (the password field is masked
    /// either way, but the label still says what is being asked).
    Prompt { text: String, secret: bool },
    /// A hint, such as a fingerprint reader announcement.
    Info(String),
    /// A refusal that is not the end of the conversation.
    Error(String),
}

/// How an attempt ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The user and the account are good.
    Authenticated,
    /// PAM said no; the text is its own wording.
    Denied(String),
}

/// The buffer the PAM thread pushes events into; the lock polls it from a
/// Freya task. `Arc<Mutex<...>>` keeps the thread boundary explicit.
pub type EventBuffer = Arc<Mutex<Vec<Event>>>;

/// One event from the PAM thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// PAM said something.
    Said(Message),
    /// The attempt is over.
    Ended(Outcome),
}

/// Which PAM stack the lock uses.
#[derive(Debug, Clone, Copy)]
pub struct Service {
    /// Yuki's own stack, `/etc/pam.d/yuki-lock`.
    pub preferred: &'static str,
    /// Used when Yuki's file is not installed.
    pub fallback: &'static str,
}

impl Service {
    /// The lock's stack: `yuki-lock`, or `login` on a system that has not
    /// installed the NixOS PAM service yet.
    pub const LOCK: Service = Service {
        preferred: "yuki-lock",
        fallback: "login",
    };
}

/// One run of the stack, from `pam_start` to `pam_end`.
pub struct Attempt {
    answers: Sender<Zeroizing<String>>,
}

impl Attempt {
    /// Starts authenticating `user` against `service`, pushing events into
    /// `events` from a dedicated thread.
    pub fn start(
        events: EventBuffer,
        notify: Option<UnixStream>,
        service: Service,
        user: String,
    ) -> Self {
        let (answers, answer_rx) = std::sync::mpsc::channel();

        std::thread::Builder::new()
            .name("yuki-pam".to_string())
            .spawn(move || {
                let outcome = converse(&service, &user, &events, &notify, &answer_rx);
                if let Ok(mut buffer) = events.lock() {
                    buffer.push(Event::Ended(outcome));
                }
                if let Some(stream) = &notify {
                    let mut stream = stream;
                    let _ = stream.write_all(&[1]);
                }
            })
            .expect("could not start the PAM thread");

        Self { answers }
    }

    /// Answers the prompt PAM is waiting on. The `Zeroizing` value is wiped
    /// when the PAM thread drops its copies.
    pub fn answer(&self, text: Zeroizing<String>) {
        let _ = self.answers.send(text);
    }
}

/// Resolves the service name, logging the fallback once.
fn service_name(service: Service) -> CString {
    resolve_service(service, |name| {
        std::path::Path::new("/etc/pam.d").join(name).is_file()
    })
}

fn resolve_service(service: Service, installed: impl Fn(&str) -> bool) -> CString {
    if installed(service.preferred) {
        return CString::new(service.preferred).expect("no interior nul");
    }
    if installed(service.fallback) {
        eprintln!(
            "lock: /etc/pam.d/{} is not installed; falling back to {}",
            service.preferred, service.fallback
        );
        return CString::new(service.fallback).expect("no interior nul");
    }
    eprintln!(
        "lock: no PAM service found ({} or {}); authentication will fail",
        service.preferred, service.fallback
    );
    CString::new(service.preferred).expect("no interior nul")
}

/// The whole conversation, on the PAM thread.
fn converse(
    service: &Service,
    user: &str,
    events: &EventBuffer,
    notify: &Option<UnixStream>,
    answers: &Receiver<Zeroizing<String>>,
) -> Outcome {
    let Ok(user_c) = CString::new(user) else {
        return Outcome::Denied("invalid user name".to_string());
    };
    let service = service_name(*service);

    let mut state = ConversationState {
        events,
        notify,
        answers,
    };
    let conv = PamConv {
        conv: Some(conversation),
        appdata_ptr: &mut state as *mut ConversationState as *mut c_void,
    };

    let mut pamh: *mut PamHandle = std::ptr::null_mut();
    let status = unsafe { pam_start(service.as_ptr(), user_c.as_ptr(), &conv, &mut pamh) };
    if status != PAM_SUCCESS || pamh.is_null() {
        return Outcome::Denied(format!("PAM unavailable (status {status})"));
    }

    let outcome = authenticate(pamh);
    let end_status = if matches!(outcome, Outcome::Authenticated) {
        PAM_SUCCESS
    } else {
        PAM_CONV_ERR
    };
    unsafe { pam_end(pamh, end_status) };
    outcome
}

fn authenticate(pamh: *mut PamHandle) -> Outcome {
    let status = unsafe { pam_authenticate(pamh, 0) };
    if status != PAM_SUCCESS {
        return Outcome::Denied(strerror(pamh, status));
    }
    let status = unsafe { pam_acct_mgmt(pamh, 0) };
    if status != PAM_SUCCESS {
        return Outcome::Denied(strerror(pamh, status));
    }
    let status = unsafe { pam_setcred(pamh, PAM_REFRESH_CRED) };
    if status != PAM_SUCCESS {
        eprintln!(
            "lock: could not refresh credentials: {}",
            strerror(pamh, status)
        );
    }
    Outcome::Authenticated
}

fn strerror(pamh: *mut PamHandle, status: c_int) -> String {
    let message = unsafe { pam_strerror(pamh, status) };
    if message.is_null() {
        return format!("authentication failed (status {status})");
    }
    unsafe { CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}

struct ConversationState<'a> {
    events: &'a EventBuffer,
    notify: &'a Option<UnixStream>,
    answers: &'a Receiver<Zeroizing<String>>,
}

/// PAM's conversation callback, called on the PAM thread.
///
/// # Safety
///
/// `appdata_ptr` is the `ConversationState` passed to `pam_start`; the arrays
/// are PAM's, laid out as `pam_message`/`pam_response` on Linux-PAM.
unsafe extern "C" fn conversation(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    appdata_ptr: *mut c_void,
) -> c_int {
    if num_msg <= 0 || msg.is_null() || resp.is_null() || appdata_ptr.is_null() {
        return PAM_CONV_ERR;
    }
    let state = unsafe { &*(appdata_ptr as *const ConversationState) };
    let count = num_msg as usize;
    let responses =
        unsafe { libc::calloc(count, std::mem::size_of::<PamResponse>()) } as *mut PamResponse;
    if responses.is_null() {
        return PAM_BUF_ERR;
    }

    for i in 0..count {
        let message = unsafe { *msg.add(i) };
        if message.is_null() {
            unsafe { libc::free(responses as *mut c_void) };
            return PAM_CONV_ERR;
        }
        let text = if unsafe { (*message).msg }.is_null() {
            String::new()
        } else {
            unsafe { CStr::from_ptr((*message).msg) }
                .to_string_lossy()
                .into_owned()
        };
        let style = unsafe { (*message).msg_style };

        let answer = match style {
            PAM_PROMPT_ECHO_OFF | PAM_PROMPT_ECHO_ON => {
                let secret = style == PAM_PROMPT_ECHO_OFF;
                if let Ok(mut buffer) = state.events.lock() {
                    buffer.push(Event::Said(Message::Prompt { text, secret }));
                    if let Some(stream) = state.notify.as_ref() {
                        let mut stream = stream;
                        let _ = stream.write_all(&[1]);
                    }
                } else {
                    unsafe { libc::free(responses as *mut c_void) };
                    return PAM_CONV_ERR;
                }
                match state.answers.recv() {
                    Ok(answer) => Some(answer),
                    Err(_) => {
                        unsafe { libc::free(responses as *mut c_void) };
                        return PAM_CONV_ERR;
                    }
                }
            }
            PAM_ERROR_MSG | PAM_TEXT_INFO => {
                let message = if style == PAM_ERROR_MSG {
                    Message::Error(text)
                } else {
                    Message::Info(text)
                };
                if let Ok(mut buffer) = state.events.lock() {
                    buffer.push(Event::Said(message));
                    if let Some(stream) = state.notify.as_ref() {
                        let mut stream = stream;
                        let _ = stream.write_all(&[1]);
                    }
                }
                None
            }
            _ => {
                // An unknown style is answered with no response.
                None
            }
        };

        if let Some(answer) = answer {
            let mut terminated = Zeroizing::new(Vec::with_capacity(answer.len() + 1));
            terminated.extend_from_slice(answer.as_bytes());
            terminated.push(0);
            drop(answer);
            if terminated[..terminated.len() - 1].contains(&0) {
                unsafe { libc::free(responses as *mut c_void) };
                return PAM_CONV_ERR;
            }
            let copy = unsafe { libc::strdup(terminated.as_ptr() as *const c_char) };
            drop(terminated);
            if copy.is_null() {
                unsafe { libc::free(responses as *mut c_void) };
                return PAM_BUF_ERR;
            }
            unsafe {
                (*responses.add(i)).resp = copy;
            }
        }
    }

    unsafe {
        *resp = responses;
    }
    PAM_SUCCESS
}

/// The user this process runs as, taken from the uid rather than an
/// environment variable a caller could spoof.
pub fn current_user() -> Option<String> {
    let uid = unsafe { libc::getuid() };
    let pw = unsafe { libc::getpwuid(uid) };
    if pw.is_null() {
        return None;
    }
    Some(
        unsafe { CStr::from_ptr((*pw).pw_name) }
            .to_string_lossy()
            .into_owned(),
    )
}

/// PAM writes prompts for a terminal (`"Password: "`); the field label above
/// the box reads better without the punctuation.
pub fn prompt_label(text: &str) -> String {
    let label = text.trim().trim_end_matches(':').trim_end().to_string();
    if label.is_empty() {
        "Contraseña".to_string()
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use super::{Service, prompt_label, resolve_service};

    #[test]
    fn prompt_labels_lose_terminal_punctuation() {
        assert_eq!(prompt_label("Password: "), "Password");
        assert_eq!(prompt_label("Contraseña:"), "Contraseña");
        assert_eq!(prompt_label("  "), "Contraseña");
    }

    #[test]
    fn a_missing_yuki_lock_falls_back_to_login() {
        let only = |present: &'static [&'static str]| move |name: &str| present.contains(&name);
        let pick = |present| resolve_service(Service::LOCK, only(present));
        assert_eq!(pick(&["yuki-lock", "login"]).to_str(), Ok("yuki-lock"));
        assert_eq!(pick(&["login"]).to_str(), Ok("login"));
        assert_eq!(pick(&[]).to_str(), Ok("yuki-lock"));
    }
}
