//! PAM authentication for the lock screen.
//! Adapted from Otto's `components/otto-auth-ui/src/pam.rs` (MIT, Copyright (c) 2024 Riccardo Canalicchio); see NOTICE.
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::{
    io::Write,
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender},
    },
};

use zeroize::{Zeroize, Zeroizing};

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
// SAFETY: these signatures match <security/pam_appl.h>.
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Message {
    Prompt { text: String, secret: bool },
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Authenticated,
    Denied(String),
}

pub type EventBuffer = Arc<Mutex<Vec<Event>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Said(Message),
    Ended(Outcome),
}

#[derive(Debug, Clone, Copy)]
pub struct Service {
    pub preferred: &'static str,
    pub fallback: &'static str,
}

impl Service {
    pub const LOCK: Service = Service {
        preferred: "malaysia-lock",
        fallback: "login",
    };
}

pub struct Attempt {
    answers: Sender<Zeroizing<String>>,
}

impl Attempt {
    pub fn start(
        events: EventBuffer,
        notify: Option<UnixStream>,
        service: Service,
        user: String,
    ) -> Self {
        let (answers, answer_rx) = std::sync::mpsc::channel();

        std::thread::Builder::new()
            .name("malaysia-pam".to_string())
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

    pub fn answer(&self, text: Zeroizing<String>) {
        let _ = self.answers.send(text);
    }
}

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
    // SAFETY: `service` and `user_c` are NUL-terminated, and `conv` and `state` outlive `pamh`, which `pam_end` releases below.
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
    // SAFETY: `pamh` comes from a successful `pam_start` and is not used after this call.
    unsafe { pam_end(pamh, end_status) };
    outcome
}

fn authenticate(pamh: *mut PamHandle) -> Outcome {
    // SAFETY: `pamh` is a live handle from `pam_start`.
    let status = unsafe { pam_authenticate(pamh, 0) };
    if status != PAM_SUCCESS {
        return Outcome::Denied(strerror(pamh, status));
    }
    // SAFETY: `pamh` is a live handle from `pam_start`.
    let status = unsafe { pam_acct_mgmt(pamh, 0) };
    if status != PAM_SUCCESS {
        return Outcome::Denied(strerror(pamh, status));
    }
    // SAFETY: `pamh` is a live handle from `pam_start`.
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
    // SAFETY: `pamh` is a live handle from `pam_start`.
    let message = unsafe { pam_strerror(pamh, status) };
    if message.is_null() {
        return format!("authentication failed (status {status})");
    }
    // SAFETY: `pam_strerror` returns a static NUL-terminated string, checked non-null above.
    unsafe { CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}

struct ConversationState<'a> {
    events: &'a EventBuffer,
    notify: &'a Option<UnixStream>,
    answers: &'a Receiver<Zeroizing<String>>,
}

// SAFETY: callers pass the `calloc` array of `count` responses that `conversation` still owns.
unsafe fn discard_responses(responses: *mut PamResponse, count: usize) {
    for j in 0..count {
        // SAFETY: `j < count`, inside the array.
        let answer = unsafe { (*responses.add(j)).resp };
        if !answer.is_null() {
            // SAFETY: a non-null `resp` is a NUL-terminated `strdup` copy owned by this array.
            unsafe {
                std::slice::from_raw_parts_mut(answer as *mut u8, libc::strlen(answer)).zeroize();
                libc::free(answer as *mut c_void);
            }
        }
    }
    // SAFETY: `responses` comes from `calloc` and nothing references it after this call.
    unsafe { libc::free(responses as *mut c_void) };
}

// SAFETY: PAM calls this with `num_msg` messages and the `appdata_ptr` given to `pam_start`; null pointers are rejected below.
unsafe extern "C" fn conversation(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    appdata_ptr: *mut c_void,
) -> c_int {
    if num_msg <= 0 || msg.is_null() || resp.is_null() || appdata_ptr.is_null() {
        return PAM_CONV_ERR;
    }
    // SAFETY: `appdata_ptr` is the `ConversationState` that `converse` keeps alive until `pam_end`.
    let state = unsafe { &*(appdata_ptr as *const ConversationState) };
    let count = num_msg as usize;
    let responses =
        // SAFETY: a plain allocation; a null result is checked below.
        unsafe { libc::calloc(count, std::mem::size_of::<PamResponse>()) } as *mut PamResponse;
    if responses.is_null() {
        return PAM_BUF_ERR;
    }

    for i in 0..count {
        // SAFETY: `i < num_msg`, the length PAM gives for `msg`.
        let message = unsafe { *msg.add(i) };
        if message.is_null() {
            // SAFETY: `responses` is the array allocated above and is never handed to PAM.
            unsafe { discard_responses(responses, count) };
            return PAM_CONV_ERR;
        }
        // SAFETY: `message` was checked non-null above.
        let text = if unsafe { (*message).msg }.is_null() {
            String::new()
        } else {
            // SAFETY: PAM passes a NUL-terminated `msg`, checked non-null.
            unsafe { CStr::from_ptr((*message).msg) }
                .to_string_lossy()
                .into_owned()
        };
        // SAFETY: `message` was checked non-null above.
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
                    // SAFETY: `responses` is the array allocated above and is never handed to PAM.
                    unsafe { discard_responses(responses, count) };
                    return PAM_CONV_ERR;
                }
                match state.answers.recv() {
                    Ok(answer) => Some(answer),
                    Err(_) => {
                        // SAFETY: `responses` is the array allocated above and is never handed to PAM.
                        unsafe { discard_responses(responses, count) };
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
            _ => None,
        };

        if let Some(answer) = answer {
            let mut terminated = Zeroizing::new(Vec::with_capacity(answer.len() + 1));
            terminated.extend_from_slice(answer.as_bytes());
            terminated.push(0);
            drop(answer);
            if terminated[..terminated.len() - 1].contains(&0) {
                // SAFETY: `responses` is the array allocated above and is never handed to PAM.
                unsafe { discard_responses(responses, count) };
                return PAM_CONV_ERR;
            }
            // SAFETY: `terminated` ends in NUL and has no interior NUL (checked above).
            let copy = unsafe { libc::strdup(terminated.as_ptr() as *const c_char) };
            drop(terminated);
            if copy.is_null() {
                // SAFETY: `responses` is the array allocated above and is never handed to PAM.
                unsafe { discard_responses(responses, count) };
                return PAM_BUF_ERR;
            }
            // SAFETY: `i < count`, inside the array allocated above.
            unsafe {
                (*responses.add(i)).resp = copy;
            }
        }
    }

    // SAFETY: `resp` was checked non-null; PAM takes ownership of `responses`.
    unsafe {
        *resp = responses;
    }
    PAM_SUCCESS
}

pub fn current_user() -> Option<String> {
    // SAFETY: `getuid` has no preconditions.
    let uid = unsafe { libc::getuid() };
    // SAFETY: the static entry is copied below before any other `getpw*` call on this thread.
    let pw = unsafe { libc::getpwuid(uid) };
    if pw.is_null() {
        return None;
    }
    Some(
        // SAFETY: `pw` was checked non-null and `pw_name` is NUL-terminated.
        unsafe { CStr::from_ptr((*pw).pw_name) }
            .to_string_lossy()
            .into_owned(),
    )
}

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
    use std::ffi::{CString, c_void};
    use std::sync::{Arc, Mutex, mpsc};

    use zeroize::Zeroizing;

    use super::{
        ConversationState, EventBuffer, PAM_CONV_ERR, PAM_PROMPT_ECHO_OFF, PamMessage, PamResponse,
        Service, conversation, prompt_label, resolve_service,
    };

    #[test]
    fn prompt_labels_lose_terminal_punctuation() {
        assert_eq!(prompt_label("Password: "), "Password");
        assert_eq!(prompt_label("Contraseña:"), "Contraseña");
        assert_eq!(prompt_label("  "), "Contraseña");
    }

    #[test]
    fn a_missing_malaysia_lock_falls_back_to_login() {
        let only = |present: &'static [&'static str]| move |name: &str| present.contains(&name);
        let pick = |present| resolve_service(Service::LOCK, only(present));
        assert_eq!(
            pick(&["malaysia-lock", "login"]).to_str(),
            Ok("malaysia-lock")
        );
        assert_eq!(pick(&["login"]).to_str(), Ok("login"));
        assert_eq!(pick(&[]).to_str(), Ok("malaysia-lock"));
    }

    #[test]
    fn a_failed_conversation_leaves_resp_null() {
        let events: EventBuffer = Arc::new(Mutex::new(Vec::new()));
        let (answers, answer_rx) = mpsc::channel();
        answers.send(Zeroizing::new("secret".to_string())).unwrap();
        let notify = None;
        let mut state = ConversationState {
            events: &events,
            notify: &notify,
            answers: &answer_rx,
        };

        let text = CString::new("Password: ").unwrap();
        let prompt = PamMessage {
            msg_style: PAM_PROMPT_ECHO_OFF,
            msg: text.as_ptr(),
        };
        let mut messages = [&prompt as *const PamMessage, std::ptr::null()];
        let mut responses: *mut PamResponse = std::ptr::null_mut();

        // SAFETY: `messages` holds the two pointers `num_msg` says, and `state` outlives the call.
        let status = unsafe {
            conversation(
                2,
                messages.as_mut_ptr(),
                &mut responses,
                &mut state as *mut ConversationState as *mut c_void,
            )
        };

        assert_eq!(status, PAM_CONV_ERR);
        assert!(responses.is_null());
    }
}
