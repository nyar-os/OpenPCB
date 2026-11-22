//! Editable board model, persistence and output contracts for OpenPCB.
//!
//! This crate owns the native `.opcb` document, the process-wide handle store,
//! the editing operations, the design rule checks and the fabrication,
//! assembly and preview writers that the Node-API surface, the command line and
//! the studio all share.
//!
//! The model describes one editable printed circuit board: copper layers and
//! stackup, footprints, nets, tracks and vias, plus the fabrication and assembly
//! outputs derived from them. Every entry point reads the same capability
//! matrix, so no surface can claim a different set of features.
//!
//! Implemented today: create, open, import a KiCad PCB or IPC-2581 document,
//! save, inspect, summary, close, undo and redo, layer management, footprint
//! moves and pad add/update/delete, track and via deletion, plated routed slots,
//! a rectangular outline, editable design rules, waivable findings with per-code
//! severity overrides, a copper area and balance report, a deterministic
//! two-layer autorouter, a read-only SVG preview, and export to drill, BOM,
//! pick-and-place, SVG, PDF, PNG, KiCad PCB, IPC-2581 and Gerber RS-274X (copper,
//! silkscreen, solder mask and edge cuts). STEP remains declared but
//! unimplemented and rejects with `capability/not-implemented`.

#![deny(missing_docs)]

mod board;
mod copper;
mod drill;
mod export;
mod impedance;
mod inspect;
mod ipc2581;
mod ipc2581_import;
mod kicad_import;
mod kicad_pcb;
mod outline;
mod panel;
mod route;
mod store;

#[cfg(test)]
#[path = "../tests/unit/model.rs"]
mod tests;

use std::{fs, path::Path};

pub use board::{
    AddDifferentialPairRequest, AddFootprintRequest, AddKeepoutRequest, AddLayerRequest, AddNetClassRequest, AddNetRequest,
    AddPadRequest, AddSlotRequest, AddStitchingRequest, AddTeardropsRequest, AddTestPointRequest, AddTextRequest,
    AddTrackRequest, AddViaRequest, AddWaiverRequest, AddZoneRequest, AssignNetClassRequest, DeletePadRequest,
    ImpedanceRequest, ImpedanceTableRequest, MoveFootprintRequest, RouteOptions, SetOutlineRequest, SetRulesRequest,
    SetSeverityRequest, SetStackupRequest, SetViaBackDrillRequest, SplitTrackRequest, UpdateFootprintRequest,
    UpdateKeepoutRequest, UpdateLayerRequest, UpdatePadRequest, UpdateSlotRequest, UpdateTextRequest, UpdateTrackRequest,
    UpdateViaRequest, UpdateZoneRequest, WidthRequest,
};
use board::{Board, DOCUMENT_FORMAT, DOCUMENT_VERSION, Document};
pub use outline::OutlineGeometryRequest;
pub use panel::PanelizeRequest;
use serde_json::{Map, Value, json};

/// PCB board formats the product must classify.
///
/// Covers the editable board, fabrication and assembly outputs, exchange
/// containers and preview images.
pub const BOARD_FORMATS: &[&str] = &[
    "kicad_pcb",
    "gbr",
    "drl",
    "drill_map",
    "pos",
    "placement",
    "bom",
    "testpoints",
    "ipc_d_356",
    "step",
    "ipc2581",
    "svg",
    "pdf",
    "png",
];

/// Render targets declared for preview surfaces.
const RENDER_FORMATS: &[&str] = &["svg", "png", "pdf"];

/// Export targets declared by the command line.
const EXPORT_TARGETS: &[&str] = &[
    "kicad_pcb",
    "gbr",
    "drl",
    "drill_map",
    "pos",
    "placement",
    "bom",
    "testpoints",
    "ipc_d_356",
    "step",
    "pdf",
    "ipc2581",
    "svg",
    "png",
];

/// Formats this build can read into the native document.
const SUPPORTED_IMPORTS: &[&str] = &["kicad_pcb", "ipc2581"];

/// Stable failure of one board operation.
#[derive(Debug)]
pub struct ServiceError {
    /// Stable, comparable code.
    pub code: String,
    /// Human-readable message; never carries binary payloads.
    pub message: String,
}

impl ServiceError {
    /// Builds a failure from a stable code and a message.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for ServiceError {}

/// Arguments of [`create`].
#[derive(Debug, Clone, Default)]
pub struct CreateRequest {
    /// Board title.
    pub title: Option<String>,
    /// Template the board starts from; `blank` is the only implemented value.
    pub template: Option<String>,
}

/// Builds a `format/invalid` failure.
pub(crate) fn invalid(message: impl Into<String>) -> ServiceError {
    ServiceError::new("format/invalid", message)
}

/// Builds a `session/argument-invalid` failure.
pub(crate) fn argument(message: impl Into<String>) -> ServiceError {
    ServiceError::new("session/argument-invalid", message)
}

/// What this build can actually do.
///
/// The matrix is the single source of truth for the CLI, the SDK and the
/// studio. A format, check, renderer or target is `supported` only when a test
/// exercises it; everything else stays `unsupported` or `partial`.
pub fn capabilities() -> Value {
    let mut formats = Map::new();
    for &format in BOARD_FORMATS {
        let export = if export::SUPPORTED_EXPORTS.contains(&format) { "supported" } else { "unsupported" };
        let import = if SUPPORTED_IMPORTS.contains(&format) { "supported" } else { "unsupported" };
        let loss_codes: Vec<Value> = match format {
            "kicad_pcb" => kicad_import::LOSS_CODES.iter().map(|code| Value::String((*code).to_string())).collect(),
            "ipc2581" => ipc2581_import::LOSS_CODES.iter().map(|code| Value::String((*code).to_string())).collect(),
            _ => Vec::new(),
        };
        formats.insert(
            format.to_string(),
            json!({ "import": import, "edit": "unsupported", "export": export, "lossCodes": loss_codes }),
        );
    }

    let mut checks = Map::new();
    for &check in inspect::CHECKS {
        let level = if inspect::SUPPORTED_CHECKS.contains(&check) { "supported" } else { "unsupported" };
        checks.insert(check.to_string(), Value::String(level.to_string()));
    }

    let mut renderers = Map::new();
    for &renderer in RENDER_FORMATS {
        let level = if export::SUPPORTED_PREVIEWS.contains(&renderer) { "supported" } else { "unsupported" };
        renderers.insert(renderer.to_string(), Value::String(level.to_string()));
    }

    let mut export_targets = Map::new();
    for &target in EXPORT_TARGETS {
        let level = if export::SUPPORTED_EXPORTS.contains(&target) { "supported" } else { "unsupported" };
        export_targets.insert(target.to_string(), Value::String(level.to_string()));
    }

    json!({
        "engine": { "package": env!("CARGO_PKG_NAME"), "version": env!("CARGO_PKG_VERSION") },
        "project": { "import": "partial", "edit": "partial", "export": "partial" },
        "formats": Value::Object(formats),
        "checks": Value::Object(checks),
        "renderers": Value::Object(renderers),
        "exportTargets": Value::Object(export_targets),
        "vias": {
            "through": "supported",
            "blind": "supported",
            "buried": "supported",
            "micro": "unsupported",
        },
        "differentialPairs": "supported",
        "backDrill": "supported",
        "teardrops": {
            "linear": "supported",
            "arc": "unsupported",
        },
        "designRules": "supported",
        "solderMask": "supported",
        "solderPaste": "supported",
        "gerberX2": "supported",
        "drcPolicy": "supported",
        "routing": "supported",
        "zones": "supported",
        "texts": "supported",
        "keepouts": "supported",
        "viaStitching": "supported",
        "slots": "supported",
        "copperBalance": "supported",
        "netLengths": "supported",
        "zoneReport": "supported",
        "drillReport": "supported",
        "panelization": "supported",
        "polygonOutline": "supported",
        "netClasses": "supported",
        "testPoints": "supported",
        "stackup": "supported",
        "impedance": "supported",
        "impedanceTable": "supported",
    })
}

/// Creates an empty board with the default stackup.
pub fn create(request: CreateRequest) -> Result<Value, ServiceError> {
    let board = Board::new(store::next_board_id(), request.title, request.template)?;
    let handle = store::insert(board);
    locate(handle)
}

/// Opens a native board document and returns its handle.
pub fn open(path: &Path) -> Result<Value, ServiceError> {
    let raw = fs::read_to_string(path).map_err(|error| invalid(format!("cannot read `{}`: {error}", path.display())))?;
    let document: Document = serde_json::from_str(&raw)
        .map_err(|error| invalid(format!("`{}` is not a board document: {error}", path.display())))?;
    if document.format != DOCUMENT_FORMAT {
        return Err(invalid(format!(
            "`{}` has document tag `{}`, expected `{DOCUMENT_FORMAT}`",
            path.display(),
            document.format
        )));
    }
    if document.version != DOCUMENT_VERSION {
        return Err(ServiceError::new(
            "format/unsupported-version",
            format!(
                "`{}` uses document version {}; this build reads version {DOCUMENT_VERSION}",
                path.display(),
                document.version
            ),
        ));
    }
    board::validate(&document.document)?;
    let handle = store::insert(document.document);
    locate(handle)
}

/// Imports one exchange board file as a native board and returns its handle.
///
/// The format is detected from the content: a document starting with `<` is
/// read as IPC-2581, one starting with `(` as a KiCad PCB. The report follows
/// the export report shape and adds the loss ledger: every construct the
/// document cannot hold is listed with a stable code. The board is validated
/// before it receives a handle, so a successful import can always be saved to
/// `.opcb` and reopened.
pub fn import(path: &Path) -> Result<Value, ServiceError> {
    let raw = fs::read(path).map_err(|error| invalid(format!("cannot read `{}`: {error}", path.display())))?;
    let bytes = raw.len() as u64;
    let text = String::from_utf8(raw).map_err(|error| invalid(format!("`{}` is not UTF-8: {error}", path.display())))?;
    let digest = boost_source::content_digest_of(text.as_bytes());
    let trimmed = text.trim_start();
    let Some(first) = trimmed.chars().next()
    else {
        return Err(invalid(format!("`{}` is empty; expected a KiCad PCB or IPC-2581 document", path.display())));
    };
    let (board, losses, format) = if first == '<' {
        let imported = ipc2581_import::parse(&text, format!("ipc2581-{digest:016x}"))?;
        (imported.board, imported.losses, "ipc2581")
    }
    else if first == '(' {
        let imported = kicad_import::parse(&text, format!("kicad-{digest:016x}"))?;
        (imported.board, imported.losses, "kicad_pcb")
    }
    else {
        return Err(invalid(format!(
            "`{}` is neither a KiCad PCB nor an IPC-2581 document; it starts with `{first}`",
            path.display()
        )));
    };
    board::validate(&board)?;
    let handle = store::insert(board);
    let summary = store::summary_of(handle)?;
    let mut loss_codes: Vec<String> = Vec::new();
    for item in &losses {
        if !loss_codes.iter().any(|code| code == item.code) {
            loss_codes.push(item.code.to_string());
        }
    }
    let losses: Vec<Value> = losses.iter().map(|item| json!({ "code": item.code, "message": item.message })).collect();
    Ok(json!({
        "handle": handle,
        "format": format,
        "path": path.display().to_string(),
        "bytes": bytes,
        "revision": summary["revision"],
        "editable": true,
        "document": summary,
        "losses": losses,
        "lossCodes": loss_codes,
    }))
}

/// Writes one open board to a native document.
pub fn save(handle: u64, path: &Path, overwrite: bool) -> Result<Value, ServiceError> {
    let current = store::read(handle, |board| board.clone())?;
    board::validate(&current)?;
    if path.exists() && !overwrite {
        return Err(invalid(format!("`{}` already exists; pass overwrite to replace it", path.display())));
    }
    let text = serde_json::to_string_pretty(&Document::new(current.clone()))
        .map_err(|error| invalid(format!("cannot serialize the board: {error}")))?;
    let bytes = atomic_write(path, text.as_bytes())?;
    Ok(json!({
        "path": path.display().to_string(),
        "revision": current.revision,
        "bytes": bytes,
    }))
}

/// Runs the requested rule checks on one open board.
pub fn inspect(handle: u64, checks: Option<Vec<String>>) -> Result<Value, ServiceError> {
    store::read(handle, |board| inspect::run(board, checks))?
}

/// Reads the summary of one open board.
pub fn summary(handle: u64) -> Result<Value, ServiceError> {
    store::summary_of(handle)
}

/// Projects one open board to an exchange target.
pub fn export(handle: u64, format: &str, path: &Path) -> Result<Value, ServiceError> {
    store::read(handle, |board| export::run(board, format, path))?
}

/// Renders one open board for a preview surface without writing a file.
///
/// Only `svg` has a preview renderer. `png` stays declared but unimplemented,
/// so it rejects with `capability/not-implemented` and points at the export
/// path. `pdf` is available through [`export`] as a vector document.
pub fn render(handle: u64, format: &str) -> Result<Value, ServiceError> {
    match format {
        "svg" => store::read(handle, |board| {
            let (svg, width, height) = export::svg_preview(board)?;
            Ok(json!({
                "revision": board.revision,
                "format": "svg",
                "svg": svg,
                "width": width,
                "height": height,
            }))
        })?,
        other => Err(ServiceError::new(
            "capability/not-implemented",
            format!("`{other}` preview rendering is not implemented; use `export` to write `{other}`"),
        )),
    }
}

/// Closes one open board and releases its handle.
pub fn close(handle: u64) -> Result<(), ServiceError> {
    store::remove(handle)
}

/// Restores the previous snapshot of one open board.
pub fn undo(handle: u64) -> Result<Value, ServiceError> {
    store::undo(handle)?;
    edit_result(handle, None, None)
}

/// Reapplies the next snapshot of one open board.
pub fn redo(handle: u64) -> Result<Value, ServiceError> {
    store::redo(handle)?;
    edit_result(handle, None, None)
}

/// Adds one footprint to an open board.
pub fn add_footprint(handle: u64, request: AddFootprintRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_footprint(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one footprint from an open board.
pub fn delete_footprint(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_footprint(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Moves one footprint on an open board.
pub fn move_footprint(handle: u64, request: MoveFootprintRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.move_footprint(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Updates the value or package of one footprint.
pub fn update_footprint(handle: u64, request: UpdateFootprintRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_footprint(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Adds one pad to a footprint on an open board.
pub fn add_pad(handle: u64, request: AddPadRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_pad(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Updates the geometry or net of one pad on an open board.
pub fn update_pad(handle: u64, request: UpdatePadRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_pad(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one pad from a footprint and returns its number.
pub fn delete_pad(handle: u64, request: DeletePadRequest) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_pad(request)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Adds one track to an open board.
pub fn add_track(handle: u64, request: AddTrackRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_track(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Updates the net, layer, width or endpoints of one track on an open board.
pub fn update_track(handle: u64, request: UpdateTrackRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_track(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Splits one track at a point on its centreline.
///
/// The original track keeps its id as the first segment and a new `track-N`
/// becomes the second, so the split is one revision and one undo step. The
/// response carries both segments in `first` and `second` alongside the
/// committed revision and the undo/redo flags.
pub fn split_track(handle: u64, request: SplitTrackRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.split_track(request)?;
        Ok(())
    })?;
    let summary = store::summary_of(handle)?;
    Ok(json!({
        "handle": handle,
        "revision": summary["revision"],
        "canUndo": summary["canUndo"],
        "canRedo": summary["canRedo"],
        "board": summary,
        "first": object["first"],
        "second": object["second"],
    }))
}

/// Adds one via to an open board.
pub fn add_via(handle: u64, request: AddViaRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_via(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Updates the net, position, diameter or drill of one via on an open board.
pub fn update_via(handle: u64, request: UpdateViaRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_via(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Declares one net class on an open board.
pub fn add_net_class(handle: u64, request: AddNetClassRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_net_class(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one net class from an open board.
pub fn delete_net_class(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_net_class(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Assigns one declared net to a net class on an open board.
pub fn assign_net_class(handle: u64, request: AssignNetClassRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.assign_net_class(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Declares one test point on an existing pad or via.
pub fn add_test_point(handle: u64, request: AddTestPointRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_test_point(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one declared test point from an open board.
pub fn delete_test_point(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_test_point(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Adds one copper pour region to an open board.
pub fn add_zone(handle: u64, request: AddZoneRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_zone(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Updates the layer, net or polygon of one copper pour region.
pub fn update_zone(handle: u64, request: UpdateZoneRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_zone(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one copper pour region from an open board.
pub fn delete_zone(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_zone(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Adds one silkscreen text item to an open board.
pub fn add_text(handle: u64, request: AddTextRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_text(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Updates the content, layer or metrics of one silkscreen text item.
pub fn update_text(handle: u64, request: UpdateTextRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_text(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one silkscreen text item and returns its id.
pub fn delete_text(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_text(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Lists every silkscreen text item of one open board.
pub fn list_texts(handle: u64) -> Result<Value, ServiceError> {
    store::read(handle, |board| {
        Ok(json!({
            "revision": board.revision,
            "units": board.units,
            "count": board.texts.len(),
            "items": board.texts.iter().map(board::TextItem::to_value).collect::<Vec<_>>(),
        }))
    })?
}

/// Adds one copper keepout region to an open board.
pub fn add_keepout(handle: u64, request: AddKeepoutRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_keepout(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Updates the layer or polygon of one keepout region.
pub fn update_keepout(handle: u64, request: UpdateKeepoutRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_keepout(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one keepout region from an open board.
pub fn delete_keepout(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_keepout(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Reports the area and perimeter of every copper keepout region.
pub fn keepout_report(handle: u64) -> Result<Value, ServiceError> {
    store::read(handle, |board| board.keepout_report())
}

/// Adds linear teardrop copper at every pad-track junction.
pub fn add_teardrops(handle: u64, request: AddTeardropsRequest) -> Result<Value, ServiceError> {
    let mut report = Value::Null;
    store::mutate(handle, |board| {
        report = board.add_teardrops(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(report), None)
}

/// Sets the back-drill depth and remaining stub of one through via.
pub fn set_via_back_drill(handle: u64, request: SetViaBackDrillRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.set_via_back_drill(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Routes every net with at least two pads on an open board.
///
/// The router is deterministic: the same board and options always produce the
/// same copper. Edges it cannot place without a clearance or outline violation
/// are reported in `unrouted` instead of being approximated.
pub fn route(handle: u64, options: RouteOptions) -> Result<Value, ServiceError> {
    let mut report = Value::Null;
    store::mutate_if_changed(handle, |board| {
        let (value, changed) = route::run(board, options)?;
        report = value;
        Ok(changed)
    })?;
    Ok(report)
}

/// Deletes one track from an open board.
pub fn delete_track(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_track(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Generates a stitching via fence along the inset board outline.
pub fn stitch_vias(handle: u64, request: AddStitchingRequest) -> Result<Value, ServiceError> {
    let mut report = Value::Null;
    store::mutate(handle, |board| {
        report = board.stitch_vias(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(report), None)
}

/// Deletes one via from an open board.
pub fn delete_via(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_via(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Adds one plated routed slot to an open board.
///
/// A slot spans every copper layer and is written as a `G85` routed segment in
/// the Excellon drill file.
pub fn add_slot(handle: u64, request: AddSlotRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_slot(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Updates the net, endpoints or width of one plated routed slot.
pub fn update_slot(handle: u64, request: UpdateSlotRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_slot(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one slot from an open board.
pub fn delete_slot(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_slot(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Adds one layer to an open board.
pub fn add_layer(handle: u64, request: AddLayerRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_layer(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Renames or reconfigures one layer on an open board.
pub fn update_layer(handle: u64, request: UpdateLayerRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.update_layer(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Sets the rectangular outline of an open board.
pub fn set_outline(handle: u64, request: SetOutlineRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.set_outline(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Replaces one or more design-rule values on an open board.
///
/// Every value is in millimetres. At least one rule is required, and a
/// rejected request leaves the board and its revision untouched.
pub fn set_rules(handle: u64, request: SetRulesRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.set_rules(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Declares the physical stackup used by controlled-impedance calculations.
pub fn set_stackup(handle: u64, request: SetStackupRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.set_stackup(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Computes the single-ended or differential impedance of one trace from the
/// board stackup.
///
/// Request overrides replace the stored stackup per call, so a caller can test a
/// width before committing a stackup. The structure defaults to microstrip. A
/// `diffMicrostrip` or `diffStripline` request also needs an edge `gap` and
/// reports the odd, even and differential impedances.
pub fn stackup_impedance(handle: u64, request: ImpedanceRequest) -> Result<Value, ServiceError> {
    store::read(handle, |board| {
        let structure = impedance::Structure::parse(request.structure.as_deref())?;
        let stackup =
            board.resolved_stackup(request.copper_thickness, request.dielectric_height, request.dielectric_constant)?;
        let values = impedance::values(structure, request.width, request.gap, stackup)?;
        let mut report = json!({
            "revision": board.revision,
            "structure": structure.name(),
            "width": round6(request.width),
            "impedance": round6(values.impedance),
            "copperThickness": round6(stackup.copper_thickness),
            "dielectricHeight": round6(stackup.dielectric_height),
            "dielectricConstant": round6(stackup.dielectric_constant),
        });
        if let Some(odd) = values.odd {
            report["oddImpedance"] = json!(round6(odd));
        }
        if let Some(even) = values.even {
            report["evenImpedance"] = json!(round6(even));
        }
        if structure.differential()
            && let Some(gap) = request.gap
        {
            report["gap"] = json!(round6(gap));
        }
        Ok(report)
    })?
}

/// Converts a target impedance to the trace width that reaches it.
///
/// A differential structure solves for the width of each half at the requested
/// edge gap and reports the resulting odd and even impedances.
pub fn stackup_width(handle: u64, request: WidthRequest) -> Result<Value, ServiceError> {
    store::read(handle, |board| {
        let structure = impedance::Structure::parse(request.structure.as_deref())?;
        let stackup =
            board.resolved_stackup(request.copper_thickness, request.dielectric_height, request.dielectric_constant)?;
        let width = impedance::width(structure, request.impedance, request.gap, stackup)?;
        let values = impedance::values(structure, width, request.gap, stackup)?;
        let mut report = json!({
            "revision": board.revision,
            "structure": structure.name(),
            "impedance": round6(request.impedance),
            "width": round6(width),
            "copperThickness": round6(stackup.copper_thickness),
            "dielectricHeight": round6(stackup.dielectric_height),
            "dielectricConstant": round6(stackup.dielectric_constant),
        });
        if let Some(odd) = values.odd {
            report["oddImpedance"] = json!(round6(odd));
        }
        if let Some(even) = values.even {
            report["evenImpedance"] = json!(round6(even));
        }
        if structure.differential()
            && let Some(gap) = request.gap
        {
            report["gap"] = json!(round6(gap));
        }
        Ok(report)
    })?
}

/// Suggests the trace width for each target impedance on one stackup.
///
/// The table uses the same closed form as [`stackup_width`], then recomputes the
/// impedance of the suggested width so the row can be verified. Targets default
/// to `[50]`; up to 64 are accepted and each must be reachable. A differential
/// structure needs an edge `gap` and reports odd and even impedances per row.
pub fn impedance_table(handle: u64, request: ImpedanceTableRequest) -> Result<Value, ServiceError> {
    store::read(handle, |board| {
        let structure = impedance::Structure::parse(request.structure.as_deref())?;
        let stackup =
            board.resolved_stackup(request.copper_thickness, request.dielectric_height, request.dielectric_constant)?;
        let targets = request.targets.unwrap_or_else(|| vec![50.0]);
        if targets.is_empty() {
            return Err(argument("`targets` must list at least one impedance"));
        }
        if targets.len() > 64 {
            return Err(argument("`targets` accepts at most 64 impedances"));
        }
        for target in &targets {
            if !target.is_finite() || *target <= 0.0 {
                return Err(argument("`targets` must list positive, finite impedances"));
            }
        }
        let mut rows = Vec::with_capacity(targets.len());
        for target in targets {
            let width = impedance::width(structure, target, request.gap, stackup)?;
            let values = impedance::values(structure, width, request.gap, stackup)?;
            let mut row = json!({
                "targetImpedance": round6(target),
                "width": round6(width),
                "impedance": round6(values.impedance),
            });
            if let Some(odd) = values.odd {
                row["oddImpedance"] = json!(round6(odd));
            }
            if let Some(even) = values.even {
                row["evenImpedance"] = json!(round6(even));
            }
            rows.push(row);
        }
        let mut report = json!({
            "revision": board.revision,
            "units": "mm",
            "structure": structure.name(),
            "targets": rows.len(),
            "copperThickness": round6(stackup.copper_thickness),
            "dielectricHeight": round6(stackup.dielectric_height),
            "dielectricConstant": round6(stackup.dielectric_constant),
            "rows": rows,
        });
        if structure.differential()
            && let Some(gap) = request.gap
        {
            report["gap"] = json!(round6(gap));
        }
        Ok(report)
    })?
}

/// Declares one net on an open board.
pub fn add_net(handle: u64, request: AddNetRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_net(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Declares one differential pair on an open board.
pub fn add_differential_pair(handle: u64, request: AddDifferentialPairRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_differential_pair(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Deletes one differential pair from an open board.
pub fn delete_differential_pair(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_differential_pair(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Declares one waiver for a finding code on an open board.
pub fn add_waiver(handle: u64, request: AddWaiverRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.add_waiver(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Removes one waiver from an open board.
pub fn delete_waiver(handle: u64, id: &str) -> Result<Value, ServiceError> {
    let mut deleted = String::new();
    store::mutate(handle, |board| {
        deleted = board.delete_waiver(id)?;
        Ok(())
    })?;
    edit_result(handle, None, Some(deleted))
}

/// Overrides the severity of one finding code on an open board.
pub fn set_severity(handle: u64, request: SetSeverityRequest) -> Result<Value, ServiceError> {
    let mut object = Value::Null;
    store::mutate(handle, |board| {
        object = board.set_severity(request)?;
        Ok(())
    })?;
    edit_result(handle, Some(object), None)
}

/// Removes one severity override from an open board.
pub fn delete_severity(handle: u64, code: &str) -> Result<Value, ServiceError> {
    let mut removed = Value::Null;
    store::mutate(handle, |board| {
        removed = board.delete_severity(code)?;
        Ok(())
    })?;
    edit_result(handle, Some(removed), None)
}

/// Reports copper area per layer and the coverage imbalance across them.
///
/// Feature areas are summed, so overlapping copper is counted once per feature.
/// `boardArea` and the coverage fields are present only when the board has a
/// rectangular outline.
pub fn copper_balance(handle: u64) -> Result<Value, ServiceError> {
    store::read(handle, copper::report)
}

/// Reports the area and perimeter of every copper pour region.
pub fn zone_report(handle: u64) -> Result<Value, ServiceError> {
    store::read(handle, copper::zones)
}

/// Reports routed copper length per declared net.
///
/// Nets are listed in document order. Each entry carries the track count, the
/// summed centreline length in document units and the via count; a declared net
/// without copper reports zeros. Tracks are straight segments, so the summed
/// length is exact, and copper without a declared net is not listed.
pub fn net_lengths(handle: u64) -> Result<Value, ServiceError> {
    store::read(handle, copper::net_lengths)
}

/// Reports the drill tools, holes and slots of one open board.
///
/// Every via and plated routed slot is grouped by finished tool diameter in
/// millimetres, so the tool table matches the Excellon writer without reading
/// the file.
pub fn drill_report(handle: u64) -> Result<Value, ServiceError> {
    store::read(handle, drill::report)
}

/// Builds the panelization report of one open board.
///
/// The panel repeats the rectangular outline across `columns` and `rows` with
/// the requested gaps and edge rail. The report carries the panel outline,
/// every placement, the area utilization and the seam or tab placeholders of
/// the requested style.
pub fn panelize(handle: u64, request: PanelizeRequest) -> Result<Value, ServiceError> {
    store::read(handle, |board| panel::report(board, request))?
}

/// Reports the geometry of a polygonal board outline.
///
/// A requested polygon must have at least three distinct vertices, enclose an
/// area and not self-intersect. The report gives the area and perimeter in
/// millimetres and, when `offset` is set, the outward or inward offset polygon.
/// An offset that empties, self-intersects or turns the outline inside out is
/// rejected. Arc segments are not accepted.
pub fn outline_geometry(handle: u64, request: OutlineGeometryRequest) -> Result<Value, ServiceError> {
    store::read(handle, |board| outline::report(board, request))?
}

/// Lists one open board as a summary.
pub fn list_board(handle: u64) -> Result<Value, ServiceError> {
    store::summary_of(handle)
}

fn locate(handle: u64) -> Result<Value, ServiceError> {
    let summary = store::summary_of(handle)?;
    Ok(json!({ "handle": handle, "board": summary }))
}

fn edit_result(handle: u64, object: Option<Value>, deleted: Option<String>) -> Result<Value, ServiceError> {
    let summary = store::summary_of(handle)?;
    let mut value = json!({ "handle": handle, "revision": summary["revision"], "board": summary });
    if let Some(object) = object {
        value["object"] = object;
    }
    if let Some(deleted) = deleted {
        value["deleted"] = Value::String(deleted);
    }
    Ok(value)
}

/// Rounds a computed value for a readable API report without a negative zero.
pub(crate) fn round6(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let value = (value * 1_000_000.0).round() / 1_000_000.0;
    if value == 0.0 { 0.0 } else { value }
}

/// Writes bytes through a temporary sibling and renames the result into place.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<u64, ServiceError> {
    let parent = path.parent().filter(|parent| !parent.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err(invalid(format!("`{}` has no existing parent directory", path.display())));
    }
    let target =
        if path.parent().is_some_and(|parent| !parent.as_os_str().is_empty()) { path.to_path_buf() } else { parent.join(path) };
    boost_session::save_atomic(&target, bytes)
        .map_err(|error| invalid(format!("cannot write `{}`: {error}", target.display())))?;
    Ok(bytes.len() as u64)
}
