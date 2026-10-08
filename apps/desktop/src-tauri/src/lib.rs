//! The FreeKerf desktop app (ADR 0006): a Tauri 2 window over
//! [`freekerf_service::Service`]. The web front end in `ui/web` reaches the app only
//! through the commands of [`commands`]; it never sees the protocol or transports.

pub mod bridge;

/// Starts the app (blocks until the last window closes).
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .try_init();
    let bridge = match bridge::Bridge::from_env() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("cannot start the service: {e}");
            return;
        }
    };
    let result = tauri::Builder::default()
        .manage(bridge)
        .invoke_handler(tauri::generate_handler![
            commands::dispatch,
            commands::catalog,
            commands::subscribe
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        tracing::error!("the app stopped with an error: {e}");
    }
}

/// The Tauri commands: the whole surface the web front end can call.
pub mod commands {
    use freekerf_service::{ActionError, ActionSpec};
    use serde_json::Value;
    use tauri::ipc::Channel;

    use crate::bridge::Bridge;

    /// Runs an action of the registry as the desktop user.
    ///
    /// # Errors
    /// The action failed, was refused or needs a higher tier.
    #[tauri::command]
    #[allow(clippy::needless_pass_by_value)] // Tauri passes owned arguments
    pub fn dispatch(
        bridge: tauri::State<'_, Bridge>,
        action: String,
        params: Value,
    ) -> Result<Value, ActionError> {
        bridge.dispatch(&action, params)
    }

    /// The action catalogue (names, tiers, parameter schemas).
    #[tauri::command]
    #[allow(clippy::needless_pass_by_value)]
    pub fn catalog(bridge: tauri::State<'_, Bridge>) -> Vec<ActionSpec> {
        bridge.catalog()
    }

    /// Streams service events (JSON) to the front end until the channel closes.
    #[tauri::command]
    #[allow(clippy::needless_pass_by_value)]
    pub fn subscribe(bridge: tauri::State<'_, Bridge>, events: Channel<Value>) {
        bridge.forward_events(move |e| events.send(e).is_ok());
    }
}
