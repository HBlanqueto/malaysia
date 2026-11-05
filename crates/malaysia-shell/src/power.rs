const LOGIND: &str = "org.freedesktop.login1";
const LOGIND_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    Suspend,
    Restart,
    ShutDown,
    LogOut,
}

impl PowerAction {
    pub fn from_arg(arg: &str) -> Option<Self> {
        match arg {
            "suspend" => Some(Self::Suspend),
            "restart" => Some(Self::Restart),
            "shutdown" => Some(Self::ShutDown),
            "logout" => Some(Self::LogOut),
            _ => None,
        }
    }

    pub fn arg(self) -> &'static str {
        match self {
            Self::Suspend => "suspend",
            Self::Restart => "restart",
            Self::ShutDown => "shutdown",
            Self::LogOut => "logout",
        }
    }

    fn call(self) -> (&'static str, Option<String>) {
        match self {
            Self::Suspend => ("Suspend", None),
            Self::Restart => ("Reboot", None),
            Self::ShutDown => ("PowerOff", None),
            Self::LogOut => ("TerminateSession", std::env::var("XDG_SESSION_ID").ok()),
        }
    }
}

fn dry_run() -> bool {
    std::env::var("MALAYSIA_POWER_DRY_RUN").is_ok_and(|value| value != "0")
}

pub fn run(action: PowerAction) {
    let (method, session) = action.call();
    if dry_run() {
        tracing::info!(
            method = %format!("{MANAGER}.{method}"),
            session = session.as_deref().unwrap_or("-"),
            "power: dry run, not calling logind"
        );
        return;
    }
    let result =
        zbus::blocking::Connection::system().and_then(|connection| match (action, session) {
            (PowerAction::LogOut, Some(session)) => connection.call_method(
                Some(LOGIND),
                LOGIND_PATH,
                Some(MANAGER),
                method,
                &(session,),
            ),
            (PowerAction::LogOut, None) => Err(zbus::Error::Failure(
                "XDG_SESSION_ID is not set; cannot end the session".to_string(),
            )),
            _ => connection.call_method(Some(LOGIND), LOGIND_PATH, Some(MANAGER), method, &(true,)),
        });
    match result {
        Ok(_) => tracing::info!(method, "power: logind call sent"),
        Err(error) => tracing::error!(%error, method, "power: logind call failed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_round_trip() {
        for action in [
            PowerAction::Suspend,
            PowerAction::Restart,
            PowerAction::ShutDown,
            PowerAction::LogOut,
        ] {
            assert_eq!(PowerAction::from_arg(action.arg()), Some(action));
        }
        assert_eq!(PowerAction::from_arg("halt"), None);
    }

    #[test]
    fn actions_map_to_logind_methods() {
        assert_eq!(PowerAction::Suspend.call().0, "Suspend");
        assert_eq!(PowerAction::Restart.call().0, "Reboot");
        assert_eq!(PowerAction::ShutDown.call().0, "PowerOff");
        assert_eq!(PowerAction::LogOut.call().0, "TerminateSession");
    }
}
