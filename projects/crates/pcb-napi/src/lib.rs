//! Thin Node-API surface for OpenPCB.
//!
//! Every export performs one orthogonal operation and returns a JSON string, so
//! the TypeScript layer owns argument parsing, help text and exit codes. Board
//! semantics stay in `open-pcb-model`; this crate only maps values and owns the
//! WebView host.
//!
//! The same operations answer the studio's `board.*` JSON-RPC messages, so the
//! window and the command line cannot drift into different board models.

#![deny(missing_docs)]

use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use open_pcb_model::{CreateRequest, ServiceError};
use serde_json::{Value, json};
use wae::{
    CallContext, Error, JsValue, Result, function, optional_string_prop, register_module,
    webview::{DragRect, IpcHandler, IpcOutcome, IpcSink, OpenOptions, WebViewError, WebViewHost},
};

fn fail(error: ServiceError) -> Error {
    Error::new(format!("{}: {}", error.code, error.message))
}

fn argument(ctx: &CallContext, index: usize) -> Result<JsValue> {
    ctx.args().get(index).copied().ok_or_else(|| Error::new("session/argument-invalid: a required argument is missing"))
}

fn text(ctx: &CallContext, index: usize) -> Result<String> {
    argument(ctx, index)?.as_string()
}

fn handle(ctx: &CallContext, index: usize) -> Result<u64> {
    let raw = argument(ctx, index)?.as_i64()?;
    u64::try_from(raw).map_err(|_| Error::new("session/argument-invalid: the board handle must be a non-negative integer"))
}

fn respond(ctx: &CallContext, value: &Value) -> Result<JsValue> {
    let text = value.to_string();
    ctx.env().string(&text)
}

/// Library version of the native binding.
#[function]
fn version(ctx: CallContext) -> Result<JsValue> {
    ctx.env().string(env!("CARGO_PKG_VERSION"))
}

/// Reports the declared board capability matrix.
///
/// The matrix comes from the same model crate the CLI reads, so neither entry
/// point can report different capabilities.
#[function]
fn capabilities(ctx: CallContext) -> Result<JsValue> {
    let matrix = open_pcb_model::capabilities().to_string();
    ctx.env().string(&matrix)
}

/// Creates a board from a JSON request.
#[function]
fn board_create(ctx: CallContext) -> Result<JsValue> {
    let raw = text(&ctx, 0)?;
    let request: Value =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let created = open_pcb_model::create(CreateRequest {
        title: request["title"].as_str().map(str::to_string),
        template: request["template"].as_str().map(str::to_string),
    })
    .map_err(fail)?;
    respond(&ctx, &created)
}

/// Opens a native board document from a path.
#[function]
fn board_open(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let opened = open_pcb_model::open(Path::new(&path)).map_err(fail)?;
    respond(&ctx, &opened)
}

/// Imports one KiCad PCB or IPC-2581 file and returns the handle plus its loss ledger.
#[function]
fn board_import(ctx: CallContext) -> Result<JsValue> {
    let path = text(&ctx, 0)?;
    let report = open_pcb_model::import(Path::new(&path)).map_err(fail)?;
    respond(&ctx, &report)
}

/// Writes one open board to a native document path.
#[function]
fn board_save(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let path = text(&ctx, 1)?;
    let overwrite = ctx.args().get(2).map(|value| value.as_bool()).transpose()?.unwrap_or(false);
    let report = open_pcb_model::save(handle, Path::new(&path), overwrite).map_err(fail)?;
    respond(&ctx, &report)
}

/// Runs the requested rule checks on one open board.
#[function]
fn board_inspect(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let checks = match ctx.args().get(1) {
        Some(value) => {
            let raw = value.as_string()?;
            let names: Vec<String> =
                serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
            Some(names)
        }
        None => None,
    };
    let report = open_pcb_model::inspect(handle, checks).map_err(fail)?;
    respond(&ctx, &report)
}

/// Projects one open board to an exchange target.
#[function]
fn board_export(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let format = text(&ctx, 1)?;
    let path = text(&ctx, 2)?;
    let report = open_pcb_model::export(handle, &format, Path::new(&path)).map_err(fail)?;
    respond(&ctx, &report)
}

/// Closes one open board and releases its handle.
#[function]
fn board_close(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    open_pcb_model::close(handle).map_err(fail)?;
    ctx.env().boolean(true)
}

/// Adds one footprint to an open board.
#[function]
fn board_add_footprint(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::AddFootprintRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::add_footprint(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Deletes one footprint from an open board.
#[function]
fn board_delete_footprint(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let id = text(&ctx, 1)?;
    let value = open_pcb_model::delete_footprint(handle, &id).map_err(fail)?;
    respond(&ctx, &value)
}

/// Adds one track to an open board.
#[function]
fn board_add_track(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::AddTrackRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::add_track(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Adds one via to an open board.
#[function]
fn board_add_via(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::AddViaRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::add_via(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Adds linear teardrop copper at every pad-track junction.
#[function]
fn board_add_teardrops(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::AddTeardropsRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::add_teardrops(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Sets the back-drill depth and remaining stub of one through via.
#[function]
fn board_set_via_back_drill(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::SetViaBackDrillRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::set_via_back_drill(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Routes every net that has at least two net-assigned pads.
#[function]
fn board_route(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let options: open_pcb_model::RouteOptions =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::route(handle, options).map_err(fail)?;
    respond(&ctx, &value)
}

/// Declares one net on an open board.
#[function]
fn board_add_net(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::AddNetRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::add_net(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Declares one differential pair on an open board.
#[function]
fn board_add_differential_pair(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::AddDifferentialPairRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::add_differential_pair(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Deletes one differential pair from an open board.
#[function]
fn board_delete_differential_pair(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let id = text(&ctx, 1)?;
    let value = open_pcb_model::delete_differential_pair(handle, &id).map_err(fail)?;
    respond(&ctx, &value)
}

/// Restores the previous snapshot of one open board.
#[function]
fn board_undo(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let value = open_pcb_model::undo(handle).map_err(fail)?;
    respond(&ctx, &value)
}

/// Reapplies the next snapshot of one open board.
#[function]
fn board_redo(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let value = open_pcb_model::redo(handle).map_err(fail)?;
    respond(&ctx, &value)
}

/// Moves one footprint on an open board.
#[function]
fn board_move_footprint(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::MoveFootprintRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::move_footprint(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Updates the value or package of one footprint.
#[function]
fn board_update_footprint(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::UpdateFootprintRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::update_footprint(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Deletes one track from an open board.
#[function]
fn board_delete_track(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let id = text(&ctx, 1)?;
    let value = open_pcb_model::delete_track(handle, &id).map_err(fail)?;
    respond(&ctx, &value)
}

/// Deletes one via from an open board.
#[function]
fn board_delete_via(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let id = text(&ctx, 1)?;
    let value = open_pcb_model::delete_via(handle, &id).map_err(fail)?;
    respond(&ctx, &value)
}

/// Adds one layer to an open board.
#[function]
fn board_add_layer(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::AddLayerRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::add_layer(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Renames or reconfigures one layer.
#[function]
fn board_update_layer(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::UpdateLayerRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::update_layer(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Sets the rectangular outline of an open board.
#[function]
fn board_set_outline(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::SetOutlineRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::set_outline(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Replaces one or more design-rule values on an open board.
#[function]
fn board_set_rules(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let raw = text(&ctx, 1)?;
    let request: open_pcb_model::SetRulesRequest =
        serde_json::from_str(&raw).map_err(|error| Error::new(format!("session/argument-invalid: {error}")))?;
    let value = open_pcb_model::set_rules(handle, request).map_err(fail)?;
    respond(&ctx, &value)
}

/// Lists one open board as a summary.
#[function]
fn board_list(ctx: CallContext) -> Result<JsValue> {
    let handle = handle(&ctx, 0)?;
    let value = open_pcb_model::list_board(handle).map_err(fail)?;
    respond(&ctx, &value)
}

// One WebView host per process, owned by the thread that opened it.
thread_local! {
    static GUI: RefCell<Option<GuiState>> = const { RefCell::new(None) };
}

const GUI_CLOSED: &str = "gui_closed";
const GUI_HOST_EXITED: &str = "gui_host_exited";
const GUI_NOT_OPEN: &str = "gui_not_open";

/// Drag rectangles the page reported for its self-drawn title bar.
static DRAG_REGIONS: Mutex<Option<Vec<DragRect>>> = Mutex::new(None);
/// Window chrome commands queued by the IPC worker for the window thread.
static WINDOW_COMMANDS: Mutex<Vec<WindowCommand>> = Mutex::new(Vec::new());

/// A window operation that must run on the thread that owns the host.
#[derive(Clone, Copy)]
enum WindowCommand {
    Minimize,
    ToggleMaximize,
}

struct GuiState {
    host: WebViewHost,
    backend: &'static str,
    opened_at: Instant,
    /// Set when the page asks the host to close, so a dead host is
    /// distinguishable from one that never processed the request.
    close_requested: Arc<AtomicBool>,
}

struct BoardIpcHandler {
    close_requested: Arc<AtomicBool>,
}

impl IpcHandler for BoardIpcHandler {
    fn on_ipc(&mut self, message: &str, _sink: &IpcSink) -> IpcOutcome {
        let Ok(value) = serde_json::from_str::<Value>(message)
        else {
            return IpcOutcome::Ignored;
        };
        let ty = value.get("type").and_then(Value::as_str).unwrap_or("");
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        match ty {
            "close" => {
                self.close_requested.store(true, Ordering::SeqCst);
                IpcOutcome::CloseWindow
            }
            "window.close" => {
                self.close_requested.store(true, Ordering::SeqCst);
                IpcOutcome::CloseWindow
            }
            "window.dragRegions" => {
                let rects = parse_drag_rects(&value);
                let count = rects.len();
                if let Ok(mut pending) = DRAG_REGIONS.lock() {
                    *pending = Some(rects);
                }
                reply(id, "windowDragRegionsResult", json!({ "count": count }))
            }
            "window.minimize" => {
                queue_window_command(WindowCommand::Minimize);
                reply(id, "windowMinimizeResult", json!({ "queued": true }))
            }
            "window.toggleMaximize" => {
                queue_window_command(WindowCommand::ToggleMaximize);
                reply(id, "windowToggleMaximizeResult", json!({ "queued": true }))
            }
            "ready" => reply(
                id,
                "readyResult",
                json!({
                    "version": env!("CARGO_PKG_VERSION"),
                    "platform": platform_name(),
                    "window": { "customChrome": cfg!(target_os = "windows") },
                }),
            ),
            "capabilities" => reply(id, "capabilitiesResult", json!({ "capabilities": open_pcb_model::capabilities() })),
            other if other.starts_with("board.") => match board_rpc(other, &value) {
                Ok(result) => reply(id, "boardResult", result),
                Err(error) => failure(id, "error", format!("{}: {}", error.code, error.message)),
            },
            _ => IpcOutcome::Ignored,
        }
    }
}

/// Parses the page's `rects` array into host drag rectangles.
fn parse_drag_rects(value: &Value) -> Vec<DragRect> {
    value.get("rects").and_then(Value::as_array).map(|items| items.iter().filter_map(drag_rect).collect()).unwrap_or_default()
}

fn drag_rect(item: &Value) -> Option<DragRect> {
    Some(DragRect {
        x: i32::try_from(item.get("x")?.as_i64()?).ok()?,
        y: i32::try_from(item.get("y")?.as_i64()?).ok()?,
        width: i32::try_from(item.get("width")?.as_i64()?).ok()?,
        height: i32::try_from(item.get("height")?.as_i64()?).ok()?,
    })
}

fn queue_window_command(command: WindowCommand) {
    if let Ok(mut queue) = WINDOW_COMMANDS.lock() {
        queue.push(command);
    }
}

/// Applies drag regions and window commands queued by the IPC worker.
fn apply_window_requests(host: &mut WebViewHost) {
    if let Ok(mut pending) = DRAG_REGIONS.lock()
        && let Some(rects) = pending.take()
    {
        let _ = host.set_drag_regions(&rects);
    }
    if let Ok(mut queue) = WINDOW_COMMANDS.lock() {
        for command in std::mem::take(&mut *queue) {
            let _ = match command {
                WindowCommand::Minimize => host.minimize(),
                WindowCommand::ToggleMaximize => host.toggle_maximize(),
            };
        }
    }
}

/// Maps one studio `board.*` request onto the model crate.
fn board_rpc(kind: &str, value: &Value) -> std::result::Result<Value, ServiceError> {
    match kind {
        "board.create" => {
            let request = value.get("request").cloned().unwrap_or_else(|| value.clone());
            open_pcb_model::create(CreateRequest {
                title: request.get("title").and_then(Value::as_str).map(str::to_string),
                template: request.get("template").and_then(Value::as_str).map(str::to_string),
            })
        }
        "board.open" => {
            let path = rpc_text(value, "path")?;
            open_pcb_model::open(Path::new(path))
        }
        "board.import" => {
            let path = rpc_text(value, "path")?;
            open_pcb_model::import(Path::new(path))
        }
        "board.save" => {
            let handle = rpc_handle(value)?;
            let path = rpc_text(value, "path")?;
            let overwrite = value.get("overwrite").and_then(Value::as_bool).unwrap_or(false);
            let report = open_pcb_model::save(handle, Path::new(path), overwrite)?;
            Ok(json!({ "report": report }))
        }
        "board.inspect" => {
            let handle = rpc_handle(value)?;
            let checks = value
                .get("checks")
                .and_then(Value::as_array)
                .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect::<Vec<String>>());
            let report = open_pcb_model::inspect(handle, checks)?;
            Ok(json!({ "report": report }))
        }
        "board.export" => {
            let handle = rpc_handle(value)?;
            let format = rpc_text(value, "format")?;
            let target = rpc_text(value, "target").or_else(|_| rpc_text(value, "path"))?;
            let report = open_pcb_model::export(handle, format, Path::new(target))?;
            Ok(json!({ "report": report }))
        }
        "board.close" => {
            let handle = rpc_handle(value)?;
            open_pcb_model::close(handle)?;
            Ok(json!({ "closed": true }))
        }
        "board.addFootprint" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::AddFootprintRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::add_footprint(handle, request)
        }
        "board.deleteFootprint" => {
            let handle = rpc_handle(value)?;
            let id = rpc_text(value, "id")?;
            open_pcb_model::delete_footprint(handle, id)
        }
        "board.addTrack" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::AddTrackRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::add_track(handle, request)
        }
        "board.addVia" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::AddViaRequest = serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::add_via(handle, request)
        }
        "board.addNet" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::AddNetRequest = serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::add_net(handle, request)
        }
        "board.route" => {
            let handle = rpc_handle(value)?;
            let raw = value.get("options").or_else(|| value.get("request")).cloned().unwrap_or_else(|| json!({}));
            let options: open_pcb_model::RouteOptions = serde_json::from_value(raw).map_err(argument_error)?;
            open_pcb_model::route(handle, options)
        }
        "board.undo" => {
            let handle = rpc_handle(value)?;
            open_pcb_model::undo(handle)
        }
        "board.redo" => {
            let handle = rpc_handle(value)?;
            open_pcb_model::redo(handle)
        }
        "board.addLayer" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::AddLayerRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::add_layer(handle, request)
        }
        "board.updateLayer" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::UpdateLayerRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::update_layer(handle, request)
        }
        "board.moveFootprint" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::MoveFootprintRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::move_footprint(handle, request)
        }
        "board.updateFootprint" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::UpdateFootprintRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::update_footprint(handle, request)
        }
        "board.deleteTrack" => {
            let handle = rpc_handle(value)?;
            let id = rpc_text(value, "id")?;
            open_pcb_model::delete_track(handle, id)
        }
        "board.deleteVia" => {
            let handle = rpc_handle(value)?;
            let id = rpc_text(value, "id")?;
            open_pcb_model::delete_via(handle, id)
        }
        "board.addTeardrops" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::AddTeardropsRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::add_teardrops(handle, request)
        }
        "board.setViaBackDrill" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::SetViaBackDrillRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::set_via_back_drill(handle, request)
        }
        "board.addDifferentialPair" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::AddDifferentialPairRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::add_differential_pair(handle, request)
        }
        "board.deleteDifferentialPair" => {
            let handle = rpc_handle(value)?;
            let id = rpc_text(value, "id")?;
            open_pcb_model::delete_differential_pair(handle, id)
        }
        "board.setOutline" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::SetOutlineRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::set_outline(handle, request)
        }
        "board.setRules" => {
            let handle = rpc_handle(value)?;
            let request: open_pcb_model::SetRulesRequest =
                serde_json::from_value(rpc_request(value)).map_err(argument_error)?;
            open_pcb_model::set_rules(handle, request)
        }
        "board.list" => {
            let handle = rpc_handle(value)?;
            let summary = open_pcb_model::list_board(handle)?;
            Ok(json!({ "board": summary }))
        }
        other => {
            Err(ServiceError::new("capability/not-implemented", format!("`{other}` is not a board request this build answers")))
        }
    }
}

fn rpc_handle(value: &Value) -> std::result::Result<u64, ServiceError> {
    value
        .get("handle")
        .and_then(Value::as_u64)
        .ok_or_else(|| ServiceError::new("session/argument-invalid", "`handle` must be a non-negative integer"))
}

fn rpc_text<'a>(value: &'a Value, key: &str) -> std::result::Result<&'a str, ServiceError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ServiceError::new("session/argument-invalid", format!("`{key}` must be a string")))
}

fn rpc_request(value: &Value) -> Value {
    value.get("request").cloned().unwrap_or_else(|| json!({}))
}

fn argument_error(error: serde_json::Error) -> ServiceError {
    ServiceError::new("session/argument-invalid", error.to_string())
}

fn reply(id: Value, kind: &str, payload: Value) -> IpcOutcome {
    let mut body = payload;
    body["type"] = Value::String(kind.to_string());
    body["id"] = id;
    IpcOutcome::ReplyToPage(body.to_string())
}

fn failure(id: Value, kind: &str, message: String) -> IpcOutcome {
    reply(id, kind, json!({ "message": message }))
}

fn platform_name() -> &'static str {
    if cfg!(windows) {
        "win32"
    }
    else if cfg!(target_os = "macos") {
        "darwin"
    }
    else {
        "linux"
    }
}

fn gui_status(
    state: &'static str,
    code: Option<&'static str>,
    backend: Option<&'static str>,
    uptime_ms: u64,
    message: Option<String>,
) -> Value {
    json!({
        "state": state,
        "code": code,
        "backend": backend,
        "platform": platform_name(),
        "uptimeMs": uptime_ms,
        "message": message,
    })
}

fn gui_open_result(ok: bool, status: Value, error: Option<&WebViewError>) -> Value {
    let (stage, hresult, attempted) = match error {
        Some(error) => {
            let attempted = match error {
                WebViewError::Host(host) => host.attempted.clone(),
                _ => Vec::new(),
            };
            (error.stage(), error.hresult(), attempted)
        }
        None => (None, None, Vec::new()),
    };
    json!({
        "ok": ok,
        "status": status,
        "stage": stage,
        "hresult": hresult,
        "attempted": attempted,
    })
}

/// Resolves the GUI content directory: explicit override, then the built tree.
fn resolve_gui_dir() -> Option<PathBuf> {
    if let Ok(value) = std::env::var("OPEN_PCB_GUI") {
        let path = PathBuf::from(value);
        if path.is_dir() {
            return Some(path);
        }
        if path.is_file() {
            return path.parent().map(Path::to_path_buf);
        }
    }

    let manifest_relative = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("projects")
        .join("packages")
        .join("open-pcb")
        .join("dist")
        .join("gui");
    if manifest_relative.join("index.html").is_file() {
        return Some(manifest_relative);
    }

    if let Ok(cwd) = std::env::current_dir() {
        for candidate in [cwd.join("projects/packages/open-pcb/dist/gui"), cwd.join("dist/gui"), cwd.join("gui")] {
            if candidate.join("index.html").is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Opens the OpenPCB Studio window, or focuses the live one.
#[function(js_name = "openGui")]
fn open_gui(ctx: CallContext) -> Result<JsValue> {
    let mut title = "OpenPCB Studio".to_string();
    let mut content_dir: Option<String> = None;
    let mut width = 1280i32;
    let mut height = 820i32;

    if let Some(arg) = ctx.args().first().copied()
        && !arg.is_undefined()?
    {
        if let Some(value) = optional_string_prop(arg, "title")? {
            title = value;
        }
        if let Some(value) = optional_string_prop(arg, "contentDir")? {
            content_dir = Some(value);
        }
        if let Ok(value) = arg.get_named("width")
            && let Ok(number) = value.as_i64()
        {
            width = number as i32;
        }
        if let Ok(value) = arg.get_named("height")
            && let Ok(number) = value.as_i64()
        {
            height = number as i32;
        }
    }

    let live = GUI.with(|slot| {
        let guard = slot.borrow();
        guard
            .as_ref()
            .filter(|state| state.host.is_alive())
            .map(|state| (state.backend, state.opened_at.elapsed().as_millis() as u64))
    });
    if let Some((backend, uptime_ms)) = live {
        GUI.with(|slot| {
            if let Some(state) = slot.borrow_mut().as_mut() {
                let _ = state.host.focus();
            }
        });
        let status = gui_status("open", None, Some(backend), uptime_ms, None);
        return respond(&ctx, &gui_open_result(true, status, None));
    }
    GUI.with(|slot| {
        *slot.borrow_mut() = None;
    });

    let dir = match content_dir {
        Some(dir) => PathBuf::from(dir),
        None => match resolve_gui_dir() {
            Some(dir) => dir,
            None => {
                let status = gui_status(
                    "failed",
                    Some("gui_content_missing"),
                    None,
                    0,
                    Some("the WebView content directory is missing; build the GUI first".to_string()),
                );
                return respond(&ctx, &gui_open_result(false, status, None));
            }
        },
    };

    let close_requested = Arc::new(AtomicBool::new(false));
    let mut options = OpenOptions::from_content_dir(dir.display().to_string(), title);
    options.width = width;
    options.height = height;
    options.accept_file_drop = true;
    options.decorations = !cfg!(target_os = "windows");

    match WebViewHost::open(options, Box::new(BoardIpcHandler { close_requested: Arc::clone(&close_requested) })) {
        Ok(host) => {
            let backend = host.backend_name();
            GUI.with(|slot| {
                *slot.borrow_mut() = Some(GuiState { host, backend, opened_at: Instant::now(), close_requested });
            });
            let status = gui_status("open", None, Some(backend), 0, None);
            respond(&ctx, &gui_open_result(true, status, None))
        }
        Err(error) => {
            let status = gui_status("failed", error.code(), None, 0, Some(error.to_string()));
            respond(&ctx, &gui_open_result(false, status, Some(&error)))
        }
    }
}

/// Pumps the host message loop once and reports the observed lifecycle state.
#[function(js_name = "pumpEvents")]
fn pump_events(ctx: CallContext) -> Result<JsValue> {
    let status = GUI.with(|slot| {
        let mut guard = slot.borrow_mut();
        let Some(state) = guard.as_mut()
        else {
            return gui_status("closed", Some(GUI_NOT_OPEN), None, 0, Some("no OpenPCB window is open".to_string()));
        };
        let backend = state.backend;
        let uptime_ms = state.opened_at.elapsed().as_millis() as u64;
        apply_window_requests(&mut state.host);
        if state.host.pump() {
            return gui_status("open", None, Some(backend), uptime_ms, None);
        }
        let requested = state.close_requested.swap(false, Ordering::SeqCst);
        *guard = None;
        if requested {
            gui_status("closed", Some(GUI_CLOSED), Some(backend), uptime_ms, None)
        }
        else {
            gui_status(
                "closed",
                Some(GUI_HOST_EXITED),
                Some(backend),
                uptime_ms,
                Some(format!("the {backend} webview host stopped without a page close request")),
            )
        }
    });
    respond(&ctx, &status)
}

/// Closes the OpenPCB window; the result distinguishes "already closed".
#[function(js_name = "closeGui")]
fn close_gui(ctx: CallContext) -> Result<JsValue> {
    let result = GUI.with(|slot| {
        let mut guard = slot.borrow_mut();
        let Some(state) = guard.as_mut()
        else {
            return json!({
                "closed": false,
                "status": gui_status("not_open", Some(GUI_NOT_OPEN), None, 0, Some("no OpenPCB window is open".to_string())),
            });
        };
        let backend = state.backend;
        let uptime_ms = state.opened_at.elapsed().as_millis() as u64;
        match state.host.close() {
            Ok(()) => {
                *guard = None;
                json!({ "closed": true, "status": gui_status("closed", None, Some(backend), uptime_ms, None) })
            }
            Err(error) => json!({
                "closed": false,
                "status": gui_status("open", Some("gui_close_failed"), Some(backend), uptime_ms, Some(error.to_string())),
            }),
        }
    });
    respond(&ctx, &result)
}

/// Reports whether the OpenPCB window is alive.
#[function(js_name = "isGuiOpen")]
fn is_gui_open(ctx: CallContext) -> Result<JsValue> {
    let open = GUI.with(|slot| slot.borrow().as_ref().is_some_and(|state| state.host.is_alive()));
    ctx.env().boolean(open)
}

register_module![
    version,
    capabilities,
    board_create,
    board_open,
    board_import,
    board_save,
    board_inspect,
    board_export,
    board_close,
    board_add_footprint,
    board_delete_footprint,
    board_add_track,
    board_add_via,
    board_add_teardrops,
    board_set_via_back_drill,
    board_route,
    board_add_net,
    board_undo,
    board_redo,
    board_move_footprint,
    board_update_footprint,
    board_delete_track,
    board_delete_via,
    board_add_differential_pair,
    board_delete_differential_pair,
    board_add_layer,
    board_update_layer,
    board_set_outline,
    board_set_rules,
    board_list,
    open_gui,
    pump_events,
    close_gui,
    is_gui_open
];
