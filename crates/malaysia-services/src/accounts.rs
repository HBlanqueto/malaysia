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

impl Service for Accounts {
    const NAME: &'static str = ACCOUNTS;
    type Snapshot = Option<String>;
    type Command = Infallible;

    fn read(&mut self, bus: &Connection) -> zbus::Result<Option<String>> {
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
        Ok(property::<String>(&properties, "IconFile").filter(|path| !path.is_empty()))
    }

    fn run(&mut self, _bus: &Connection, command: Infallible) -> zbus::Result<()> {
        match command {}
    }
}
