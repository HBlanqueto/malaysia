use std::convert::Infallible;
use std::os::unix::fs::MetadataExt;

use zbus::blocking::Connection;
use zbus::zvariant::OwnedObjectPath;

use crate::bus::{Service, get_all, property};

const ACCOUNTS: &str = "org.freedesktop.Accounts";
const ACCOUNTS_PATH: &str = "/org/freedesktop/Accounts";
const USER: &str = "org.freedesktop.Accounts.User";

#[derive(Default)]
pub struct Accounts;

fn passwd_entry() -> Option<nix::unistd::User> {
    nix::unistd::User::from_uid(nix::unistd::Uid::current())
        .ok()
        .flatten()
}

pub fn login_name() -> Option<String> {
    passwd_entry().map(|entry| entry.name)
}

pub fn full_name() -> Option<String> {
    let gecos = passwd_entry()?.gecos;
    let name = gecos.to_string_lossy();
    let name = name.split(',').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
}

#[derive(Clone, PartialEq)]
pub struct Account {
    pub picture: Option<String>,
    pub password_hint: Option<String>,
}

impl Service for Accounts {
    const NAME: &'static str = ACCOUNTS;
    type Snapshot = Account;
    type Command = Infallible;

    fn read(&mut self, bus: &Connection) -> zbus::Result<Account> {
        let uid = std::fs::metadata("/proc/self")
            .map_err(|error| zbus::Error::Failure(error.to_string()))?
            .uid();
        let user: OwnedObjectPath = bus
            .call_method(
                Some(ACCOUNTS),
                ACCOUNTS_PATH,
                Some(ACCOUNTS),
                "FindUserById",
                &(i64::from(uid),),
            )?
            .body()
            .deserialize()?;
        let properties = get_all(bus, ACCOUNTS, user.as_ref(), USER)?;
        let text =
            |name: &str| property::<String>(&properties, name).filter(|value| !value.is_empty());
        Ok(Account {
            picture: text("IconFile"),
            password_hint: text("PasswordHint"),
        })
    }

    fn run(&mut self, _bus: &Connection, command: Infallible) -> zbus::Result<()> {
        match command {}
    }
}
