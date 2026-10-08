//! The service as seen from the window, without Tauri types (unit-testable).

use std::sync::Arc;
use std::thread;
use std::time::Duration;

use freekerf_service::preview::{HEAD_FRAME_BYTES, head_frame};
use freekerf_service::{ActionError, ActionSpec, Caller, CallerKind, Service, Stores, Tier};
use serde_json::Value;

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
        Service::new(stores).map(Self::new).map_err(|e| e.to_string())
    }

    /// Runs an action.
    ///
    /// # Errors
    /// See [`ActionError`].
    pub fn dispatch(&self, action: &str, params: Value) -> Result<Value, ActionError> {
        self.service.dispatch(&self.caller, action, params)
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
