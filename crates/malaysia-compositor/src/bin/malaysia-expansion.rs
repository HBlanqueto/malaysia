use std::error::Error;

use limitless::client::expansion::limitless_expansion_v1::limitless_expansion_v1::LimitlessExpansionV1;
use wayland_client::globals::{BindError, GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};

struct Toggle;

impl Dispatch<WlRegistry, GlobalListContents> for Toggle {
    fn event(
        _state: &mut Toggle,
        _registry: &WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalListContents,
        _connection: &Connection,
        _queue: &QueueHandle<Toggle>,
    ) {
    }
}

delegate_noop!(Toggle: LimitlessExpansionV1);

fn main() -> Result<(), Box<dyn Error>> {
    let connection = Connection::connect_to_env()?;
    let (globals, queue) = registry_queue_init::<Toggle>(&connection)?;
    let expansion: LimitlessExpansionV1 =
        globals
            .bind(&queue.handle(), 1..=1, ())
            .map_err(|error| match error {
                BindError::NotPresent => {
                    "the compositor does not offer limitless_expansion_v1".into()
                }
                other => Box::<dyn Error>::from(other),
            })?;
    expansion.toggle();
    connection.roundtrip()?;
    Ok(())
}
