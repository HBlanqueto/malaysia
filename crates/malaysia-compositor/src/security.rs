//! `wp_security_context_v1`: sandboxed clients lose the privileged globals.

use std::sync::Arc;

use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::calloop::ping::Ping;
use smithay::reexports::wayland_server::Client;
use smithay::wayland::security_context::{
    SecurityContext, SecurityContextHandler, SecurityContextListenerSource,
};

use crate::idle::IdleHost;
use crate::state::{ClientDisconnectQueue, ClientState, State};

pub(crate) fn unrestricted(client: &Client) -> bool {
    client
        .get_data::<ClientState>()
        .is_some_and(|state| !state.restricted)
}

impl SecurityContextHandler for State {
    fn context_created(&mut self, source: SecurityContextListenerSource, context: SecurityContext) {
        tracing::info!(
            engine = ?context.sandbox_engine,
            app_id = ?context.app_id,
            "security context created"
        );
        self.security_listeners.push(source);
    }
}

pub(crate) fn listen<D: IdleHost + 'static>(
    handle: &LoopHandle<'static, D>,
    sources: Vec<SecurityContextListenerSource>,
    queue: &ClientDisconnectQueue,
    ping: &Ping,
) {
    for source in sources {
        let queue = queue.clone();
        let ping = ping.clone();
        let inserted = handle.insert_source(source, move |stream, _, data: &mut D| {
            let client = ClientState::restricted(queue.clone(), ping.clone());
            let mut display = data.idle_state().display_handle.clone();
            match display.insert_client(stream, Arc::new(client)) {
                Ok(_) => tracing::info!("sandboxed client connected"),
                Err(error) => tracing::warn!(%error, "rejected a sandboxed client"),
            }
        });
        if let Err(error) = inserted {
            tracing::warn!(%error, "listening on a security context");
        }
    }
}
