//! The service as seen from the window, without Tauri types (unit-testable).

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use freekerf_service::automation::PERMISSION_TIMEOUT;
use freekerf_service::permissions::GrantScope;
use freekerf_service::preview::{HEAD_FRAME_BYTES, head_frame};
use freekerf_service::remote::{ServeOptions, endpoint, serve};
use freekerf_service::{ActionError, ActionSpec, Caller, CallerKind, Service, Stores, Tier};
use serde_json::Value;

/// Actions slow enough (image processing, planning) to run off the window's main
/// thread, where a synchronous command blocks the whole window. Every other action
/// keeps the ordered `dispatch`: the window sends quick ones without waiting.
pub const BACKGROUND_ACTIONS: &[&str] = &[
    "document.raster_preview",
    "document.import_data",
    "job.from_document",
];

/// The service plus the caller the window acts as.
#[derive(Debug, Clone)]
pub struct Bridge {
    service: Service,
    caller: Caller,
}

impl Bridge {
    /// A bridge over `service`. The window may use every tier up to `machine`;
    /// `laser` actions are refused here (they need the fire confirmation).
    #[must_use]
    pub fn new(service: Service) -> Self {
        Self {
            service,
            caller: Caller::new(CallerKind::User, "desktop", Tier::Machine),
        }
    }

    /// A bridge over a service with the per-user files (`$FREEKERF_HOME` overrides).
    ///
    /// # Errors
    /// No home directory, or the settings file is invalid.
    pub fn from_env() -> Result<Self, String> {
        let home = std::env::var_os("FREEKERF_HOME").map(std::path::PathBuf::from);
        let stores = Stores::files(home.as_deref())?;
        let bridge = Service::new(stores).map(Self::new).map_err(|e| e.to_string())?;
        // a person is here: scripts and agents ask them in the window
        bridge.service.install_window_prompter(PERMISSION_TIMEOUT);
        bridge.serve_endpoint(home.as_deref());
        Ok(bridge)
    }

    /// Serves the local endpoint (agents through `freekerf mcp`, PLAN §9.6) on a
    /// thread; agents are refused while MCP is off in the settings. Another running
    /// instance keeps the endpoint (logged).
    fn serve_endpoint(&self, home: Option<&std::path::Path>) {
        let endpoint = match endpoint(home) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("no local endpoint: {e}");
                return;
            }
        };
        let service = self.service.clone();
        let spawned = thread::Builder::new()
            .name("freekerf-endpoint".into())
            .spawn(move || {
                if let Err(e) = serve(&service, &endpoint, ServeOptions::default()) {
                    tracing::warn!("local endpoint not served: {e}");
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("no local endpoint: {e}");
        }
    }

    /// The person's answer to a permission request (`None`: denied).
    #[must_use]
    pub fn answer_permission(&self, id: u64, scope: Option<GrantScope>) -> bool {
        self.service.answer_permission(id, scope)
    }

    /// Changes a setting as the person (also `automation.*` and `safety.*`).
    ///
    /// # Errors
    /// Unknown key, invalid value, or the file cannot be saved.
    pub fn set_user_setting(&self, key: &str, value: Value) -> Result<(), ActionError> {
        self.service.set_user_setting(key, value)
    }

    /// Revokes the session and "always" grants of a macro or an agent.
    ///
    /// # Errors
    /// The settings file cannot be saved.
    pub fn revoke(&self, kind: CallerKind, name: &str) -> Result<usize, ActionError> {
        self.service.revoke(kind, name)
    }

    /// Stops every running script at once (the person's Cancel).
    pub fn cancel_scripts(&self) {
        self.service.cancel_scripts();
    }

    /// Runs an action.
    ///
    /// # Errors
    /// See [`ActionError`].
    pub fn dispatch(&self, action: &str, params: Value) -> Result<Value, ActionError> {
        self.service.dispatch(&self.caller, action, params)
    }

    /// Runs one of [`BACKGROUND_ACTIONS`] (the `dispatch_background` command, off
    /// the window's main thread).
    ///
    /// # Errors
    /// [`ActionError::Refused`] for any other action, else see [`ActionError`].
    pub fn dispatch_background(&self, action: &str, params: Value) -> Result<Value, ActionError> {
        if !BACKGROUND_ACTIONS.contains(&action) {
            return Err(ActionError::Refused(format!(
                "`{action}` does not run in the background"
            )));
        }
        self.dispatch(action, params)
    }

    /// Runs the pending laser request after the person pressed "Fire laser"
    /// (`Service::confirm_fire`). The only way the window runs a `laser` action.
    ///
    /// # Errors
    /// No such request, countdown not over, an out-of-date job not accepted, or
    /// the action's error.
    pub fn confirm_fire(&self, id: u64, accept_stale: bool) -> Result<Value, ActionError> {
        self.service.confirm_fire(id, accept_stale)
    }

    /// Every action spec.
    #[must_use]
    pub fn catalog(&self) -> Vec<ActionSpec> {
        self.service.specs()
    }

    /// The tool path of the loaded program (binary, `freekerf_service::preview`).
    #[must_use]
    pub fn preview(&self) -> Option<Arc<Vec<u8>>> {
        self.service.preview()
    }

    /// The document's drawable geometry (binary, `freekerf_service::preview::encode_document`).
    #[must_use]
    pub fn document_geometry(&self) -> Vec<u8> {
        self.service.document_geometry()
    }

    /// The current head frame (`freekerf_service::preview::head_frame`).
    #[must_use]
    pub fn head(&self) -> [u8; HEAD_FRAME_BYTES] {
        head_frame(&self.service.snapshot())
    }

    /// Sends a head frame to `send` every `period` when it changed, on a thread,
    /// until `send` returns `false`.
    pub fn forward_head(&self, period: Duration, send: impl Fn(Vec<u8>) -> bool + Send + 'static) {
        let bridge = self.clone();
        let spawned = thread::Builder::new()
            .name("freekerf-desktop-head".into())
            .spawn(move || {
                let mut last = None;
                loop {
                    let frame = bridge.head();
                    if last != Some(frame) {
                        if !send(frame.to_vec()) {
                            break;
                        }
                        last = Some(frame);
                    }
                    thread::sleep(period);
                }
            });
        if let Err(e) = spawned {
            tracing::error!("cannot start the head forwarder: {e}");
        }
    }

    /// Forwards every service event as JSON to `send` on a thread, until `send`
    /// returns `false` (the window went away).
    pub fn forward_events(&self, send: impl Fn(Value) -> bool + Send + 'static) {
        let rx = self.service.subscribe();
        let spawned = thread::Builder::new()
            .name("freekerf-desktop-events".into())
            .spawn(move || {
                for e in rx {
                    if !send(e.to_json()) {
                        break;
                    }
                }
            });
        if let Err(e) = spawned {
            tracing::error!("cannot start the event forwarder: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bridge() -> Bridge {
        Bridge::new(Service::in_memory().unwrap())
    }

    #[test]
    fn reads_state_and_catalog() {
        let b = bridge();
        let state = b.dispatch("state.get", serde_json::json!({})).unwrap();
        assert_eq!(state["machine"]["connected"], false);
        assert!(b.catalog().iter().any(|s| s.name == "machine.stop"));
    }

    #[test]
    fn only_slow_actions_run_in_the_background() {
        let b = bridge();
        let names: Vec<String> = b.catalog().into_iter().map(|s| s.name).collect();
        for a in BACKGROUND_ACTIONS {
            assert!(names.iter().any(|n| n == a), "{a} is not an action");
        }
        assert!(matches!(
            b.dispatch_background("state.get", serde_json::json!({})),
            Err(ActionError::Refused(_))
        ));
        // the same registry and checks as `dispatch`
        assert!(matches!(
            b.dispatch_background("document.raster_preview", serde_json::json!({"id": 99})),
            Err(e) if !matches!(e, ActionError::Refused(_))
        ));
    }

    #[test]
    fn laser_actions_are_refused_without_confirmation() {
        let b = bridge();
        for action in ["job.run", "machine.send", "machine.custom_code"] {
            let e = b
                .dispatch(action, serde_json::json!({"line": "M3", "code": "M3"}))
                .unwrap_err();
            assert!(
                matches!(
                    e,
                    ActionError::PermissionDenied { .. } | ActionError::InvalidParams(_)
                ),
                "{action}: {e:?}"
            );
        }
        let e = b.dispatch("job.run", serde_json::json!({})).unwrap_err();
        assert!(matches!(e, ActionError::PermissionDenied { .. }), "{e:?}");
    }

    #[test]
    fn preview_and_head() {
        let b = bridge();
        assert!(b.preview().is_none());
        b.dispatch(
            "job.load_text",
            serde_json::json!({"name": "a.nc", "gcode": "M3 S100\nG1 X10 F100\n"}),
        )
        .unwrap();
        let p = b.preview().unwrap();
        assert_eq!(&p[0..4], b"FKTP");
        assert_eq!(b.head().len(), HEAD_FRAME_BYTES);
        // the document's geometry, after a shape is drawn from the window
        assert_eq!(b.document_geometry()[8..12], [0, 0, 0, 0]);
        b.dispatch(
            "document.add_shape",
            serde_json::json!({"shape": "rect", "x": 0, "y": 0, "width": 5, "height": 5}),
        )
        .unwrap();
        assert_eq!(b.document_geometry()[8..12], [4, 0, 0, 0]);
    }

    #[test]
    fn head_frames_are_sent_on_change_until_the_receiver_goes() {
        let b = bridge();
        let (tx, rx) = std::sync::mpsc::channel();
        b.forward_head(Duration::from_millis(1), move |f| tx.send(f).is_ok());
        let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(first.len(), HEAD_FRAME_BYTES);
        // unchanged state: no second frame
        assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
        drop(rx);
    }

    #[test]
    fn every_laser_action_needs_the_confirmation() {
        let b = bridge();
        let laser: Vec<_> = b
            .catalog()
            .into_iter()
            .filter(|s| s.tier == Tier::Laser)
            .collect();
        assert!(laser.len() >= 4);
        for spec in laser {
            let e = b.dispatch(&spec.name, serde_json::json!({})).unwrap_err();
            assert!(
                matches!(e, ActionError::PermissionDenied { .. }),
                "{}: {e:?}",
                spec.name
            );
        }
        // asking opens the confirmation; confirming at once is refused (countdown)
        let r = b
            .dispatch(
                "fire.request",
                serde_json::json!({"action": "machine.send", "params": {"line": "M3"}}),
            )
            .unwrap();
        let e = b.confirm_fire(r["id"].as_u64().unwrap(), false).unwrap_err();
        assert!(matches!(e, ActionError::Refused(_)), "{e:?}");
    }

    #[test]
    fn stop_is_always_allowed() {
        bridge().dispatch("machine.stop", serde_json::json!({})).unwrap();
    }

    #[test]
    fn events_are_forwarded_as_json() {
        let b = bridge();
        let (tx, rx) = std::sync::mpsc::channel();
        b.forward_events(move |e| tx.send(e).is_ok());
        b.dispatch("machine.connect", serde_json::json!({"emulator": true}))
            .unwrap();
        let first = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(first["event"].is_string(), "{first}");
        b.dispatch("machine.disconnect", serde_json::json!({})).unwrap();
    }
}
