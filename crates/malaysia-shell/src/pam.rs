// Adapted from Otto's `components/otto-auth-ui/src/pam.rs` (MIT, Copyright (c) 2024 Riccardo Canalicchio); see NOTICE.
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::mpsc::{Receiver, Sender};

use zeroize::{Zeroize, Zeroizing};

use crate::localize::tr;

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
    Prompt(String),
    Info(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Authenticated,
    Denied(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Said(Message),
    Ended(Outcome),
}

const SERVICE: &str = "malaysia-lock";
const FALLBACK_SERVICE: &str = "login";

pub struct Attempt {
    answers: Sender<Zeroizing<String>>,
}

impl Attempt {
    pub fn start(events: async_channel::Sender<Event>, user: String) -> Self {
        let (answers, answer_rx) = std::sync::mpsc::channel();

        std::thread::Builder::new()
            .name("malaysia-pam".to_string())
            .spawn(move || {
                let outcome = converse(&user, &events, &answer_rx);
                let _ = events.try_send(Event::Ended(outcome));
            })
            .expect("could not start the PAM thread");

        Self { answers }
    }

    pub fn answer(&self, text: Zeroizing<String>) {
        let _ = self.answers.send(text);
    }
}

fn resolve_service(installed: impl Fn(&str) -> bool) -> CString {
    if installed(SERVICE) {
        return CString::new(SERVICE).expect("no interior nul");
    }
    if installed(FALLBACK_SERVICE) {
        eprintln!(
            "lock: /etc/pam.d/{SERVICE} is not installed; falling back to {FALLBACK_SERVICE}"
        );
        return CString::new(FALLBACK_SERVICE).expect("no interior nul");
    }
    eprintln!(
        "lock: no PAM service found ({SERVICE} or {FALLBACK_SERVICE}); authentication will fail"
    );
    CString::new(SERVICE).expect("no interior nul")
}

fn converse(
    user: &str,
    events: &async_channel::Sender<Event>,
    answers: &Receiver<Zeroizing<String>>,
) -> Outcome {
    let Ok(user_c) = CString::new(user) else {
        return Outcome::Denied(tr!("lock-invalid-user").to_string());
    };
    let service = resolve_service(|name| std::path::Path::new("/etc/pam.d").join(name).is_file());

    let mut state = ConversationState { events, answers };
    let conv = PamConv {
        conv: Some(conversation),
        appdata_ptr: &mut state as *mut ConversationState as *mut c_void,
    };

    let mut pamh: *mut PamHandle = std::ptr::null_mut();
    // SAFETY: `service` and `user_c` are NUL-terminated, and `conv` and `state` outlive `pamh`, which `pam_end` releases below.
    let status = unsafe { pam_start(service.as_ptr(), user_c.as_ptr(), &conv, &mut pamh) };
    if status != PAM_SUCCESS || pamh.is_null() {
        return Outcome::Denied(tr!("lock-pam-unavailable", status = status));
    }

    let (outcome, last_status) = authenticate(pamh);
    // SAFETY: `pamh` comes from a successful `pam_start` and is not used after this call.
    unsafe { pam_end(pamh, last_status) };
    outcome
}

fn authenticate(pamh: *mut PamHandle) -> (Outcome, c_int) {
    // SAFETY: `pamh` is a live handle from `pam_start`.
    let status = unsafe { pam_authenticate(pamh, 0) };
    if status != PAM_SUCCESS {
        return (Outcome::Denied(strerror(pamh, status)), status);
    }
    // SAFETY: `pamh` is a live handle from `pam_start`.
    let status = unsafe { pam_acct_mgmt(pamh, 0) };
    if status != PAM_SUCCESS {
        return (Outcome::Denied(strerror(pamh, status)), status);
    }
    // SAFETY: `pamh` is a live handle from `pam_start`.
    let status = unsafe { pam_setcred(pamh, PAM_REFRESH_CRED) };
    if status != PAM_SUCCESS {
        eprintln!(
            "lock: could not refresh credentials: {}",
            strerror(pamh, status)
        );
    }
    (Outcome::Authenticated, status)
}

fn strerror(pamh: *mut PamHandle, status: c_int) -> String {
    // SAFETY: `pamh` is a live handle from `pam_start`.
    let message = unsafe { pam_strerror(pamh, status) };
    if message.is_null() {
        return tr!("lock-authentication-failed", status = status);
    }
    // SAFETY: `pam_strerror` returns a static NUL-terminated string, checked non-null above.
    unsafe { CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}

struct ConversationState<'a> {
    events: &'a async_channel::Sender<Event>,
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
                if state
                    .events
                    .try_send(Event::Said(Message::Prompt(text)))
                    .is_err()
                {
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
                let _ = state.events.try_send(Event::Said(message));
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
    // SAFETY: the entry is process-wide static storage; the lock calls this before the PAM thread exists.
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
        tr!("lock-password").to_string()
    } else {
        label
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::{CString, c_void};
    use std::sync::mpsc;

    use zeroize::Zeroizing;

    use crate::localize::tr;

    use super::{
        ConversationState, PAM_CONV_ERR, PAM_PROMPT_ECHO_OFF, PamMessage, PamResponse,
        conversation, prompt_label, resolve_service,
    };

    #[test]
    fn prompt_labels_lose_terminal_punctuation() {
        assert_eq!(prompt_label("Password: "), "Password");
        assert_eq!(prompt_label("Contraseña:"), "Contraseña");
        assert_eq!(prompt_label("  "), tr!("lock-password"));
    }

    #[test]
    fn a_missing_malaysia_lock_falls_back_to_login() {
        let only = |present: &'static [&'static str]| move |name: &str| present.contains(&name);
        let pick = |present| resolve_service(only(present));
        assert_eq!(
            pick(&["malaysia-lock", "login"]).to_str(),
            Ok("malaysia-lock")
        );
        assert_eq!(pick(&["login"]).to_str(), Ok("login"));
        assert_eq!(pick(&[]).to_str(), Ok("malaysia-lock"));
    }

    #[test]
    fn a_failed_conversation_leaves_resp_null() {
        let (events, _received) = async_channel::unbounded();
        let (answers, answer_rx) = mpsc::channel();
        answers.send(Zeroizing::new("secret".to_string())).unwrap();
        let mut state = ConversationState {
            events: &events,
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
