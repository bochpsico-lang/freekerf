//! The document, selection, history and view actions (PLAN §9.1). They are
//! registered into the host's registry ([`register_session_actions`]), next to the
//! machine actions, so the UI, scripts and agents reach both the same way.
//! Reading or writing files is the host's job: these actions take bytes and text.

use std::collections::BTreeSet;
use std::sync::{Mutex, MutexGuard};

use base64::Engine;
use freekerf_document::{
    Bytes, Color, Document, ImageData, ImageObject, Layer, LayerId, LayerMode, ObjectId, ObjectKind,
    RasterSettings, Source, SourceFormat,
};
use freekerf_geometry::Point;
use freekerf_geometry::hatch::FillPattern;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::session::{Mode, Session, Snapshot, ViewState, Viewport};
use crate::view::{Dialog, Panel, ToastLevel, Tool, ViewCommand};
use crate::{ActionError, NoParams, Registry, RegistryError, Tier};

/// A registry context that holds a session.
pub trait SessionHost: Send + Sync + 'static {
    /// The session.
    fn session(&self) -> &Mutex<Session>;
}

fn lock<C: SessionHost>(c: &C) -> MutexGuard<'_, Session> {
    c.session()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn failed(e: impl std::fmt::Display) -> ActionError {
    ActionError::Failed(e.to_string())
}

/// Applies a view command to the session's view; returns a new toast's id.
///
/// # Errors
/// An invalid viewport or an empty toast.
pub fn view_cmd<C: SessionHost>(c: &C, cmd: ViewCommand) -> Result<Option<u64>, ActionError> {
    let mut s = lock(c);
    let mut v = s.view().clone();
    let r = v
        .apply(cmd)
        .map_err(|e| ActionError::InvalidParams(e.to_string()))?;
    s.change_view(|view| *view = v);
    Ok(r)
}

fn view_unit<C: SessionHost>(c: &C, cmd: ViewCommand) -> Result<Changed, ActionError> {
    view_cmd(c, cmd).map(|_| Changed { changed: true })
}

/// Object ids given by a caller, or the selection when none.
fn targets(ids: Option<Vec<u64>>, s: &Session) -> BTreeSet<ObjectId> {
    ids.map_or_else(
        || s.selection().clone(),
        |v| v.into_iter().map(ObjectId).collect(),
    )
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FkprojParams {
    /// The project file's text (`.fkproj`, JSON).
    text: String,
}

#[derive(Serialize, JsonSchema)]
struct FkprojText {
    /// The project as `.fkproj` text.
    text: String,
}

/// File formats of `document.import_data`.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum ImportFormat {
    /// SVG drawing (colour layers).
    Svg,
    /// ASCII DXF drawing.
    Dxf,
    /// PNG, JPEG, BMP or GIF image.
    Image,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ImportParams {
    /// File format.
    format: ImportFormat,
    /// The file's bytes, base64 (or `text`).
    #[serde(default)]
    data: Option<String>,
    /// SVG or DXF: the file as text instead of `data` (for scripts that build a
    /// drawing).
    #[serde(default)]
    text: Option<String>,
    /// Name of the file (kept as the objects' source).
    #[serde(default)]
    name: String,
    /// Images: resolution in dots per inch (default 96); position is the origin.
    #[serde(default)]
    dpi: Option<f64>,
    /// Speed of new layers, mm/min (default 1000).
    #[serde(default)]
    speed_mm_min: Option<u32>,
    /// Power of new layers, `S` (default 1000).
    #[serde(default)]
    power: Option<u32>,
}

#[derive(Serialize, JsonSchema)]
struct Imported {
    /// Ids of the new objects.
    objects: Vec<u64>,
}

/// How `document.select` combines the ids with the current selection.
#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum SelectMode {
    /// Select exactly these.
    #[default]
    Replace,
    /// Add these.
    Add,
    /// Remove these.
    Remove,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectParams {
    /// Object ids.
    ids: Vec<u64>,
    /// Replace (default), add or remove.
    #[serde(default)]
    mode: SelectMode,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct IdsParams {
    /// Object ids (default: the selection).
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TranslateParams {
    /// Distance along X, mm.
    dx: f64,
    /// Distance along Y, mm.
    dy: f64,
    /// Object ids (default: the selection).
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MoveToLayerParams {
    /// Layer id.
    layer: u32,
    /// Object ids (default: the selection).
    #[serde(default)]
    ids: Option<Vec<u64>>,
}

#[derive(Deserialize, JsonSchema, Default)]
#[serde(deny_unknown_fields)]
struct LayerFields {
    /// Name.
    #[serde(default)]
    name: Option<String>,
    /// `line`, `fill`, `fill_and_line`, `cut` or `ignore`.
    #[serde(default)]
    mode: Option<LayerMode>,
    /// Speed, mm/min.
    #[serde(default)]
    speed_mm_min: Option<u32>,
    /// `S` power.
    #[serde(default)]
    power: Option<u32>,
    /// Passes (at least 1).
    #[serde(default)]
    passes: Option<u32>,
    /// Shown and planned.
    #[serde(default)]
    visible: Option<bool>,
    /// Filling pattern.
    #[serde(default)]
    fill_pattern: Option<FillPattern>,
    /// Filling lines per mm.
    #[serde(default)]
    lines_per_mm: Option<f64>,
}

impl LayerFields {
    fn apply(self, l: &mut Layer) -> Result<(), ActionError> {
        if let Some(v) = self.name {
            l.name = v;
        }
        if let Some(v) = self.mode {
            l.mode = v;
        }
        if let Some(v) = self.speed_mm_min {
            l.speed_mm_min = v;
        }
        if let Some(v) = self.power {
            l.power = v;
        }
        if let Some(v) = self.passes {
            if v == 0 {
                return Err(ActionError::InvalidParams("passes must be at least 1".into()));
            }
            l.passes = v;
        }
        if let Some(v) = self.visible {
            l.visible = v;
        }
        if let Some(v) = self.fill_pattern {
            l.fill.pattern = v;
        }
        if let Some(v) = self.lines_per_mm {
            if !(v > 0.0 && v.is_finite()) {
                return Err(ActionError::InvalidParams("lines_per_mm must be positive".into()));
            }
            l.fill.lines_per_mm = v;
        }
        Ok(())
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AddLayerParams {
    /// Colour, `#RRGGBB`.
    color: Color,
    /// Other fields (defaults: line, 1000 mm/min, power 1000, 1 pass).
    #[serde(flatten)]
    fields: LayerFields,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct UpdateLayerParams {
    /// Layer id.
    id: u32,
    /// Fields to change.
    #[serde(flatten)]
    fields: LayerFields,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct LayerIdParams {
    /// Layer id.
    id: u32,
}

#[derive(Serialize, JsonSchema)]
struct Changed {
    /// Whether anything changed.
    changed: bool,
}

#[derive(Serialize, JsonSchema)]
struct NewId {
    /// Id of what was created.
    id: u64,
}

#[derive(Serialize, JsonSchema)]
struct HistoryResult {
    /// Label of the step undone or redone; none when there was nothing to do.
    step: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ModeParams {
    /// `design` or `produce`.
    mode: Mode,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ToolParams {
    /// `select`, `rect`, `ellipse` or `text`.
    tool: Tool,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct PanelParams {
    /// `layers`, `object`, `machine` or `console`.
    name: Panel,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DialogParams {
    /// The dialog (`settings`, `shortcuts`, `recover`, `welcome`, `import`, `raster`, `about`...).
    name: Dialog,
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OptionalDialogParams {
    /// The dialog (default: the one on top).
    #[serde(default)]
    name: Option<Dialog>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ToastParams {
    /// `info` (default), `ok`, `warning` or `error`.
    #[serde(default)]
    level: ToastLevel,
    /// i18n key (or plain text).
    message: String,
    /// Placeholders of the message.
    #[serde(default)]
    args: std::collections::BTreeMap<String, String>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ToastIdParams {
    /// Id returned by `view.toast`.
    id: u64,
}

#[derive(Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FocusParams {
    /// Element with the focus (`canvas`, `console`...), none when absent.
    #[serde(default)]
    target: Option<String>,
}

/// Imports a file's bytes into `doc`: drawings by colour layer (an existing layer of
/// the same colour is reused), images on a layer of their own at the origin.
///
/// # Errors
/// The file cannot be read as `format`.
#[allow(clippy::cast_precision_loss)]
fn import_into(doc: &mut Document, p: &ImportParams, bytes: &[u8]) -> Result<Vec<ObjectId>, ActionError> {
    let (speed, power) = (p.speed_mm_min.unwrap_or(1000), p.power.unwrap_or(1000));
    let (format, imported) = match p.format {
        ImportFormat::Svg => (
            SourceFormat::Svg,
            freekerf_import::svg::import(bytes, &freekerf_import::svg::SvgOptions::default())
                .map_err(failed)?,
        ),
        ImportFormat::Dxf => (
            SourceFormat::Dxf,
            freekerf_import::dxf::import(bytes, &freekerf_import::dxf::DxfOptions::default())
                .map_err(failed)?,
        ),
        ImportFormat::Image => {
            let img = freekerf_raster::image::Image::decode(bytes).map_err(failed)?;
            let dpi = p.dpi.filter(|d| *d > 0.0).unwrap_or(96.0);
            let mm = |px: u32| f64::from(px) * 25.4 / dpi;
            let layer = doc.add_layer(Layer::new(LayerId(0), Color::BLACK, speed, power));
            let id = doc
                .add_object_with(
                    layer,
                    ObjectKind::Image(Box::new(ImageObject {
                        data: ImageData::Encoded {
                            media_type: "image/*".into(),
                            bytes: Bytes(bytes.to_vec()),
                        },
                        position: Point::new(0.0, 0.0),
                        size_mm: [mm(img.width()), mm(img.height())],
                        settings: RasterSettings::default(),
                        edits: Vec::new(),
                    })),
                    p.name.clone(),
                    Some(Source {
                        format: SourceFormat::Image,
                        file: p.name.clone(),
                    }),
                )
                .map_err(failed)?;
            return Ok(vec![id]);
        }
    };
    let source = Source {
        format,
        file: p.name.clone(),
    };
    let mut ids = Vec::new();
    for o in imported.objects {
        let layer = doc.layer_for_color(o.color, speed, power);
        ids.push(
            doc.add_object_with(layer, o.kind, String::new(), Some(source.clone()))
                .map_err(failed)?,
        );
    }
    Ok(ids)
}

/// Imports a file into the session as one undo step, the format taken from the
/// name's extension (`.svg`, `.dxf`, or an image). Returns the new object ids.
///
/// # Errors
/// Unknown extension, or a file that cannot be read.
pub fn import_file(session: &mut Session, name: &str, bytes: &[u8]) -> Result<Vec<u64>, ActionError> {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let format = match ext.as_str() {
        "svg" => ImportFormat::Svg,
        "dxf" => ImportFormat::Dxf,
        "png" | "jpg" | "jpeg" | "bmp" | "gif" => ImportFormat::Image,
        _ => {
            return Err(ActionError::InvalidParams(format!(
                "cannot import `.{ext}` files"
            )));
        }
    };
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name).to_owned();
    let p = ImportParams {
        format,
        data: None,
        text: None,
        name: base,
        dpi: None,
        speed_mm_min: None,
        power: None,
    };
    let mut ids = Vec::new();
    session.edit("import", |d, _| {
        ids = import_into(d, &p, bytes)?;
        Ok::<_, ActionError>(!ids.is_empty())
    })?;
    Ok(ids.into_iter().map(|i| i.0).collect())
}

/// Registers the session actions into `r`.
///
/// # Errors
/// A name clash with an action the host registered first.
#[allow(clippy::too_many_lines)] // one registration per action
pub fn register_session_actions<C: SessionHost>(r: &mut Registry<C>) -> Result<(), RegistryError> {
    // ---- state
    r.register(
        "session.snapshot",
        "The session as front ends and agents read it: revision, selection, view, undo/redo labels, layers and objects without geometry.",
        Tier::Read,
        |c: &C, _: NoParams| -> Result<Snapshot, ActionError> { Ok(lock(c).snapshot()) },
    )?;
    r.register(
        "document.get",
        "The whole document (layers, objects with geometry and images), as in a `.fkproj` file.",
        Tier::Read,
        |c: &C, _: NoParams| Ok(lock(c).document().clone()),
    )?;
    r.register(
        "document.to_fkproj",
        "The document as `.fkproj` text.",
        Tier::Read,
        |c: &C, _: NoParams| {
            Ok(FkprojText {
                text: lock(c).document().to_fkproj(),
            })
        },
    )?;

    // ---- document
    r.register(
        "document.new",
        "Starts an empty document (the undo history starts again).",
        Tier::Edit,
        |c: &C, _: NoParams| {
            lock(c).replace_document(Document::new());
            Ok(Changed { changed: true })
        },
    )?;
    r.register(
        "document.from_fkproj",
        "Replaces the document with a `.fkproj` text (the undo history starts again).",
        Tier::Edit,
        |c: &C, p: FkprojParams| {
            let doc = Document::from_fkproj(p.text.as_bytes()).map_err(failed)?;
            lock(c).replace_document(doc);
            Ok(Changed { changed: true })
        },
    )?;
    r.register(
        "document.import_data",
        "Imports a file given as base64 bytes (`data`), or as `text` for SVG and DXF, into the document: SVG and DXF by colour layer, images on a layer of their own. One undo step.",
        Tier::Edit,
        |c: &C, p: ImportParams| {
            let bytes = match (&p.data, &p.text) {
                (Some(data), None) => base64::engine::general_purpose::STANDARD
                    .decode(data.as_bytes())
                    .map_err(|e| ActionError::InvalidParams(format!("data: {e}")))?,
                (None, Some(text)) if !matches!(p.format, ImportFormat::Image) => text.clone().into_bytes(),
                (None, Some(_)) => {
                    return Err(ActionError::InvalidParams("images need `data` (base64)".into()));
                }
                _ => return Err(ActionError::InvalidParams("give exactly one of `data` and `text`".into())),
            };
            let mut ids = Vec::new();
            lock(c).edit("import", |d, _| {
                ids = import_into(d, &p, &bytes)?;
                Ok::<_, ActionError>(!ids.is_empty())
            })?;
            Ok(Imported {
                objects: ids.into_iter().map(|i| i.0).collect(),
            })
        },
    )?;
    r.register(
        "document.delete",
        "Deletes objects (default: the selection). One undo step.",
        Tier::Edit,
        |c: &C, p: IdsParams| {
            let mut s = lock(c);
            let ids = targets(p.ids, &s);
            let changed = s.edit("delete", |d, _| {
                let mut any = false;
                for id in &ids {
                    any |= d.remove_object(*id).is_ok();
                }
                Ok::<_, ActionError>(any)
            })?;
            Ok(Changed { changed })
        },
    )?;
    r.register(
        "document.translate",
        "Moves objects (default: the selection) by `dx`, `dy` mm. One undo step.",
        Tier::Edit,
        |c: &C, p: TranslateParams| {
            if !(p.dx.is_finite() && p.dy.is_finite()) {
                return Err(ActionError::InvalidParams("dx and dy must be finite".into()));
            }
            let mut s = lock(c);
            let ids = targets(p.ids, &s);
            let changed = s.edit("move", |d, _| {
                let mut any = false;
                for id in &ids {
                    if let Some(o) = d.object_mut(*id) {
                        match &mut o.kind {
                            ObjectKind::Shape { paths } => {
                                paths.iter_mut().for_each(|path| path.translate(p.dx, p.dy));
                            }
                            ObjectKind::Image(img) => {
                                img.position = Point::new(img.position.x + p.dx, img.position.y + p.dy);
                            }
                            ObjectKind::Unsupported { .. } => {}
                        }
                        any = true;
                    }
                }
                Ok::<_, ActionError>(any && (p.dx != 0.0 || p.dy != 0.0))
            })?;
            Ok(Changed { changed })
        },
    )?;
    r.register(
        "document.move_to_layer",
        "Moves objects (default: the selection) to another layer. One undo step.",
        Tier::Edit,
        |c: &C, p: MoveToLayerParams| {
            let mut s = lock(c);
            let ids = targets(p.ids, &s);
            let changed = s.edit("move to layer", |d, _| {
                if d.layer(LayerId(p.layer)).is_none() {
                    return Err(ActionError::InvalidParams(format!("no layer {}", p.layer)));
                }
                let mut any = false;
                for id in &ids {
                    if d.object(*id).is_some_and(|o| o.layer != LayerId(p.layer)) {
                        d.set_object_layer(*id, LayerId(p.layer)).map_err(failed)?;
                        any = true;
                    }
                }
                Ok(any)
            })?;
            Ok(Changed { changed })
        },
    )?;
    r.register(
        "document.add_layer",
        "Adds a layer of a colour (line, 1000 mm/min, power 1000, 1 pass unless given). One undo step.",
        Tier::Edit,
        |c: &C, p: AddLayerParams| {
            let mut id = 0;
            lock(c).edit("add layer", |d, _| {
                let mut l = Layer::new(LayerId(0), p.color, 1000, 1000);
                p.fields.apply(&mut l)?;
                id = d.add_layer(l).0;
                Ok::<_, ActionError>(true)
            })?;
            Ok(NewId { id: u64::from(id) })
        },
    )?;
    r.register(
        "document.update_layer",
        "Changes a layer's name, mode, speed, power, passes, visibility or filling. One undo step.",
        Tier::Edit,
        |c: &C, p: UpdateLayerParams| {
            let changed = lock(c).edit("layer settings", |d, _| {
                let l = d
                    .layer_mut(LayerId(p.id))
                    .ok_or_else(|| ActionError::InvalidParams(format!("no layer {}", p.id)))?;
                let before = l.clone();
                p.fields.apply(l)?;
                Ok(*l != before)
            })?;
            Ok(Changed { changed })
        },
    )?;
    r.register(
        "document.remove_layer",
        "Removes a layer and its objects. One undo step.",
        Tier::Edit,
        |c: &C, p: LayerIdParams| {
            lock(c).edit("remove layer", |d, _| {
                d.remove_layer(LayerId(p.id)).map_err(failed)?;
                Ok::<_, ActionError>(true)
            })?;
            Ok(Changed { changed: true })
        },
    )?;

    // ---- selection
    r.register(
        "document.select",
        "Selects objects (replace, add to or remove from the selection). Unknown ids are ignored.",
        Tier::Ui,
        |c: &C, p: SelectParams| {
            let mut s = lock(c);
            let ids: BTreeSet<ObjectId> = p.ids.into_iter().map(ObjectId).collect();
            let mut sel = s.selection().clone();
            match p.mode {
                SelectMode::Replace => sel = ids,
                SelectMode::Add => sel.extend(ids),
                SelectMode::Remove => sel.retain(|i| !ids.contains(i)),
            }
            s.select(sel);
            Ok(s.selection().iter().map(|i| i.0).collect::<Vec<_>>())
        },
    )?;
    r.register(
        "document.select_all",
        "Selects every object.",
        Tier::Ui,
        |c: &C, _: NoParams| {
            let mut s = lock(c);
            let all = s.document().objects.iter().map(|o| o.id).collect();
            s.select(all);
            Ok(s.selection().len())
        },
    )?;
    r.register(
        "document.clear_selection",
        "Selects nothing.",
        Tier::Ui,
        |c: &C, _: NoParams| {
            lock(c).select(BTreeSet::new());
            Ok(Changed { changed: true })
        },
    )?;

    // ---- history
    r.register(
        "history.undo",
        "Undoes the last document change.",
        Tier::Edit,
        |c: &C, _: NoParams| Ok(HistoryResult { step: lock(c).undo() }),
    )?;
    r.register(
        "history.redo",
        "Redoes the last undone change.",
        Tier::Edit,
        |c: &C, _: NoParams| Ok(HistoryResult { step: lock(c).redo() }),
    )?;

    // ---- view (the desktop app renders it; scripts and agents drive the same screen)
    r.register(
        "view.get",
        "The view: mode, tool, panel, dialogs, toasts, viewport, focus, the pending laser request.",
        Tier::Read,
        |c: &C, _: NoParams| -> Result<ViewState, ActionError> { Ok(lock(c).view().clone()) },
    )?;
    r.register(
        "view.set_mode",
        "Switches between design and produce (produce shows the machine panel).",
        Tier::Ui,
        |c: &C, p: ModeParams| view_unit(c, ViewCommand::SetMode(p.mode)),
    )?;
    r.register(
        "view.set_tool",
        "Chooses the Design mode tool (select, rectangle, ellipse, text).",
        Tier::Ui,
        |c: &C, p: ToolParams| view_unit(c, ViewCommand::SetTool(p.tool)),
    )?;
    r.register(
        "view.open_panel",
        "Shows a panel of the dock (`layers`, `object`, `machine`, `console`; opens the drawer in the medium layout).",
        Tier::Ui,
        |c: &C, p: PanelParams| view_unit(c, ViewCommand::OpenPanel(p.name)),
    )?;
    r.register(
        "view.close_panel",
        "Closes the drawer when it shows this panel.",
        Tier::Ui,
        |c: &C, p: PanelParams| view_unit(c, ViewCommand::ClosePanel(p.name)),
    )?;
    r.register(
        "view.open_dialog",
        "Opens a dialog on top of the others (dialogs are not modal: STOP stays reachable).",
        Tier::Ui,
        |c: &C, p: DialogParams| view_unit(c, ViewCommand::OpenDialog(p.name)),
    )?;
    r.register(
        "view.close_dialog",
        "Closes a dialog (default: the top one). Closing the fire confirmation drops its laser request.",
        Tier::Ui,
        |c: &C, p: OptionalDialogParams| view_unit(c, ViewCommand::CloseDialog(p.name)),
    )?;
    r.register(
        "view.toast",
        "Shows a short message (at most 3 at once); returns its id.",
        Tier::Ui,
        |c: &C, p: ToastParams| {
            view_cmd(
                c,
                ViewCommand::Toast {
                    level: p.level,
                    message: p.message,
                    args: p.args,
                },
            )
            .map(|id| id.unwrap_or(0))
        },
    )?;
    r.register(
        "view.dismiss_toast",
        "Removes a toast.",
        Tier::Ui,
        |c: &C, p: ToastIdParams| view_unit(c, ViewCommand::DismissToast(p.id)),
    )?;
    r.register(
        "view.set_viewport",
        "Centres the view on a point (mm) at a zoom (CSS pixels per mm).",
        Tier::Ui,
        |c: &C, p: Viewport| view_unit(c, ViewCommand::SetViewport(p)),
    )?;
    r.register(
        "view.fit",
        "Fits the work area in the canvas.",
        Tier::Ui,
        |c: &C, _: NoParams| view_unit(c, ViewCommand::Fit),
    )?;
    r.register(
        "view.focus",
        "Records which element has the keyboard focus.",
        Tier::Ui,
        |c: &C, p: FocusParams| view_unit(c, ViewCommand::Focus(p.target)),
    )?;
    r.describe_result::<ViewState>("view.get")?;
    r.describe_result::<u64>("view.toast")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::Caller;

    struct Host(Mutex<Session>);

    impl SessionHost for Host {
        fn session(&self) -> &Mutex<Session> {
            &self.0
        }
    }

    fn setup() -> (Registry<Host>, Host) {
        let mut r = Registry::new();
        register_session_actions(&mut r).unwrap();
        (r, Host(Mutex::new(Session::new())))
    }

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20">
        <rect x="1" y="1" width="5" height="5" stroke="red" fill="none"/><path d="M1 1 L5 5" stroke="#00f"/></svg>"##;

    fn b64(b: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(b)
    }

    #[test]
    fn document_actions() {
        let (r, h) = setup();
        let user = Caller::user();
        let d = |name: &str, p: Value| r.dispatch(&h, &user, name, p);
        let ids = d(
            "document.import_data",
            json!({"format": "svg", "data": b64(SVG.as_bytes()), "name": "a.svg"}),
        )
        .unwrap();
        let ids: Vec<u64> = serde_json::from_value(ids["objects"].clone()).unwrap();
        assert_eq!(ids.len(), 2);
        let snap = d("session.snapshot", Value::Null).unwrap();
        assert_eq!(snap["document"]["layers"].as_array().unwrap().len(), 2);
        assert_eq!(snap["undo"], json!(["import"]));

        assert_eq!(
            d("document.select", json!({"ids": [ids[0], 999]})).unwrap(),
            json!([ids[0]])
        );
        assert_eq!(
            d("document.select", json!({"ids": [ids[1]], "mode": "add"}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            d("document.select", json!({"ids": [ids[1]], "mode": "remove"})).unwrap(),
            json!([ids[0]])
        );
        assert_eq!(d("document.select_all", Value::Null).unwrap(), json!(2));
        d("document.clear_selection", Value::Null).unwrap();
        d("document.select", json!({"ids": [ids[0]]})).unwrap();
        // move the selection
        let before = d("session.snapshot", Value::Null).unwrap()["document"]["objects"][0]["bounds"].clone();
        assert_eq!(
            d("document.translate", json!({"dx": 10.0, "dy": 0.0})).unwrap(),
            json!({"changed": true})
        );
        let after = d("session.snapshot", Value::Null).unwrap()["document"]["objects"][0]["bounds"].clone();
        assert_eq!(after[0].as_f64().unwrap(), before[0].as_f64().unwrap() + 10.0);
        assert!(d("document.translate", json!({"dx": f64::NAN, "dy": 0.0})).is_err());
        // layers
        let lid = d(
            "document.add_layer",
            json!({"color": "#00FF00", "mode": "cut", "power": 500}),
        )
        .unwrap()["id"]
            .clone();
        assert!(d("document.add_layer", json!({"color": "#00FF00", "passes": 0})).is_err());
        assert_eq!(
            d("document.move_to_layer", json!({"layer": lid})).unwrap(),
            json!({"changed": true})
        );
        assert!(d("document.move_to_layer", json!({"layer": 77})).is_err());
        assert_eq!(
            d("document.update_layer", json!({"id": lid, "speed_mm_min": 300, "lines_per_mm": 5.0, "visible": false, "name": "x", "fill_pattern": "cross"}))
                .unwrap(),
            json!({"changed": true})
        );
        assert_eq!(
            d("document.update_layer", json!({"id": lid, "speed_mm_min": 300})).unwrap(),
            json!({"changed": false})
        );
        assert!(d("document.update_layer", json!({"id": 77})).is_err());
        assert!(d("document.update_layer", json!({"id": lid, "lines_per_mm": -1.0})).is_err());
        assert!(
            d("document.remove_layer", json!({"id": 77})).is_err(),
            "unknown layer"
        );
        // delete, undo, redo
        assert_eq!(
            d("document.delete", json!({"ids": [ids[1]]})).unwrap(),
            json!({"changed": true})
        );
        assert_eq!(
            d("document.delete", json!({"ids": [ids[1]]})).unwrap(),
            json!({"changed": false})
        );
        assert_eq!(d("history.undo", Value::Null).unwrap(), json!({"step": "delete"}));
        assert_eq!(d("history.redo", Value::Null).unwrap(), json!({"step": "delete"}));
        // fkproj round trip
        let text = d("document.to_fkproj", Value::Null).unwrap()["text"]
            .as_str()
            .unwrap()
            .to_owned();
        d("document.new", Value::Null).unwrap();
        assert_eq!(d("document.get", Value::Null).unwrap()["objects"], json!([]));
        d("document.from_fkproj", json!({"text": text})).unwrap();
        assert_eq!(
            d("document.get", Value::Null).unwrap()["objects"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(d("document.from_fkproj", json!({"text": "{}"})).is_err());
        assert!(d("document.import_data", json!({"format": "svg", "data": "!!"})).is_err());
        assert!(d("document.import_data", json!({"format": "svg"})).is_err());
        assert!(d("document.import_data", json!({"format": "image", "text": "x"})).is_err());
        assert!(
            d(
                "document.import_data",
                json!({"format": "svg", "text": "x", "data": "eA=="})
            )
            .is_err()
        );
        let text = d("document.import_data", json!({"format": "svg", "text": SVG})).unwrap();
        assert_eq!(text["objects"].as_array().unwrap().len(), 2);
        d("history.undo", Value::Null).unwrap();
        assert!(
            d(
                "document.import_data",
                json!({"format": "dxf", "data": b64(b"junk")})
            )
            .is_err()
        );
        // a layer goes with its objects, in one undoable step
        let n = |d: &dyn Fn(&str, Value) -> Result<Value, ActionError>| {
            d("document.get", Value::Null).unwrap()["objects"]
                .as_array()
                .unwrap()
                .len()
        };
        let layer = d("document.get", Value::Null).unwrap()["objects"][0]["layer"].clone();
        d("document.remove_layer", json!({"id": layer})).unwrap();
        assert_eq!(n(&d), 0);
        d("history.undo", Value::Null).unwrap();
        assert_eq!(n(&d), 1);
    }

    #[test]
    fn importing_files_by_extension() {
        let mut s = Session::new();
        assert_eq!(import_file(&mut s, "dir/a.SVG", SVG.as_bytes()).unwrap().len(), 2);
        assert_eq!(s.document().objects[0].source.as_ref().unwrap().file, "a.SVG");
        assert!(import_file(&mut s, "a.nc", b"G0").is_err());
        assert!(import_file(&mut s, "noext", b"").is_err());
    }

    #[test]
    fn images_and_dxf() {
        let (r, h) = setup();
        let user = Caller::user();
        let png = freekerf_raster::image::Image::new(96, 48, 0xFF00_0000)
            .encode_png()
            .unwrap();
        let out = r
            .dispatch(
                &h,
                &user,
                "document.import_data",
                json!({"format": "image", "data": b64(&png), "name": "p.png"}),
            )
            .unwrap();
        assert_eq!(out["objects"].as_array().unwrap().len(), 1);
        let doc = r.dispatch(&h, &user, "document.get", Value::Null).unwrap();
        let size = &doc["objects"][0]["size_mm"];
        assert!((size[0].as_f64().unwrap() - 25.4).abs() < 1e-9);
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n5\n21\n5\n0\nENDSEC\n0\nEOF\n";
        let out = r
            .dispatch(
                &h,
                &user,
                "document.import_data",
                json!({"format": "dxf", "data": b64(dxf.as_bytes())}),
            )
            .unwrap();
        assert_eq!(out["objects"].as_array().unwrap().len(), 1);
        assert!(
            r.dispatch(
                &h,
                &user,
                "document.import_data",
                json!({"format": "image", "data": b64(b"x")})
            )
            .is_err()
        );
    }

    #[test]
    fn view_actions_and_tiers() {
        let (r, h) = setup();
        let user = Caller::user();
        let d = |name: &str, p: Value| r.dispatch(&h, &user, name, p);
        d("view.set_mode", json!({"mode": "design"})).unwrap();
        d("view.set_tool", json!({"tool": "rect"})).unwrap();
        d("view.open_panel", json!({"name": "console"})).unwrap();
        d("view.close_panel", json!({"name": "console"})).unwrap();
        d("view.open_panel", json!({"name": "layers"})).unwrap();
        assert!(d("view.open_panel", json!({"name": "nope"})).is_err());
        d("view.set_viewport", json!({"center": [10.0, 20.0], "zoom": 4.0})).unwrap();
        assert!(d("view.set_viewport", json!({"center": [10.0, 20.0], "zoom": 0.0})).is_err());
        d("view.open_dialog", json!({"name": "raster"})).unwrap();
        d("view.open_dialog", json!({"name": "settings"})).unwrap();
        d("view.close_dialog", Value::Null).unwrap();
        let id = d("view.toast", json!({"message": "toast.saved", "level": "ok"})).unwrap();
        assert!(d("view.toast", json!({"message": " "})).is_err());
        d("view.fit", Value::Null).unwrap();
        d("view.focus", json!({"target": "canvas"})).unwrap();
        let v = d("view.get", Value::Null).unwrap();
        assert_eq!(v["mode"], "design");
        assert_eq!(v["tool"], "rect");
        assert_eq!(
            (v["panel"].clone(), v["drawer_open"].clone()),
            (json!("layers"), json!(true))
        );
        assert_eq!(v["dialogs"], json!(["raster"]));
        assert_eq!(v["viewport"], json!({"center": [10.0, 20.0], "zoom": 4.0}));
        assert_eq!(v["toasts"][0]["id"], id);
        assert_eq!(
            (v["fit_request"].clone(), v["focus"].clone()),
            (json!(1), json!("canvas"))
        );
        d("view.dismiss_toast", json!({"id": id})).unwrap();
        d("view.close_dialog", json!({"name": "raster"})).unwrap();
        // every view change is a session event
        assert!(
            h.0.lock()
                .unwrap()
                .drain_events()
                .iter()
                .any(|e| e.change == crate::session::Change::View)
        );
        // a read-only caller cannot select or edit
        let reader = Caller::new(crate::CallerKind::Agent, "a", Tier::Read);
        assert!(r.dispatch(&h, &reader, "session.snapshot", Value::Null).is_ok());
        assert!(matches!(
            r.dispatch(&h, &reader, "document.select_all", Value::Null),
            Err(ActionError::PermissionDenied { .. })
        ));
        // every session action has a description and a tier; names are unique
        assert!(r.specs().all(|s| !s.description.is_empty()));
        assert!(r.len() >= 24);
        let mut again = Registry::<Host>::new();
        register_session_actions(&mut again).unwrap();
        assert!(register_session_actions(&mut again).is_err());
    }
}
