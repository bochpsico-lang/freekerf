//! View state: what the app shows, as data (PLAN §9, M4a). The desktop UI renders
//! it and changes it only through `view.*` actions, so scripts, agents and tests
//! see and drive the same screen a person does.
//!
//! Purely visual, transient things (hover, animations, a drag in progress, the
//! camera while a gesture runs) stay in the UI; the viewport is committed here when
//! a gesture ends.

use std::collections::BTreeMap;

use schemars::JsonSchema;

use crate::fire::FireRequest;
use serde::{Deserialize, Serialize};

/// Most toasts kept at once (`design/design-system.md`: at most 3 stacked).
pub const MAX_TOASTS: usize = 3;

/// Workspace mode (`design/ux.md`: one workspace, two modes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Editing tools, palette, inspector.
    #[default]
    Design,
    /// Machine control; the canvas is a preview.
    Produce,
}

/// Active tool of the Design mode (`design/interaction.md` §2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    /// Select, move, resize, rotate.
    #[default]
    Select,
    /// Draw a rectangle.
    Rect,
    /// Draw an ellipse.
    Ellipse,
    /// Place a text.
    Text,
}

/// Panels of the dock (wide layout) or the drawers (medium layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Panel {
    /// Colour layers.
    Layers,
    /// Inspector of the selection.
    Object,
    /// Connection, job, jog, custom buttons.
    Machine,
    /// G-code console.
    Console,
}

/// Dialogs. Dialogs are not modal (STOP stays reachable); several can be open,
/// the last one is on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Dialog {
    /// Settings.
    Settings,
    /// Keyboard shortcuts.
    Shortcuts,
    /// "Ready to burn?": checklist and countdown before any laser action.
    FireConfirmation,
    /// Job recovery after an interruption.
    Recover,
    /// Welcome flow.
    Welcome,
    /// Import (Design mode).
    Import,
    /// How the selected image is engraved (Design mode, raster pipeline).
    Raster,
    /// The material test grid (Design mode).
    MaterialTest,
    /// About FreeKerf.
    About,
}

/// Kind of a toast.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToastLevel {
    /// Information.
    #[default]
    Info,
    /// Success.
    Ok,
    /// Warning.
    Warning,
    /// Error.
    Error,
}

/// A short message. `message` is an i18n key (or plain text when no key exists),
/// `args` its placeholders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Toast {
    /// Id, increasing.
    pub id: u64,
    /// Kind.
    pub level: ToastLevel,
    /// i18n key or text.
    pub message: String,
    /// Placeholders.
    #[serde(default)]
    pub args: BTreeMap<String, String>,
}

/// What part of the table is shown: the world point at the centre and the zoom.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    /// Centre `[x, y]`, mm.
    pub center: [f64; 2],
    /// Zoom, CSS pixels per mm.
    pub zoom: f64,
}

/// The whole view state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ViewState {
    /// Mode.
    pub mode: Mode,
    /// Active tool of the Design mode.
    pub tool: Tool,
    /// Active panel of the dock.
    pub panel: Panel,
    /// The panel drawer is open (medium layout).
    pub drawer_open: bool,
    /// Open dialogs, bottom to top.
    pub dialogs: Vec<Dialog>,
    /// Toasts, oldest first (at most [`MAX_TOASTS`]).
    pub toasts: Vec<Toast>,
    /// Viewport, once the UI or a caller set it.
    pub viewport: Option<Viewport>,
    /// Increased by `view.fit`: the UI fits the work area when it changes.
    pub fit_request: u64,
    /// Element with the keyboard focus (`canvas`, `console`... free text), if known.
    pub focus: Option<String>,
    /// The laser action waiting for the person's confirmation, if any (the fire
    /// confirmation dialog is open while it is set).
    pub fire_request: Option<FireRequest>,
    /// Next toast id.
    #[serde(skip)]
    next_toast: u64,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            mode: Mode::Design,
            tool: Tool::Select,
            panel: Panel::Machine,
            drawer_open: false,
            dialogs: Vec::new(),
            toasts: Vec::new(),
            viewport: None,
            fit_request: 0,
            focus: None,
            fire_request: None,
            next_toast: 1,
        }
    }
}

/// A change of the view state.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewCommand {
    /// Switches the mode; Produce opens the machine panel.
    SetMode(Mode),
    /// Chooses the Design mode tool.
    SetTool(Tool),
    /// Shows a panel (and opens the drawer).
    OpenPanel(Panel),
    /// Closes the drawer when it shows this panel.
    ClosePanel(Panel),
    /// Opens a dialog on top (moves it up if already open).
    OpenDialog(Dialog),
    /// Closes a dialog (the top one when `None`). Closing the fire confirmation
    /// drops its request: it can no longer be confirmed.
    CloseDialog(Option<Dialog>),
    /// Adds a toast.
    Toast {
        /// Kind.
        level: ToastLevel,
        /// i18n key or text.
        message: String,
        /// Placeholders.
        args: BTreeMap<String, String>,
    },
    /// Removes a toast.
    DismissToast(u64),
    /// Sets the viewport.
    SetViewport(Viewport),
    /// Asks the UI to fit the work area.
    Fit,
    /// Records the focus.
    Focus(Option<String>),
    /// Shows (opens the fire confirmation dialog) or clears (closes it) the pending
    /// laser request. Set by the service, not an action of its own.
    SetFireRequest(Option<FireRequest>),
}

/// A command that cannot be applied.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ViewError {
    /// Zoom not positive or not finite, or a centre not finite.
    #[error("invalid viewport")]
    BadViewport,
    /// Empty toast message.
    #[error("empty toast message")]
    EmptyToast,
}

impl ViewState {
    /// Applies a command; returns the id of a new toast for [`ViewCommand::Toast`].
    ///
    /// # Errors
    /// See [`ViewError`].
    pub fn apply(&mut self, cmd: ViewCommand) -> Result<Option<u64>, ViewError> {
        match cmd {
            ViewCommand::SetMode(m) => {
                self.mode = m;
                if m == Mode::Produce {
                    self.panel = Panel::Machine;
                }
            }
            ViewCommand::SetTool(t) => self.tool = t,
            ViewCommand::OpenPanel(p) => {
                self.panel = p;
                self.drawer_open = true;
            }
            ViewCommand::ClosePanel(p) => {
                if self.panel == p {
                    self.drawer_open = false;
                }
            }
            ViewCommand::OpenDialog(d) => {
                self.dialogs.retain(|x| *x != d);
                self.dialogs.push(d);
            }
            ViewCommand::CloseDialog(d) => {
                let Some(d) = d.or_else(|| self.top_dialog()) else {
                    return Ok(None);
                };
                self.dialogs.retain(|x| *x != d);
                if d == Dialog::FireConfirmation {
                    self.fire_request = None;
                }
            }
            ViewCommand::Toast { level, message, args } => {
                if message.trim().is_empty() {
                    return Err(ViewError::EmptyToast);
                }
                let id = self.next_toast;
                self.next_toast += 1;
                self.toasts.push(Toast {
                    id,
                    level,
                    message,
                    args,
                });
                let extra = self.toasts.len().saturating_sub(MAX_TOASTS);
                self.toasts.drain(..extra);
                return Ok(Some(id));
            }
            ViewCommand::DismissToast(id) => self.toasts.retain(|t| t.id != id),
            ViewCommand::SetViewport(v) => {
                if !(v.center.iter().all(|c| c.is_finite()) && v.zoom.is_finite() && v.zoom > 0.0) {
                    return Err(ViewError::BadViewport);
                }
                self.viewport = Some(v);
            }
            ViewCommand::Fit => self.fit_request = self.fit_request.wrapping_add(1),
            ViewCommand::Focus(f) => self.focus = f,
            ViewCommand::SetFireRequest(r) => {
                self.dialogs.retain(|d| *d != Dialog::FireConfirmation);
                if r.is_some() {
                    self.dialogs.push(Dialog::FireConfirmation);
                }
                self.fire_request = r;
            }
        }
        Ok(None)
    }

    /// The dialog on top, if any.
    #[must_use]
    pub fn top_dialog(&self) -> Option<Dialog> {
        self.dialogs.last().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_panels() {
        let mut v = ViewState::default();
        assert_eq!((v.mode, v.panel), (Mode::Design, Panel::Machine));
        v.apply(ViewCommand::OpenPanel(Panel::Layers)).unwrap();
        assert_eq!(
            (v.mode, v.panel, v.drawer_open),
            (Mode::Design, Panel::Layers, true)
        );
        // closing another panel leaves the drawer open
        v.apply(ViewCommand::ClosePanel(Panel::Console)).unwrap();
        assert!(v.drawer_open);
        v.apply(ViewCommand::ClosePanel(Panel::Layers)).unwrap();
        assert!(!v.drawer_open);
        assert_eq!(v.tool, Tool::Select);
        v.apply(ViewCommand::SetTool(Tool::Ellipse)).unwrap();
        assert_eq!(v.tool, Tool::Ellipse);
        v.apply(ViewCommand::SetMode(Mode::Produce)).unwrap();
        assert_eq!(v.panel, Panel::Machine);
    }

    #[test]
    fn dialogs_stack() {
        let mut v = ViewState::default();
        v.apply(ViewCommand::OpenDialog(Dialog::Settings)).unwrap();
        v.apply(ViewCommand::OpenDialog(Dialog::Shortcuts)).unwrap();
        v.apply(ViewCommand::OpenDialog(Dialog::Settings)).unwrap();
        assert_eq!(v.dialogs, [Dialog::Shortcuts, Dialog::Settings]);
        assert_eq!(v.top_dialog(), Some(Dialog::Settings));
        v.apply(ViewCommand::CloseDialog(None)).unwrap();
        assert_eq!(v.dialogs, [Dialog::Shortcuts]);
        v.apply(ViewCommand::CloseDialog(Some(Dialog::Shortcuts)))
            .unwrap();
        assert!(v.dialogs.is_empty());
        v.apply(ViewCommand::CloseDialog(None)).unwrap();
    }

    #[test]
    fn toasts_are_capped() {
        let mut v = ViewState::default();
        let mut ids = Vec::new();
        for i in 0..5 {
            let id = v
                .apply(ViewCommand::Toast {
                    level: ToastLevel::Info,
                    message: format!("m{i}"),
                    args: BTreeMap::new(),
                })
                .unwrap()
                .unwrap();
            ids.push(id);
        }
        assert_eq!(v.toasts.iter().map(|t| t.id).collect::<Vec<_>>(), ids[2..]);
        v.apply(ViewCommand::DismissToast(ids[3])).unwrap();
        assert_eq!(v.toasts.len(), 2);
        let empty = v.apply(ViewCommand::Toast {
            level: ToastLevel::Error,
            message: " ".into(),
            args: BTreeMap::new(),
        });
        assert_eq!(empty, Err(ViewError::EmptyToast));
    }

    #[test]
    fn viewport_fit_and_focus() {
        let mut v = ViewState::default();
        let good = Viewport {
            center: [10.0, 20.0],
            zoom: 2.0,
        };
        v.apply(ViewCommand::SetViewport(good)).unwrap();
        assert_eq!(v.viewport, Some(good));
        for bad in [
            Viewport { zoom: 0.0, ..good },
            Viewport {
                center: [f64::NAN, 0.0],
                ..good
            },
            Viewport {
                zoom: f64::INFINITY,
                ..good
            },
        ] {
            assert_eq!(
                v.apply(ViewCommand::SetViewport(bad)),
                Err(ViewError::BadViewport)
            );
        }
        v.apply(ViewCommand::Fit).unwrap();
        v.apply(ViewCommand::Fit).unwrap();
        assert_eq!(v.fit_request, 2);
        v.apply(ViewCommand::Focus(Some("canvas".into()))).unwrap();
        assert_eq!(v.focus.as_deref(), Some("canvas"));
    }

    #[test]
    fn fire_requests_open_and_close_the_dialog() {
        let mut v = ViewState::default();
        let r = crate::fire::FireGate::default().request(
            "job.run",
            serde_json::Value::Null,
            "desktop",
            crate::fire::FireSummary::default(),
            3000,
            0,
        );
        v.apply(ViewCommand::OpenDialog(Dialog::Settings)).unwrap();
        v.apply(ViewCommand::SetFireRequest(Some(r))).unwrap();
        assert_eq!(v.top_dialog(), Some(Dialog::FireConfirmation));
        assert!(v.fire_request.is_some());
        v.apply(ViewCommand::SetFireRequest(None)).unwrap();
        assert_eq!(v.dialogs, [Dialog::Settings]);
        assert!(v.fire_request.is_none());
        // closing the dialog (Esc, a script) drops the request too
        let r = crate::fire::FireGate::default().request(
            "job.run",
            serde_json::Value::Null,
            "desktop",
            crate::fire::FireSummary::default(),
            3000,
            0,
        );
        v.apply(ViewCommand::SetFireRequest(Some(r))).unwrap();
        v.apply(ViewCommand::CloseDialog(None)).unwrap();
        assert!(v.fire_request.is_none());
        assert_eq!(v.dialogs, [Dialog::Settings]);
    }

    #[test]
    fn serializes_for_the_ui() {
        let v = serde_json::to_value(ViewState::default()).unwrap();
        assert_eq!(v["mode"], "design");
        assert_eq!(v["panel"], "machine");
        assert!(v["viewport"].is_null());
    }
}
