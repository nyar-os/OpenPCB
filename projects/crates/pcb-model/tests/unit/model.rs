//! Behavioural tests for the model crate.
//!
//! They create, edit, save, reopen, inspect and export boards, then assert the
//! bytes each writer produced. Temporary directories are removed on drop.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::{Value, json};

use crate::{
    AddDifferentialPairRequest, AddFiducialRequest, AddFootprintRequest, AddKeepoutRequest, AddLayerRequest,
    AddNetClassRequest, AddNetRequest, AddPadRequest, AddSlotRequest, AddStitchingRequest, AddTeardropsRequest,
    AddTestPointRequest, AddTextRequest, AddTrackRequest, AddViaRequest, AddWaiverRequest, AddZoneRequest,
    AssignNetClassRequest, CreateRequest, ImpedanceReportRequest, ImpedanceRequest, ImpedanceTableRequest,
    MoveFootprintRequest, OutlineGeometryRequest, PanelizeRequest, RouteOptions, RouteStyle, SetOutlineRequest,
    SetRulesRequest, SetSeverityRequest, SetStackupRequest, SetViaBackDrillRequest, SplitTrackRequest, UpdateFootprintRequest,
    UpdateKeepoutRequest, UpdateLayerRequest, UpdateNetClassRequest, UpdatePadRequest, UpdateSlotRequest, UpdateTextRequest,
    UpdateTrackRequest, UpdateViaRequest, UpdateZoneRequest, WidthRequest, add_differential_pair, add_fiducial, add_footprint,
    add_keepout, add_layer, add_net, add_net_class, add_pad, add_slot, add_teardrops, add_test_point, add_text, add_track,
    add_via, add_waiver, add_zone, assign_net_class,
    board::{Board, Document, Footprint, Net, Outline, Pad, Track, Via},
    capabilities, close, copper_balance, create, delete_differential_pair, delete_fiducial, delete_footprint, delete_keepout,
    delete_net_class, delete_severity, delete_slot, delete_test_point, delete_text, delete_track, delete_via, delete_waiver,
    delete_zone, drill_report, export, fab_manifest, fiducial_report, impedance_report, impedance_table, import, inspect,
    keepout_report, list_fiducials, list_texts, move_footprint, net_audit, net_lengths, open, outline_geometry, panelize, redo,
    route, save, set_outline, set_rules, set_severity, set_stackup, set_via_back_drill, split_track, stackup_impedance,
    stackup_width, stitch_vias, store, summary, testpoint_coverage, undo, update_footprint, update_keepout, update_layer,
    update_net_class, update_pad, update_slot, update_text, update_track, update_via, update_zone, zone_audit, zone_report,
};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "open-pcb-model-test-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir_all(&path).expect("the test temp directory is created");
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn created() -> u64 {
    create(CreateRequest { title: Some("Blinker".to_string()), template: Some("blank".to_string()) })
        .expect("an empty board is created")["handle"]
        .as_u64()
        .expect("the handle is a number")
}

fn handle_of(value: &Value) -> u64 {
    value["handle"].as_u64().expect("a handle is present")
}

fn codes(report: &Value) -> Vec<&str> {
    report["issues"].as_array().expect("issues is an array").iter().filter_map(|issue| issue["code"].as_str()).collect()
}

fn add_full_board(handle: u64) {
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN32".to_string()),
            x: Some(30.0),
            y: Some(20.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the footprint is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.0, 10.0],
            to: [30.0, 10.0],
        },
    )
    .expect("the track is routed");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [20.0, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");
}

fn crafted() -> Board {
    Board::new("board-test".to_string(), None, None).expect("a blank board is built")
}

fn place_footprint(handle: u64, reference: &str, layer: &str, rotation: f64, x: f64, y: f64) {
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some(reference.to_string()),
            value: Some("0402".to_string()),
            footprint: Some("R".to_string()),
            x: Some(x),
            y: Some(y),
            rotation: Some(rotation),
            layer: Some(layer.to_string()),
        },
    )
    .expect("the footprint is placed");
}

fn add_net_pad(handle: u64, footprint: &str, number: &str, at: [f64; 2], size: [f64; 2], net: &str) {
    add_pad(
        handle,
        AddPadRequest {
            footprint: footprint.to_string(),
            number: number.to_string(),
            x: at[0],
            y: at[1],
            w: size[0],
            h: size[1],
            net: Some(net.to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
}

fn overlapping_board(first: &str, second: &str) -> u64 {
    let mut board = crafted();
    for id in [first, second] {
        board.footprints.push(Footprint {
            id: id.to_string(),
            ref_: id.to_uppercase(),
            value: String::new(),
            footprint: String::new(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            layer: "F.Cu".to_string(),
            pads: vec![Pad {
                number: "1".to_string(),
                x: 0.0,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                net: Some(format!("net-{id}")),
                drill: None,
            }],
        });
    }
    store::insert(board)
}

#[test]
fn capability_matrix_matches_the_implemented_pack() {
    let matrix = capabilities();
    assert_eq!(matrix["engine"]["package"], env!("CARGO_PKG_NAME"));
    assert_eq!(matrix["project"]["import"], "partial");
    assert_eq!(matrix["project"]["edit"], "partial");
    assert_eq!(matrix["project"]["export"], "partial");

    for format in [
        "gbr",
        "drl",
        "drill_map",
        "bom",
        "pos",
        "placement",
        "testpoints",
        "ipc_d_356",
        "svg",
        "pdf",
        "kicad_pcb",
        "ipc2581",
        "png",
    ] {
        assert_eq!(matrix["formats"][format]["export"], "supported", "{format} export");
        assert_eq!(matrix["formats"][format]["edit"], "unsupported", "{format} edit");
    }
    for format in ["gbr", "drl", "drill_map", "bom", "pos", "placement", "svg", "pdf", "png"] {
        assert_eq!(matrix["formats"][format]["import"], "unsupported", "{format} import");
    }
    assert_eq!(matrix["formats"]["kicad_pcb"]["import"], "supported", "KiCad import is implemented");
    let loss_codes: Vec<&str> = matrix["formats"]["kicad_pcb"]["lossCodes"]
        .as_array()
        .expect("KiCad import lists loss codes")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for code in ["kicad/construct-not-imported", "kicad/outline-not-rectangular", "kicad/pad-shape"] {
        assert!(loss_codes.contains(&code), "KiCad loss codes include {code}; got {loss_codes:?}");
    }
    assert_eq!(matrix["formats"]["ipc2581"]["import"], "supported", "IPC-2581 import is implemented");
    let ipc_codes: Vec<&str> = matrix["formats"]["ipc2581"]["lossCodes"]
        .as_array()
        .expect("IPC-2581 import lists loss codes")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for code in ["ipc2581/construct-not-imported", "ipc2581/outline-not-rectangular", "ipc2581/pin-without-size"] {
        assert!(ipc_codes.contains(&code), "IPC-2581 loss codes include {code}; got {ipc_codes:?}");
    }
    assert_eq!(matrix["formats"]["step"]["export"], "unsupported", "step export");
    assert_eq!(matrix["formats"]["step"]["import"], "unsupported", "step import");
    for check in [
        "drc",
        "connectivity",
        "clearance",
        "annular_rings",
        "annular-ring",
        "track-width",
        "unrouted",
        "stackup",
        "outline",
        "silkscreen",
        "courtyard",
        "netlist",
        "hole-to-hole",
        "hole-to-track",
        "copper-to-edge",
        "edge-clearance",
        "silkscreen-over-pad",
        "diffpair",
        "dangling-copper",
        "netclass",
        "keepout",
        "via-in-pad",
        "pad-overlap",
        "solder-mask",
        "testpoints",
        "stencil",
    ] {
        assert_eq!(matrix["checks"][check], "supported", "{check} check");
    }
    assert_eq!(matrix["renderers"]["svg"], "supported");
    assert_eq!(matrix["renderers"]["png"], "unsupported");
    assert_eq!(matrix["renderers"]["pdf"], "unsupported");
    for target in [
        "gbr",
        "drl",
        "drill_map",
        "bom",
        "pos",
        "placement",
        "testpoints",
        "ipc_d_356",
        "svg",
        "pdf",
        "kicad_pcb",
        "ipc2581",
        "png",
    ] {
        assert_eq!(matrix["exportTargets"][target], "supported", "{target} target");
    }
    assert_eq!(matrix["exportTargets"]["step"], "unsupported");
    assert_eq!(matrix["checks"]["outline"], "supported");
    assert_eq!(matrix["vias"]["through"], "supported");
    assert_eq!(matrix["vias"]["blind"], "supported");
    assert_eq!(matrix["vias"]["buried"], "supported");
    assert_eq!(matrix["vias"]["micro"], "supported");
    assert_eq!(matrix["differentialPairs"], "supported");
    assert_eq!(matrix["backDrill"], "supported");
    assert_eq!(matrix["teardrops"]["linear"], "supported");
    assert_eq!(matrix["teardrops"]["arc"], "unsupported");
    assert_eq!(matrix["designRules"], "supported");
    assert_eq!(matrix["solderMask"], "supported");
    assert_eq!(matrix["solderPaste"], "supported");
    assert_eq!(matrix["gerberX2"], "supported");
    assert_eq!(matrix["drcPolicy"], "supported");
    assert_eq!(matrix["routing"], "supported");
    assert_eq!(matrix["zones"], "supported");
    assert_eq!(matrix["keepouts"], "supported");
    assert_eq!(matrix["viaStitching"], "supported");
    assert_eq!(matrix["slots"], "supported");
    assert_eq!(matrix["copperBalance"], "supported");
    assert_eq!(matrix["panelization"], "supported");
    assert_eq!(matrix["polygonOutline"], "supported");
    assert_eq!(matrix["netClasses"], "supported");
    assert_eq!(matrix["testPoints"], "supported");
    assert_eq!(matrix["stackup"], "supported");
    assert_eq!(matrix["impedance"], "supported");
    assert_eq!(matrix["impedanceTable"], "supported");
}

#[test]
fn create_starts_with_the_default_stackup_and_passes_inspection() {
    let handle = created();
    let report = summary(handle).expect("the board is open");
    let layers: Vec<&str> = report["layers"]
        .as_array()
        .expect("layers is an array")
        .iter()
        .map(|layer| layer["id"].as_str().expect("layer ids are strings"))
        .collect();
    assert_eq!(layers, ["F.Cu", "B.Cu", "F.SilkS", "B.SilkS", "Edge.Cuts"]);
    assert_eq!(report["counts"]["nets"], 0);
    assert_eq!(report["counts"]["footprints"], 0);
    assert_eq!(report["revision"], 1);
    assert_eq!(report["outline"], Value::Null);
    assert_eq!(report["canUndo"], false);
    assert_eq!(report["canRedo"], false);

    let missing = inspect(handle, None).expect("an empty board is inspected");
    assert_eq!(missing["passed"], false);
    assert_eq!(
        codes(&missing),
        ["drc/outline-missing", "silkscreen/outline-missing", "courtyard/outline-missing"],
        "every check that needs the outline reports it was skipped"
    );

    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");
    let inspected = inspect(handle, None).expect("outlined board is inspected");
    assert_eq!(inspected["passed"], true);
    assert_eq!(codes(&inspected), Vec::<&str>::new());

    let partial = inspect(handle, Some(vec!["drc".to_string(), "silkscreen".to_string()])).expect("the requested checks run");
    assert_eq!(partial["passed"], true);
    assert_eq!(codes(&partial), Vec::<&str>::new());

    let unknown = inspect(handle, Some(vec!["nope".to_string()])).expect_err("an unknown check is rejected");
    assert_eq!(unknown.code, "session/argument-invalid");
    close(handle).expect("the board closes");
}

#[test]
fn round_trip_preserves_objects_and_revision() {
    let directory = TempDir::new();
    let path = directory.join("blinker.opcb");
    let handle = created();
    add_full_board(handle);

    let report = summary(handle).expect("the board is open");
    assert_eq!(report["counts"]["nets"], 1);
    assert_eq!(report["counts"]["footprints"], 1);
    assert_eq!(report["counts"]["tracks"], 1);
    assert_eq!(report["counts"]["vias"], 1);
    assert_eq!(report["revision"], 6);

    let saved = save(handle, &path, false).expect("the board saves");
    assert_eq!(saved["revision"], 6);
    assert!(saved["bytes"].as_u64().expect("bytes are counted") > 0);
    let text = fs::read_to_string(&path).expect("the document is readable");
    let document: Value = serde_json::from_str(&text).expect("the document is JSON");
    assert_eq!(document["format"], "open-pcb-board");
    assert_eq!(document["version"], 1);
    assert_eq!(document["document"]["layers"][0]["kind"], "copper");
    assert_eq!(document["document"]["footprints"][0]["ref"], "U1");
    assert_eq!(document["document"]["tracks"][0]["id"], "track-1");

    close(handle).expect("the board closes");
    let reopened = open(&path).expect("the document reopens");
    let reopened_handle = handle_of(&reopened);
    assert_eq!(reopened["board"]["revision"], 6);
    assert_eq!(reopened["board"]["outline"]["to"][0], 100.0);
    assert_eq!(reopened["board"]["counts"]["vias"], 1);
    let inspected = inspect(reopened_handle, None).expect("the reopened board is inspected");
    assert_eq!(inspected["passed"], true);

    close(reopened_handle).expect("the reopened board closes");
    assert_eq!(summary(reopened_handle).expect_err("the handle is released").code, "board/not-found");
}

#[test]
fn copper_zones_persist_and_write_gerber_regions() {
    let directory = TempDir::new();
    let path = directory.join("zones.opcb");
    let handle = created();
    add_full_board(handle);
    let added = add_zone(
        handle,
        AddZoneRequest {
            layer: "F.Cu".to_string(),
            net: Some("VCC".to_string()),
            points: vec![[10.0, 10.0], [20.0, 10.0], [20.0, 20.0], [10.0, 20.0]],
        },
    )
    .expect("the copper pour is added");
    assert_eq!(added["object"]["id"], "zone-1");
    assert_eq!(added["object"]["layer"], "F.Cu");
    assert_eq!(added["board"]["counts"]["zones"], 1);

    let silk = add_zone(
        handle,
        AddZoneRequest { layer: "F.SilkS".to_string(), net: None, points: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] },
    )
    .expect_err("a zone needs a copper layer");
    assert_eq!(silk.code, "session/argument-invalid");

    let flat = add_zone(
        handle,
        AddZoneRequest { layer: "F.Cu".to_string(), net: None, points: vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]] },
    )
    .expect_err("a zone must enclose an area");
    assert_eq!(flat.code, "session/argument-invalid");

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["zones"], 1);

    let gbr = directory.join("gerber");
    export(reopened, "gbr", &gbr).expect("Gerber export writes");
    let copper = fs::read_to_string(gbr.join("F_Cu.gbr")).expect("the copper layer is readable");
    assert!(copper.contains("G36*"), "the zone opens a Gerber region");
    assert!(copper.contains("G37*"), "the zone closes the Gerber region");
    assert!(copper.contains("X10000000Y10000000D02*"), "the region starts at the first vertex");
    let bottom = fs::read_to_string(gbr.join("B_Cu.gbr")).expect("the bottom copper layer is readable");
    assert!(!bottom.contains("G36*"), "a zone only appears on its own layer");

    let svg = directory.join("zones.svg");
    export(reopened, "svg", &svg).expect("SVG export writes");
    let text = fs::read_to_string(&svg).expect("the SVG is readable");
    assert!(text.contains("<path"), "the preview draws the zone polygon");

    let removed = delete_zone(reopened, "zone-1").expect("the zone is deleted");
    assert_eq!(removed["deleted"], "zone-1");
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["zones"], 0);
    close(reopened).expect("the board closes");
}

#[test]
fn silkscreen_text_items_edit_persist_and_reach_kicad_and_svg() {
    let directory = TempDir::new();
    let handle = created();
    let added = add_text(
        handle,
        AddTextRequest {
            text: "REV A".to_string(),
            layer: "F.SilkS".to_string(),
            at: [10.0, 20.0],
            rotation: Some(90.0),
            size: None,
            thickness: None,
        },
    )
    .expect("the text item is added");
    assert_eq!(added["object"]["id"], "text-1");
    assert_eq!(added["object"]["layer"], "F.SilkS");
    assert_eq!(added["object"]["rotation"], 90.0);
    assert_eq!(added["object"]["size"], 1.0);
    assert_eq!(added["object"]["thickness"], 0.15);
    assert_eq!(added["board"]["counts"]["texts"], 1);

    add_text(
        handle,
        AddTextRequest {
            text: " ".to_string(),
            layer: "F.SilkS".to_string(),
            at: [0.0, 0.0],
            rotation: None,
            size: None,
            thickness: None,
        },
    )
    .expect_err("blank text is rejected");
    add_text(
        handle,
        AddTextRequest {
            text: "VCC".to_string(),
            layer: "F.Cu".to_string(),
            at: [0.0, 0.0],
            rotation: None,
            size: None,
            thickness: None,
        },
    )
    .expect_err("copper is not a legend layer");

    let updated = update_text(
        handle,
        UpdateTextRequest { id: "text-1".to_string(), text: Some("REV B".to_string()), size: Some(2.0), ..Default::default() },
    )
    .expect("the text item is updated");
    assert_eq!(updated["object"]["text"], "REV B");
    assert_eq!(updated["object"]["size"], 2.0);
    assert_eq!(updated["object"]["at"][1], 20.0, "an update keeps untouched fields");

    let listed = list_texts(handle).expect("the text items are listed");
    assert_eq!(listed["count"], 1);
    assert_eq!(listed["items"][0]["text"], "REV B");

    let path = directory.join("texts.opcb");
    save(handle, &path, false).expect("the board is saved");
    let reopened = handle_of(&open(&path).expect("the board reopens"));
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["texts"], 1);

    let kicad = directory.join("texts.kicad_pcb");
    export(reopened, "kicad_pcb", &kicad).expect("the KiCad file writes");
    let text = fs::read_to_string(&kicad).expect("the KiCad file is readable");
    assert!(text.contains("(gr_text \"REV B\""), "the KiCad writer emits the legend");
    assert!(text.contains("(layer \"F.SilkS\")"), "the legend keeps its silkscreen layer");

    let svg = directory.join("texts.svg");
    export(reopened, "svg", &svg).expect("the preview writes");
    let text = fs::read_to_string(&svg).expect("the preview is readable");
    assert!(text.contains(">REV B</text>"), "the preview draws the legend text");

    let removed = delete_text(reopened, "text-1").expect("the text item is deleted");
    assert_eq!(removed["deleted"], "text-1");
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["texts"], 0);

    add_footprint(
        reopened,
        AddFootprintRequest {
            reference: Some("U9".to_string()),
            value: None,
            footprint: None,
            x: Some(1.0),
            y: Some(1.0),
            rotation: None,
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the footprint is placed");
    let before = inspect(reopened, Some(vec!["silkscreen".to_string()])).expect("silkscreen runs");
    assert!(codes(&before).contains(&"silkscreen/no-reference-text"), "a missing legend is reported");
    add_text(
        reopened,
        AddTextRequest {
            text: "U9".to_string(),
            layer: "F.SilkS".to_string(),
            at: [1.0, 3.0],
            rotation: None,
            size: None,
            thickness: None,
        },
    )
    .expect("the matching legend is added");
    let after = inspect(reopened, Some(vec!["silkscreen".to_string()])).expect("silkscreen runs");
    assert!(!codes(&after).contains(&"silkscreen/no-reference-text"), "a matching legend satisfies the check");
}

#[test]
fn drill_report_groups_tools_and_lists_holes_and_slots() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the through via is placed");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [20.0, 10.0], diameter: 0.9, drill: 0.5, ..Default::default() },
    )
    .expect("the second via is placed");
    add_slot(handle, AddSlotRequest { net: Some("VCC".to_string()), from: [1.0, 1.0], to: [6.0, 1.0], width: 0.8 })
        .expect("the slot is added");
    set_via_back_drill(handle, SetViaBackDrillRequest { id: "via-1".to_string(), depth: Some(0.8), stub: Some(0.2) })
        .expect("the via is back drilled");

    let report = drill_report(handle).expect("the drill report is built");
    assert_eq!(report["units"], "mm");
    assert_eq!(report["holeCount"], 2);
    assert_eq!(report["slotCount"], 1);
    assert_eq!(report["backDrilled"], 1);
    assert_eq!(report["vias"]["through"], 2);
    assert_eq!(report["vias"]["blind"], 0);
    assert_eq!(report["vias"]["buried"], 0);

    let tools = report["tools"].as_array().expect("tools is an array");
    assert_eq!(tools.len(), 3, "each finished diameter is one tool");
    assert_eq!(tools[0]["tool"], 1);
    assert_eq!(tools[0]["diameter"], 0.3);
    assert_eq!(tools[0]["holes"], 1);
    assert_eq!(tools[1]["diameter"], 0.5);
    assert_eq!(tools[2]["slots"], 1);
    assert_eq!(tools[2]["diameter"], 0.8);

    let holes = report["holes"].as_array().expect("holes is an array");
    assert_eq!(holes[0]["id"], "via-1");
    assert_eq!(holes[0]["tool"], 1);
    assert_eq!(holes[0]["kind"], "through");
    assert_eq!(holes[0]["backDrilled"], true);
    assert_eq!(holes[0]["at"], json!([10.0, 10.0]));
    let slots = report["slots"].as_array().expect("slots is an array");
    assert_eq!(slots[0]["id"], "slot-1");
    assert_eq!(slots[0]["width"], 0.8);
    assert_eq!(slots[0]["length"], 5.0);
}

#[test]
fn copper_zones_resize_in_place_and_report_area_and_perimeter() {
    let handle = created();
    let added = add_zone(
        handle,
        AddZoneRequest {
            layer: "F.Cu".to_string(),
            net: None,
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        },
    )
    .expect("the zone is added");
    assert_eq!(added["object"]["id"], "zone-1");

    let updated = update_zone(
        handle,
        UpdateZoneRequest {
            id: "zone-1".to_string(),
            points: Some(vec![[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]]),
            ..Default::default()
        },
    )
    .expect("the zone is resized");
    assert_eq!(updated["object"]["id"], "zone-1");
    assert_eq!(updated["object"]["points"].as_array().expect("points is an array").len(), 4);

    let report = zone_report(handle).expect("the zone report is built");
    assert_eq!(report["units"], "mm2");
    assert_eq!(report["count"], 1);
    assert_eq!(report["totalArea"], 200.0);
    assert_eq!(report["zones"][0]["vertices"], 4);
    assert_eq!(report["zones"][0]["area"], 200.0);
    assert_eq!(report["zones"][0]["perimeter"], 60.0);
    assert_eq!(report["layers"][0]["layer"], "F.Cu");
    assert_eq!(report["layers"][0]["area"], 200.0);

    update_zone(handle, UpdateZoneRequest { id: "zone-1".to_string(), layer: Some("B.Cu".to_string()), ..Default::default() })
        .expect("the zone moves to the other copper layer");
    let report = zone_report(handle).expect("the zone report is rebuilt");
    assert_eq!(report["layers"][0]["layer"], "B.Cu");

    update_zone(
        handle,
        UpdateZoneRequest { id: "zone-1".to_string(), points: Some(vec![[0.0, 0.0], [1.0, 1.0]]), ..Default::default() },
    )
    .expect_err("a zone needs three points");
    update_zone(
        handle,
        UpdateZoneRequest { id: "zone-1".to_string(), layer: Some("F.SilkS".to_string()), ..Default::default() },
    )
    .expect_err("a zone needs a copper layer");
    update_zone(handle, UpdateZoneRequest { id: "zone-404".to_string(), net: Some("GND".to_string()), ..Default::default() })
        .expect_err("an unknown zone is rejected");
}

#[test]
fn slots_persist_and_write_excellon_g85_segments() {
    let directory = TempDir::new();
    let path = directory.join("slots.opcb");
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    let added =
        add_slot(handle, AddSlotRequest { net: Some("VCC".to_string()), from: [10.0, 10.0], to: [20.0, 10.0], width: 0.8 })
            .expect("the slot is added");
    assert_eq!(added["object"]["id"], "slot-1");
    assert_eq!(added["object"]["width"], 0.8);
    assert_eq!(added["board"]["counts"]["slots"], 1);

    let flat = add_slot(handle, AddSlotRequest { net: None, from: [0.0, 0.0], to: [0.0, 0.0], width: 0.5 })
        .expect_err("a slot needs a length");
    assert_eq!(flat.code, "session/argument-invalid");
    let narrow = add_slot(handle, AddSlotRequest { net: None, from: [0.0, 0.0], to: [1.0, 0.0], width: 0.0 })
        .expect_err("a slot needs a width");
    assert_eq!(narrow.code, "session/argument-invalid");

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["slots"], 1);

    let drill = directory.join("slots.drl");
    export(reopened, "drl", &drill).expect("the drill file writes");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    assert!(text.contains("T1C0.800"), "the slot width becomes a tool; got {text}");
    assert!(text.contains("X10.000Y10.000G85X20.000Y10.000"), "the slot is a routed segment; got {text}");

    let map = directory.join("slots.drill-map.svg");
    export(reopened, "drill_map", &map).expect("the drill map writes");
    let map = fs::read_to_string(&map).expect("the drill map is readable");
    assert!(map.contains("<line"), "the slot is drawn on the map");
    assert!(map.contains("routed slots x1"), "the legend counts routed slots; got {map}");

    let removed = delete_slot(reopened, "slot-1").expect("the slot is deleted");
    assert_eq!(removed["deleted"], "slot-1");
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["slots"], 0);
    close(reopened).expect("the board closes");
}

#[test]
fn via_in_pad_flags_a_via_whose_centre_lands_inside_a_pad() {
    let handle = created();
    add_full_board(handle);
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("VCC".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [30.0, 20.0], diameter: 0.3, drill: 0.15, ..Default::default() },
    )
    .expect("the via is placed on the pad");

    let report = inspect(handle, Some(vec!["via-in-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["via-in-pad/via-in-pad"]);
    assert_eq!(report["issues"][0]["severity"], "warning");
    assert_eq!(report["issues"][0]["objectId"], "via-2");
    assert_eq!(report["issues"][0]["pad"], "fp-1:1");
    assert_eq!(report["issues"][0]["layerId"], "F.Cu");
    assert_eq!(report["passed"], true, "via-in-pad is a warning");

    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [10.0, 30.0], diameter: 0.3, drill: 0.15, ..Default::default() },
    )
    .expect("the off-pad via is placed");
    let clean = inspect(handle, Some(vec!["via-in-pad".to_string()])).expect("the check runs again");
    assert_eq!(codes(&clean), ["via-in-pad/via-in-pad"], "only the on-pad via is flagged");
    assert_eq!(clean["issues"].as_array().expect("issues is an array").len(), 1);
    close(handle).expect("the board closes");
}

#[test]
fn pad_overlap_flags_two_nets_sharing_copper() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    place_footprint(handle, "U1", "F.Cu", 0.0, 10.0, 10.0);
    place_footprint(handle, "U2", "F.Cu", 0.0, 10.0, 10.0);
    add_net_pad(handle, "fp-1", "1", [0.0, 0.0], [1.0, 1.0], "VCC");
    add_net_pad(handle, "fp-2", "1", [0.5, 0.0], [1.0, 1.0], "GND");

    let revision = summary(handle).expect("the board is listed")["revision"].clone();
    let report = inspect(handle, Some(vec!["pad-overlap".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/pad-overlap"]);
    assert_eq!(report["issues"][0]["severity"], "error");
    assert_eq!(report["issues"][0]["layerId"], "F.Cu");
    assert_eq!(report["issues"][0]["objectId"], "fp-1:1+fp-2:1");
    let message = report["issues"][0]["message"].as_str().expect("the issue carries a message");
    assert!(message.contains("pad `1` of `U1`"), "the message names the first pad; got {message}");
    assert!(message.contains("pad `1` of `U2`"), "the message names the second pad; got {message}");
    assert_eq!(report["passed"], false);
    assert_eq!(report["revision"], revision);
    assert_eq!(summary(handle).expect("the board is listed")["revision"], revision, "inspection never bumps the revision");
    close(handle).expect("the board closes");
}

#[test]
fn pad_overlap_ignores_touching_pads() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    place_footprint(handle, "U1", "F.Cu", 0.0, 0.0, 0.0);
    add_net_pad(handle, "fp-1", "1", [0.0, 0.0], [1.0, 1.0], "VCC");
    add_net_pad(handle, "fp-1", "2", [1.0, 0.0], [1.0, 1.0], "GND");

    let clean = inspect(handle, Some(vec!["pad-overlap".to_string()])).expect("the check runs");
    assert!(codes(&clean).is_empty(), "a shared edge is contact, not overlap");

    update_pad(
        handle,
        UpdatePadRequest { footprint: "fp-1".to_string(), number: "2".to_string(), x: Some(0.9), ..Default::default() },
    )
    .expect("the pad moves into its neighbour");
    let report = inspect(handle, Some(vec!["pad-overlap".to_string()])).expect("the check runs again");
    assert_eq!(codes(&report), ["drc/pad-overlap"], "moving the pad into its neighbour overlaps");
    close(handle).expect("the board closes");
}

#[test]
fn pad_overlap_ignores_pads_on_different_layers_and_nets() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    place_footprint(handle, "U1", "F.Cu", 0.0, 10.0, 10.0);
    place_footprint(handle, "U2", "B.Cu", 0.0, 10.0, 10.0);
    add_net_pad(handle, "fp-1", "1", [0.0, 0.0], [1.0, 1.0], "VCC");
    add_net_pad(handle, "fp-2", "1", [0.0, 0.0], [1.0, 1.0], "GND");

    let layers = inspect(handle, Some(vec!["pad-overlap".to_string()])).expect("the check runs");
    assert!(codes(&layers).is_empty(), "copper on different layers does not collide");

    place_footprint(handle, "U3", "F.Cu", 0.0, 20.0, 20.0);
    place_footprint(handle, "U4", "F.Cu", 0.0, 20.0, 20.0);
    add_net_pad(handle, "fp-3", "1", [0.0, 0.0], [1.0, 1.0], "VCC");
    add_net_pad(handle, "fp-4", "1", [0.0, 0.0], [1.0, 1.0], "VCC");
    let nets = inspect(handle, Some(vec!["pad-overlap".to_string()])).expect("the check runs again");
    assert!(codes(&nets).is_empty(), "pads on one net are connected, not an overlap");
    close(handle).expect("the board closes");
}

#[test]
fn pad_overlap_follows_footprint_rotation() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    place_footprint(handle, "U1", "F.Cu", 0.0, 0.0, 0.0);
    add_net_pad(handle, "fp-1", "1", [0.0, 0.0], [4.0, 0.4], "VCC");
    // A quarter turn makes the second pad vertical, so it crosses the first.
    place_footprint(handle, "U2", "F.Cu", 90.0, 0.0, 2.0);
    add_net_pad(handle, "fp-2", "1", [0.0, 0.0], [4.0, 0.4], "GND");

    let report = inspect(handle, Some(vec!["pad-overlap".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/pad-overlap"], "a rotated pad is compared by its true corners");
    assert_eq!(report["issues"][0]["objectId"], "fp-1:1+fp-2:1");
    close(handle).expect("the board closes");
}

#[test]
fn pad_overlap_reports_each_pair_once_in_a_stable_order() {
    let forward = overlapping_board("fp-a", "fp-b");
    let reverse = overlapping_board("fp-b", "fp-a");
    let check = || Some(vec!["pad-overlap".to_string()]);
    let first = inspect(forward, check()).expect("the check runs");
    let second = inspect(reverse, check()).expect("the check runs on the reversed board");
    let again = inspect(forward, check()).expect("the check runs a second time");

    assert_eq!(codes(&first), ["drc/pad-overlap"]);
    assert_eq!(first["issues"][0]["objectId"], "fp-a:1+fp-b:1", "the pair is ordered by footprint id");
    assert_eq!(first["issues"], second["issues"], "insertion order does not change the report");
    assert_eq!(first["issues"], again["issues"], "repeated runs are identical");

    let revision = summary(forward).expect("the board is listed")["revision"].clone();
    assert_eq!(first["revision"], revision, "inspection leaves the revision alone");
    close(forward).expect("the board closes");
    close(reverse).expect("the board closes");
}

#[test]
fn solder_mask_flags_merged_and_sliver_openings() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    place_footprint(handle, "U1", "F.Cu", 0.0, 0.0, 0.0);
    // 0.5 mm pads 0.6 mm apart leave a 0.1 mm copper gap, so copper does not
    // overlap; the default 0.05 mm mask expansion per side closes that gap.
    add_net_pad(handle, "fp-1", "1", [0.0, 0.0], [0.5, 0.5], "VCC");
    add_net_pad(handle, "fp-1", "2", [0.6, 0.0], [0.5, 0.5], "GND");

    let check = || Some(vec!["solder-mask".to_string()]);
    let bridge = inspect(handle, check()).expect("the check runs");
    assert_eq!(codes(&bridge), ["drc/solder-mask-bridge"]);
    assert_eq!(bridge["issues"][0]["severity"], "error");
    assert_eq!(bridge["issues"][0]["layerId"], "F.Cu");
    assert_eq!(bridge["issues"][0]["objectId"], "fp-1:1+fp-1:2");
    let message = bridge["issues"][0]["message"].as_str().expect("the issue carries a message");
    assert!(message.contains("overlap"), "the message explains the merged openings; got {message}");
    assert_eq!(bridge["passed"], false);

    update_pad(
        handle,
        UpdatePadRequest { footprint: "fp-1".to_string(), number: "2".to_string(), x: Some(0.65), ..Default::default() },
    )
    .expect("the pad moves out of the opening");
    let sliver = inspect(handle, check()).expect("the check runs again");
    assert_eq!(codes(&sliver), ["drc/solder-mask-sliver"]);
    assert_eq!(sliver["issues"][0]["severity"], "warning");
    let message = sliver["issues"][0]["message"].as_str().expect("the issue carries a message");
    assert!(message.contains("0.050 mm"), "the web is measured; got {message}");
    assert_eq!(sliver["passed"], true, "a thin web warns without failing the board");

    update_pad(
        handle,
        UpdatePadRequest { footprint: "fp-1".to_string(), number: "2".to_string(), x: Some(0.7), ..Default::default() },
    )
    .expect("the pad reaches the minimum web");
    let clean = inspect(handle, check()).expect("the check runs again");
    assert!(codes(&clean).is_empty(), "a web at the minimum width is accepted");
    close(handle).expect("the board closes");
}

#[test]
fn solder_mask_skips_same_net_and_inner_layer_pads() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    place_footprint(handle, "U1", "F.Cu", 0.0, 0.0, 0.0);
    add_net_pad(handle, "fp-1", "1", [0.0, 0.0], [0.5, 0.5], "VCC");
    add_net_pad(handle, "fp-1", "2", [0.6, 0.0], [0.5, 0.5], "VCC");
    let same_net = inspect(handle, Some(vec!["solder-mask".to_string()])).expect("the check runs");
    assert!(codes(&same_net).is_empty(), "one net may share or merge its own openings");

    add_layer(handle, AddLayerRequest { name: "Inner1".to_string(), kind: Some("copper".to_string()), color: None })
        .expect("an inner copper layer is added");
    place_footprint(handle, "U2", "Inner1", 0.0, 10.0, 10.0);
    add_net_pad(handle, "fp-2", "1", [0.0, 0.0], [0.5, 0.5], "VCC");
    add_net_pad(handle, "fp-2", "2", [0.6, 0.0], [0.5, 0.5], "GND");
    let inner = inspect(handle, Some(vec!["solder-mask".to_string()])).expect("the check runs again");
    assert!(codes(&inner).is_empty(), "inner copper opens no solder mask");

    place_footprint(handle, "U3", "B.Cu", 0.0, 20.0, 20.0);
    add_net_pad(handle, "fp-3", "1", [0.0, 0.0], [0.5, 0.5], "VCC");
    add_net_pad(handle, "fp-3", "2", [0.65, 0.0], [0.5, 0.5], "GND");
    let back = inspect(handle, Some(vec!["solder-mask".to_string()])).expect("the check runs on the back side");
    assert_eq!(codes(&back), ["drc/solder-mask-sliver"]);
    assert_eq!(back["issues"][0]["layerId"], "B.Cu");
    close(handle).expect("the board closes");
}

#[test]
fn stencil_flags_area_and_aspect_ratio_failures() {
    let handle = created();
    place_footprint(handle, "U1", "F.Cu", 0.0, 0.0, 0.0);
    // 1.0 x 1.0 mm: area ratio 2.083 and aspect ratio 8.333 against 0.12 mm.
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: None,
            drill: None,
        },
    )
    .expect("the clean pad is added");
    // 0.3 x 0.3 mm: area ratio 0.625 fails, the aspect ratio 2.5 passes.
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "2".to_string(),
            x: 0.0,
            y: 5.0,
            w: 0.3,
            h: 0.3,
            net: None,
            drill: None,
        },
    )
    .expect("the area-ratio pad is added");
    // 0.17 x 20 mm: area ratio 0.702 passes, the aspect ratio 1.417 fails.
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "3".to_string(),
            x: 5.0,
            y: 0.0,
            w: 0.17,
            h: 20.0,
            net: None,
            drill: None,
        },
    )
    .expect("the aspect-ratio pad is added");

    let check = || Some(vec!["stencil".to_string()]);
    let report = inspect(handle, check()).expect("the check runs");
    assert_eq!(codes(&report), ["drc/stencil-area-ratio", "drc/stencil-aspect-ratio"]);
    assert_eq!(report["issues"][0]["severity"], "warning");
    assert_eq!(report["issues"][0]["layerId"], "F.Cu");
    assert_eq!(report["issues"][0]["objectId"], "fp-1:2");
    assert_eq!(report["issues"][0]["areaRatio"], "0.625");
    assert_eq!(report["issues"][1]["objectId"], "fp-1:3");
    assert_eq!(report["issues"][1]["aspectRatio"], "1.417");
    assert_eq!(report["passed"], true, "a stencil warning does not fail the board");

    let again = inspect(handle, check()).expect("the check runs a second time");
    assert_eq!(report["issues"], again["issues"], "repeated runs are identical");
    close(handle).expect("the board closes");
}

#[test]
fn stencil_honours_rules_and_waivers() {
    let handle = created();
    place_footprint(handle, "U1", "F.Cu", 0.0, 0.0, 0.0);
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.3,
            h: 0.3,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");

    let check = || Some(vec!["stencil".to_string()]);
    assert_eq!(codes(&inspect(handle, check()).expect("the check runs")), ["drc/stencil-area-ratio"]);

    add_waiver(
        handle,
        AddWaiverRequest {
            code: "drc/stencil-area-ratio".to_string(),
            object: Some("fp-1:1".to_string()),
            reason: Some("prototype stencil".to_string()),
        },
    )
    .expect("the finding is waived");
    let waived = inspect(handle, check()).expect("the check runs");
    assert_eq!(waived["issues"][0]["waived"], true);
    assert_eq!(waived["waived"], 1);
    assert_eq!(waived["active"], 0);
    assert_eq!(waived["passed"], true);

    let relaxed = set_rules(handle, SetRulesRequest { stencil_area_ratio: Some(0.5), ..Default::default() })
        .expect("the area-ratio rule is relaxed");
    assert_eq!(relaxed["object"]["stencilAreaRatio"], 0.5);
    assert_eq!(relaxed["object"]["stencilThickness"], 0.12, "untouched stencil rules keep their value");
    assert!(codes(&inspect(handle, check()).expect("the check runs again")).is_empty());

    let zero_thickness = set_rules(handle, SetRulesRequest { stencil_thickness: Some(0.0), ..Default::default() })
        .expect_err("a zero thickness is rejected");
    assert_eq!(zero_thickness.code, "session/argument-invalid");
    let negative_ratio = set_rules(handle, SetRulesRequest { stencil_aspect_ratio: Some(-1.0), ..Default::default() })
        .expect_err("a negative ratio is rejected");
    assert_eq!(negative_ratio.code, "session/argument-invalid");
    close(handle).expect("the board closes");
}

#[test]
fn stencil_skips_inner_layers_and_other_checks_stay_filtered() {
    let handle = created();
    add_layer(handle, AddLayerRequest { name: "Inner1".to_string(), kind: Some("copper".to_string()), color: None })
        .expect("an inner copper layer is added");
    place_footprint(handle, "U1", "Inner1", 0.0, 0.0, 0.0);
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.3,
            h: 0.3,
            net: None,
            drill: None,
        },
    )
    .expect("the inner pad is added");

    assert!(codes(&inspect(handle, Some(vec!["stencil".to_string()])).expect("the check runs")).is_empty());
    close(handle).expect("the board closes");
}

#[test]
fn copper_balance_sums_layers_and_reports_coverage_imbalance() {
    let handle = created();
    add_full_board(handle);
    add_zone(
        handle,
        AddZoneRequest {
            layer: "F.Cu".to_string(),
            net: Some("VCC".to_string()),
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        },
    )
    .expect("the pour is added");

    let report = copper_balance(handle).expect("the report is built");
    assert_eq!(report["revision"], 7);
    assert_eq!(report["units"], "mm2");
    assert_eq!(report["boardArea"], 8000.0);
    let layers = report["layers"].as_array().expect("layers is an array");
    assert_eq!(layers.len(), 2, "only copper layers are reported");
    assert_eq!(layers[0]["layer"], "F.Cu");
    assert_eq!(layers[1]["layer"], "B.Cu");
    let front = layers[0]["area"].as_f64().expect("the front area is a number");
    assert!((front - 105.282743).abs() < 1e-4, "front area is track, via and pour; got {front}");
    assert_eq!(layers[0]["zones"], 100.0);
    assert_eq!(layers[0]["tracks"], 5.0);
    let back = layers[1]["area"].as_f64().expect("the back area is a number");
    assert!((back - 0.282743).abs() < 1e-4, "back area is the through via; got {back}");
    assert_eq!(report["imbalancePercent"], 1.3125);
    close(handle).expect("the board closes");
}

#[test]
fn copper_balance_without_an_outline_reports_areas_without_coverage() {
    let mut board = crafted();
    board.tracks.push(Track {
        id: "track-1".to_string(),
        net: None,
        layer: "B.Cu".to_string(),
        width: 0.5,
        from: [0.0, 0.0],
        to: [4.0, 0.0],
        teardrop: false,
    });
    let handle = store::insert(board);
    let report = copper_balance(handle).expect("the report is built");
    assert_eq!(report["boardArea"], Value::Null);
    assert_eq!(report["minCoveragePercent"], Value::Null);
    assert_eq!(report["imbalancePercent"], Value::Null);
    assert_eq!(report["totalArea"], 2.0);
    assert_eq!(report["layers"][1]["area"], 2.0);
    close(handle).expect("the board closes");
}

#[test]
fn panelization_tiles_the_outline_and_reports_utilization() {
    let handle = created();
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");
    let report = panelize(
        handle,
        PanelizeRequest {
            columns: 2,
            rows: 2,
            gap_x: Some(5.0),
            gap_y: Some(5.0),
            rail: Some(2.0),
            tab: Some("mouse-bite".to_string()),
            tab_width: Some(5.0),
            tab_spacing: Some(20.0),
            tooling_hole: None,
            tooling_hole_inset: None,
        },
    )
    .expect("the panel is reported");

    assert_eq!(report["revision"], 2);
    assert_eq!(report["units"], "mm");
    assert_eq!(report["boardCount"], 4);
    assert_eq!(report["panelSize"]["width"], 209.0);
    assert_eq!(report["panelSize"]["height"], 169.0);
    assert_eq!(report["panelOutline"]["from"], json!([-2.0, -2.0]));
    assert_eq!(report["panelOutline"]["to"], json!([207.0, 167.0]));
    assert_eq!(report["placements"][0]["from"], json!([0.0, 0.0]));
    assert_eq!(report["placements"][3]["from"], json!([105.0, 85.0]));
    assert_eq!(report["placements"][3]["to"], json!([205.0, 165.0]));
    assert_eq!(report["seams"].as_array().expect("seams is an array").len(), 2);
    assert_eq!(report["tabCount"], 18);
    assert_eq!(report["boardArea"], 8000.0);
    assert_eq!(report["panelArea"], 35321.0);
    assert_eq!(report["breakaway"]["area"], 3321.0);
    assert_eq!(report["breakaway"]["railArea"], 1496.0);
    assert_eq!(report["breakaway"]["gapArea"], 1825.0);
    assert_eq!(report["toolingHoleCount"], 0);
    assert_eq!(report["toolingHoles"], json!([]));
    let utilization = report["utilizationPercent"].as_f64().expect("utilization is a number");
    assert!((90.0..91.0).contains(&utilization), "utilization is plausible: {utilization}");
    close(handle).expect("the board closes");
}

#[test]
fn panelization_places_tooling_holes_in_the_rail_and_reports_breakaway() {
    let handle = created();
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [60.0, 40.0] }).expect("the outline is set");
    let request = |tooling_hole, tooling_hole_inset| PanelizeRequest {
        columns: 2,
        rows: 1,
        gap_x: Some(5.0),
        gap_y: None,
        rail: Some(3.0),
        tab: None,
        tab_width: None,
        tab_spacing: None,
        tooling_hole,
        tooling_hole_inset,
    };
    let report = panelize(handle, request(Some(2.0), Some(1.5))).expect("the panel is reported");

    assert_eq!(report["panelSize"]["width"], 131.0);
    assert_eq!(report["panelSize"]["height"], 46.0);
    assert_eq!(report["panelOutline"]["from"], json!([-3.0, -3.0]));
    assert_eq!(report["panelOutline"]["to"], json!([128.0, 43.0]));
    assert_eq!(report["toolingHoleCount"], 4);
    assert_eq!(
        report["toolingHoles"],
        json!([
            { "corner": "bottom-left", "x": -1.5, "y": -1.5, "diameter": 2.0 },
            { "corner": "bottom-right", "x": 126.5, "y": -1.5, "diameter": 2.0 },
            { "corner": "top-right", "x": 126.5, "y": 41.5, "diameter": 2.0 },
            { "corner": "top-left", "x": -1.5, "y": 41.5, "diameter": 2.0 },
        ])
    );
    assert_eq!(report["breakaway"]["area"], 1226.0);
    assert_eq!(report["breakaway"]["railArea"], 1026.0);
    assert_eq!(report["breakaway"]["gapArea"], 200.0);

    let no_rail =
        panelize(handle, PanelizeRequest { rail: None, ..request(Some(2.0), None) }).expect_err("a tooling hole needs a rail");
    assert_eq!(no_rail.code, "session/argument-invalid");
    let too_big = panelize(handle, request(Some(8.0), None)).expect_err("the hole must fit inside the rail");
    assert_eq!(too_big.code, "session/argument-invalid");
    let too_close = panelize(handle, request(Some(2.0), Some(0.5))).expect_err("the inset must clear the hole radius");
    assert_eq!(too_close.code, "session/argument-invalid");
    let zero = panelize(handle, request(Some(0.0), None)).expect_err("a zero diameter is rejected");
    assert_eq!(zero.code, "session/argument-invalid");
    close(handle).expect("the board closes");
}

#[test]
fn panelization_rejects_an_absent_outline_and_invalid_tabs() {
    let handle = created();
    let request = |columns, rows, tab: Option<&str>, gap| PanelizeRequest {
        columns,
        rows,
        gap_x: Some(gap),
        gap_y: Some(gap),
        rail: None,
        tab: tab.map(str::to_string),
        tab_width: Some(5.0),
        tab_spacing: Some(20.0),
        tooling_hole: None,
        tooling_hole_inset: None,
    };

    let missing = panelize(handle, request(2, 1, None, 0.0)).expect_err("a board without an outline cannot be panelized");
    assert_eq!(missing.code, "session/argument-invalid");

    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");

    let empty = panelize(handle, request(0, 1, None, 0.0)).expect_err("an empty array is rejected");
    assert_eq!(empty.code, "session/argument-invalid");

    let tabs = panelize(handle, request(2, 1, Some("mouse-bite"), 0.0)).expect_err("tabs need a positive gap");
    assert_eq!(tabs.code, "session/argument-invalid");

    let style = panelize(handle, request(2, 1, Some("castellated"), 2.0)).expect_err("an unknown tab style is rejected");
    assert_eq!(style.code, "session/argument-invalid");
    close(handle).expect("the board closes");
}

#[test]
fn panelization_defaults_and_repeat_reports_are_stable() {
    let handle = created();
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");
    let request = || PanelizeRequest {
        columns: 3,
        rows: 1,
        gap_x: None,
        gap_y: None,
        rail: None,
        tab: None,
        tab_width: None,
        tab_spacing: None,
        tooling_hole: None,
        tooling_hole_inset: None,
    };

    let report = panelize(handle, request()).expect("the panel is reported");
    assert_eq!(report["gapX"], 0.0);
    assert_eq!(report["gapY"], 0.0);
    assert_eq!(report["rail"], 0.0);
    assert_eq!(report["tab"], "v-cut");
    assert_eq!(report["tabCount"], 0);
    assert_eq!(report["tabs"], json!([]));
    assert_eq!(report["boardCount"], 3);
    assert_eq!(report["panelSize"]["width"], 300.0);
    assert_eq!(report["panelSize"]["height"], 80.0);
    assert_eq!(report["panelOutline"]["from"], json!([0.0, 0.0]));
    assert_eq!(report["placements"].as_array().expect("placements is an array").len(), 3);
    assert_eq!(report["seams"].as_array().expect("seams is an array").len(), 2);
    assert_eq!(report["utilizationPercent"], 100.0);

    let repeat = panelize(handle, request()).expect("the panel is reported again");
    assert_eq!(repeat, report, "the same board and request produce the same report");
    close(handle).expect("the board closes");
}

#[test]
fn outline_geometry_measures_a_polygon_and_offsets_it() {
    let handle = created();
    let square = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];

    let report = outline_geometry(handle, OutlineGeometryRequest { points: Some(square.clone()), offset: Some(1.0) })
        .expect("the polygon is measured");
    assert_eq!(report["source"], "request");
    assert_eq!(report["units"], "mm");
    assert_eq!(report["vertexCount"], 4);
    assert_eq!(report["orientation"], "counter-clockwise");
    assert_eq!(report["area"], 100.0);
    assert_eq!(report["perimeter"], 40.0);
    assert_eq!(report["offset"]["distance"], 1.0);
    assert_eq!(report["offset"]["area"], 144.0);
    assert_eq!(report["offset"]["perimeter"], 48.0);
    assert_eq!(report["offset"]["points"][0], json!([-1.0, -1.0]));
    assert_eq!(report["offset"]["points"][2], json!([11.0, 11.0]));

    let inward = outline_geometry(handle, OutlineGeometryRequest { points: Some(square), offset: Some(-1.0) })
        .expect("the inward offset is measured");
    assert_eq!(inward["offset"]["area"], 64.0);
    assert_eq!(inward["offset"]["perimeter"], 32.0);
    close(handle).expect("the board closes");
}

#[test]
fn outline_geometry_rejects_broken_polygons_and_impossible_offsets() {
    let handle = created();
    let missing = outline_geometry(handle, OutlineGeometryRequest { points: None, offset: None })
        .expect_err("a board without an outline has nothing to measure");
    assert_eq!(missing.code, "session/argument-invalid");

    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [10.0, 10.0] }).expect("the outline is set");
    let stored = outline_geometry(handle, OutlineGeometryRequest { points: None, offset: None })
        .expect("the stored outline is measured");
    assert_eq!(stored["source"], "outline");
    assert_eq!(stored["area"], 100.0);
    assert_eq!(stored["perimeter"], 40.0);

    let bowtie = vec![[0.0, 0.0], [10.0, 10.0], [10.0, 0.0], [0.0, 10.0]];
    let crossing = outline_geometry(handle, OutlineGeometryRequest { points: Some(bowtie), offset: None })
        .expect_err("a self-intersecting polygon is rejected");
    assert_eq!(crossing.code, "session/argument-invalid");

    let duplicate = outline_geometry(
        handle,
        OutlineGeometryRequest { points: Some(vec![[0.0, 0.0], [0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]), offset: None },
    )
    .expect_err("a repeated vertex is rejected");
    assert_eq!(duplicate.code, "session/argument-invalid");

    let collapsed = outline_geometry(
        handle,
        OutlineGeometryRequest { points: Some(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]), offset: Some(-6.0) },
    )
    .expect_err("an offset past the centre is rejected");
    assert_eq!(collapsed.code, "session/argument-invalid");
    close(handle).expect("the board closes");
}

#[test]
fn net_classes_apply_clearance_and_width_targets() {
    let directory = TempDir::new();
    let path = directory.join("classes.opcb");
    let handle = created();
    add_full_board(handle);
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the ground net is declared");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("GND".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.0, 10.6],
            to: [30.0, 10.6],
        },
    )
    .expect("the ground track is placed");
    let clean = inspect(handle, Some(vec!["clearance".to_string()])).expect("the board is inspected");
    assert_eq!(clean["passed"], true, "the default clearance accepts the pair");

    let declared = add_net_class(
        handle,
        AddNetClassRequest {
            name: "Power".to_string(),
            width: Some(0.5),
            clearance: Some(0.6),
            via_diameter: Some(0.8),
            via_drill: Some(0.4),
        },
    )
    .expect("the class is declared");
    assert_eq!(declared["object"]["id"], "netclass-1");
    let duplicate = add_net_class(handle, AddNetClassRequest { name: "Power".to_string(), ..Default::default() })
        .expect_err("class names are unique");
    assert_eq!(duplicate.code, "session/argument-invalid");

    let assigned = assign_net_class(handle, AssignNetClassRequest { net: "VCC".to_string(), class: "Power".to_string() })
        .expect("the power net joins the class");
    assert_eq!(assigned["object"]["nets"], serde_json::json!(["VCC"]));
    assign_net_class(handle, AssignNetClassRequest { net: "GND".to_string(), class: "netclass-1".to_string() })
        .expect("a class id also assigns");
    let missing = assign_net_class(handle, AssignNetClassRequest { net: "NOPE".to_string(), class: "Power".to_string() })
        .expect_err("an undeclared net cannot join a class");
    assert_eq!(missing.code, "object/not-found");

    let report = inspect(handle, Some(vec!["netclass".to_string(), "clearance".to_string()])).expect("the board is inspected");
    assert_eq!(report["passed"], false);
    let codes: Vec<&str> =
        report["issues"].as_array().expect("issues are listed").iter().filter_map(|issue| issue["code"].as_str()).collect();
    assert!(codes.contains(&"netclass/track-too-narrow"), "class width is enforced; got {codes:?}");
    assert!(codes.contains(&"drc/clearance-violation"), "class clearance is enforced; got {codes:?}");
    let clearance = report["issues"]
        .as_array()
        .expect("issues are listed")
        .iter()
        .find(|issue| issue["code"] == "drc/clearance-violation")
        .expect("a clearance issue is listed");
    assert!(
        clearance["message"].as_str().is_some_and(|message| message.contains("0.6 mm")),
        "the finding names the class clearance: {clearance:?}"
    );

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    let listed = summary(reopened).expect("the board is listed");
    assert_eq!(listed["counts"]["netClasses"], 1);
    assert_eq!(listed["netClasses"][0]["name"], "Power");
    assert_eq!(listed["netClasses"][0]["nets"], serde_json::json!(["VCC", "GND"]));
    delete_net_class(reopened, "netclass-1").expect("the class is deleted");
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["netClasses"], 0);
    close(reopened).expect("the board closes");
}

#[test]
fn net_classes_update_partially_and_keep_assignments() {
    let directory = TempDir::new();
    let path = directory.join("class-update.opcb");
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the power net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the ground net is declared");
    add_net_class(
        handle,
        AddNetClassRequest {
            name: "Power".to_string(),
            width: Some(0.4),
            clearance: Some(0.3),
            via_diameter: Some(0.8),
            via_drill: Some(0.4),
        },
    )
    .expect("the class is declared");
    add_net_class(handle, AddNetClassRequest { name: "Signal".to_string(), ..Default::default() })
        .expect("a second class is declared");
    assign_net_class(handle, AssignNetClassRequest { net: "VCC".to_string(), class: "Power".to_string() })
        .expect("the power net joins the class");
    assign_net_class(handle, AssignNetClassRequest { net: "GND".to_string(), class: "netclass-1".to_string() })
        .expect("the ground net joins the class");
    let revision = |board: u64| summary(board).expect("the board is listed")["revision"].as_u64().expect("a revision");
    let before = revision(handle);

    let empty = update_net_class(handle, UpdateNetClassRequest { id: "netclass-1".to_string(), ..Default::default() })
        .expect_err("an update needs one field");
    assert_eq!(empty.code, "session/argument-invalid");
    assert_eq!(revision(handle), before, "a rejected update writes nothing");

    let updated = update_net_class(
        handle,
        UpdateNetClassRequest {
            id: "netclass-1".to_string(),
            name: Some("PowerRail".to_string()),
            width: Some(0.5),
            ..Default::default()
        },
    )
    .expect("the class is updated");
    assert_eq!(updated["object"]["id"], "netclass-1");
    assert_eq!(updated["object"]["name"], "PowerRail");
    assert_eq!(updated["object"]["width"], 0.5);
    assert_eq!(updated["object"]["clearance"], 0.3, "an omitted clearance is kept");
    assert_eq!(updated["object"]["viaDiameter"], 0.8, "an omitted diameter is kept");
    assert_eq!(updated["object"]["nets"], serde_json::json!(["VCC", "GND"]), "a rename keeps the assignments");
    assert_eq!(revision(handle), before + 1, "a successful update bumps the revision once");

    let duplicate = update_net_class(
        handle,
        UpdateNetClassRequest { id: "netclass-1".to_string(), name: Some("Signal".to_string()), ..Default::default() },
    )
    .expect_err("class names stay unique");
    assert_eq!(duplicate.code, "session/argument-invalid");
    let blank = update_net_class(
        handle,
        UpdateNetClassRequest { id: "netclass-1".to_string(), name: Some("   ".to_string()), ..Default::default() },
    )
    .expect_err("a blank name is rejected");
    assert_eq!(blank.code, "session/argument-invalid");
    let zero = update_net_class(
        handle,
        UpdateNetClassRequest { id: "netclass-1".to_string(), width: Some(0.0), ..Default::default() },
    )
    .expect_err("a zero width is rejected");
    assert_eq!(zero.code, "session/argument-invalid");
    let infinite = update_net_class(
        handle,
        UpdateNetClassRequest { id: "netclass-1".to_string(), clearance: Some(f64::INFINITY), ..Default::default() },
    )
    .expect_err("a non-finite clearance is rejected");
    assert_eq!(infinite.code, "session/argument-invalid");
    let missing = update_net_class(
        handle,
        UpdateNetClassRequest { id: "netclass-9".to_string(), width: Some(0.5), ..Default::default() },
    )
    .expect_err("the class must exist");
    assert_eq!(missing.code, "object/not-found");
    assert_eq!(revision(handle), before + 1, "each rejected update keeps the revision");

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    let listed = summary(reopened).expect("the board is listed");
    assert_eq!(listed["netClasses"][0]["name"], "PowerRail");
    assert_eq!(listed["netClasses"][0]["width"], 0.5);
    assert_eq!(listed["netClasses"][0]["clearance"], 0.3);
    assert_eq!(listed["netClasses"][0]["nets"], serde_json::json!(["VCC", "GND"]));
    assert_eq!(listed["netClasses"][1]["name"], "Signal");
    close(reopened).expect("the board closes");
}

#[test]
fn test_points_report_pad_and_via_probes() {
    let directory = TempDir::new();
    let path = directory.join("testpoints.opcb");
    let handle = created();
    add_full_board(handle);
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 1.0,
            y: 1.0,
            w: 0.8,
            h: 0.8,
            net: Some("VCC".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");

    let pad =
        add_test_point(handle, AddTestPointRequest { target: "fp-1:1".to_string() }).expect("the pad becomes a test point");
    assert_eq!(pad["object"]["kind"], "pad");
    assert_eq!(pad["object"]["designator"], "U1");
    assert_eq!(pad["object"]["pad"], "1");
    assert_eq!(pad["object"]["net"], "VCC");
    assert_eq!(pad["object"]["at"], serde_json::json!([31.0, 21.0]));
    let via =
        add_test_point(handle, AddTestPointRequest { target: "via-1".to_string() }).expect("the via becomes a test point");
    assert_eq!(via["object"]["kind"], "via");
    assert_eq!(via["object"]["layer"], "F.Cu..B.Cu");
    assert_eq!(via["object"]["at"], serde_json::json!([20.0, 20.0]));

    let duplicate =
        add_test_point(handle, AddTestPointRequest { target: "via-1".to_string() }).expect_err("a target is declared once");
    assert_eq!(duplicate.code, "session/argument-invalid");
    let missing =
        add_test_point(handle, AddTestPointRequest { target: "fp-9:1".to_string() }).expect_err("the target must exist");
    assert_eq!(missing.code, "object/not-found");

    let csv_path = directory.join("testpoints.csv");
    export(handle, "testpoints", &csv_path).expect("the report writes");
    let report = fs::read_to_string(&csv_path).expect("the report is readable");
    assert!(report.starts_with("Target,Designator,Pad,Kind,Net,Layer,X,Y\n"), "the header is stable: {report}");
    assert!(report.contains("fp-1:1,U1,1,pad,VCC,F.Cu,31.000,21.000"), "the pad row uses millimetres: {report}");
    assert!(report.contains("via-1,,,via,VCC,F.Cu..B.Cu,20.000,20.000"), "the via row reports its span: {report}");

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["testPoints"], 2);

    delete_via(reopened, "via-1").expect("the via is deleted");
    assert_eq!(
        summary(reopened).expect("the board is listed")["counts"]["testPoints"],
        1,
        "deleting the via drops its test point"
    );
    delete_test_point(reopened, "testpoint-1").expect("the pad test point is deleted");
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["testPoints"], 0);
    close(reopened).expect("the board closes");
}

#[test]
fn fiducials_are_managed_reported_and_undoable() {
    let directory = TempDir::new();
    let path = directory.join("fiducials.opcb");
    let handle = created();
    add_full_board(handle);
    assert_eq!(summary(handle).expect("the board is listed")["counts"]["fiducials"], 0);

    for (layer, at) in [("F.Fid", [10.0, 10.0]), ("F.Fid", [90.0, 10.0]), ("B.Fid", [10.0, 70.0]), ("B.Fid", [90.0, 70.0])] {
        add_fiducial(handle, AddFiducialRequest { layer: layer.to_string(), at, diameter: 1.0, name: Some("FID".to_string()) })
            .expect("the fiducial is added");
    }
    let listed = list_fiducials(handle).expect("the fiducials are listed");
    assert_eq!(listed["count"], 4);
    assert_eq!(listed["items"][0]["layer"], "F.Fid");
    assert_eq!(listed["items"][0]["name"], "FID");
    assert_eq!(summary(handle).expect("the board is listed")["counts"]["fiducials"], 4);

    let report = fiducial_report(handle).expect("the fiducial report is built");
    assert_eq!(report["units"], "mm");
    assert_eq!(report["count"], 4);
    assert_eq!(report["front"], 2);
    assert_eq!(report["back"], 2);
    assert_eq!(report["status"], "pass");
    assert_eq!(report["items"][0]["at"], json!([10.0, 10.0]));
    assert_eq!(report["items"][0]["diameter"], 1.0);
    assert_eq!(report["items"][0]["edgeGap"], 9.5);
    assert!(report["issues"].as_array().expect("issues is an array").is_empty());

    undo(handle).expect("the last fiducial add is undone");
    assert_eq!(list_fiducials(handle).expect("the fiducials are listed")["count"], 3);
    let warn = fiducial_report(handle).expect("the fiducial report is built");
    assert_eq!(warn["status"], "warn", "one mark on B.Fid warns below the two-per-side preference");
    redo(handle).expect("the fiducial add is redone");
    assert_eq!(list_fiducials(handle).expect("the fiducials are listed")["count"], 4);

    let deleted = delete_fiducial(handle, "fiducial-1").expect("the fiducial is deleted");
    assert_eq!(deleted["deleted"], "fiducial-1");
    let missing = delete_fiducial(handle, "fiducial-1").expect_err("the fiducial is gone");
    assert_eq!(missing.code, "object/not-found");

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    assert_eq!(list_fiducials(reopened).expect("the fiducials are listed")["count"], 3);
    close(reopened).expect("the board closes");
}

#[test]
fn fiducial_add_rejects_bad_layer_geometry_and_outline() {
    let handle = created();
    let no_outline =
        add_fiducial(handle, AddFiducialRequest { layer: "F.Fid".to_string(), at: [1.0, 1.0], diameter: 1.0, name: None })
            .expect_err("a fiducial needs an outline");
    assert_eq!(no_outline.code, "session/argument-invalid");
    add_full_board(handle);

    let wrong_layer =
        add_fiducial(handle, AddFiducialRequest { layer: "F.Cu".to_string(), at: [10.0, 10.0], diameter: 1.0, name: None })
            .expect_err("only fiducial layers are accepted");
    assert_eq!(wrong_layer.code, "session/argument-invalid");
    let zero_diameter =
        add_fiducial(handle, AddFiducialRequest { layer: "F.Fid".to_string(), at: [10.0, 10.0], diameter: 0.0, name: None })
            .expect_err("the diameter must be positive");
    assert_eq!(zero_diameter.code, "session/argument-invalid");
    let outside =
        add_fiducial(handle, AddFiducialRequest { layer: "F.Fid".to_string(), at: [120.0, 40.0], diameter: 1.0, name: None })
            .expect_err("the centre must sit inside the outline");
    assert_eq!(outside.code, "session/argument-invalid");
    let blank = add_fiducial(
        handle,
        AddFiducialRequest { layer: "F.Fid".to_string(), at: [10.0, 10.0], diameter: 1.0, name: Some(" ".to_string()) },
    )
    .expect_err("a name must not be blank");
    assert_eq!(blank.code, "session/argument-invalid");
    assert_eq!(list_fiducials(handle).expect("the fiducials are listed")["count"], 0);
    close(handle).expect("the board closes");
}

#[test]
fn fiducial_report_warns_on_edge_clearance_and_sparse_sides() {
    let handle = created();
    add_full_board(handle);
    add_fiducial(handle, AddFiducialRequest { layer: "F.Fid".to_string(), at: [0.2, 40.0], diameter: 1.0, name: None })
        .expect("the fiducial is added");
    let report = fiducial_report(handle).expect("the fiducial report is built");
    assert_eq!(report["status"], "warn");
    assert_eq!(report["edgeGap"], json!(null), "the gap lives on the item");
    let item = &report["items"][0];
    assert_eq!(item["edgeGap"], -0.3, "half the mark hangs off the edge");
    let codes: Vec<&str> =
        report["issues"].as_array().expect("issues is an array").iter().map(|issue| issue["code"].as_str().unwrap()).collect();
    assert!(codes.contains(&"fiducial/edge-clearance"), "the edge gap warns: {codes:?}");
    assert!(codes.contains(&"fiducial/total-count"), "one mark warns below three: {codes:?}");
    assert!(codes.contains(&"fiducial/side-count"), "one mark on a side warns below two: {codes:?}");
    close(handle).expect("the board closes");
}

#[test]
fn testpoints_requires_a_declared_probe_for_every_connectable_net() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "SIGNAL".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "POWER".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN32".to_string()),
            x: Some(30.0),
            y: Some(20.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 1.0,
            y: 1.0,
            w: 0.8,
            h: 0.8,
            net: Some("SIGNAL".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    add_via(
        handle,
        AddViaRequest { net: Some("POWER".to_string()), at: [20.0, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");

    add_test_point(handle, AddTestPointRequest { target: "fp-1:1".to_string() }).expect("the pad is a test point");
    add_test_point(handle, AddTestPointRequest { target: "via-1".to_string() }).expect("the via is a test point");
    let covered = inspect(handle, Some(vec!["testpoints".to_string()])).expect("the check runs");
    assert_eq!(codes(&covered), Vec::<&str>::new(), "a declared pad or via test point covers its net");
    assert_eq!(covered["passed"], true);

    delete_test_point(handle, "testpoint-1").expect("the pad test point is deleted");
    delete_test_point(handle, "testpoint-2").expect("the via test point is deleted");
    let missing = inspect(handle, Some(vec!["testpoints".to_string()])).expect("the check runs again");
    assert_eq!(codes(&missing), ["testpoints/net-not-covered", "testpoints/net-not-covered"]);
    assert_eq!(missing["passed"], true, "a missing test point is a warning");
    let issues = missing["issues"].as_array().expect("issues is an array");
    assert_eq!(issues[0]["objectId"], "net-1");
    assert!(issues[0]["message"].as_str().expect("a message").contains("SIGNAL"));
    assert_eq!(issues[1]["objectId"], "net-2");
    assert!(issues[1]["message"].as_str().expect("a message").contains("POWER"));
    close(handle).expect("the board closes");

    let unconnected = created();
    add_net(unconnected, AddNetRequest { name: "GHOST".to_string() }).expect("the net is declared");
    let report = inspect(unconnected, Some(vec!["testpoints".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["testpoints/net-unconnected"]);
    assert_eq!(report["issues"][0]["severity"], "error");
    assert_eq!(report["issues"][0]["objectId"], "net-1");
    assert!(report["issues"][0]["message"].as_str().expect("a message").contains("GHOST"));
    assert_eq!(report["passed"], false, "a net without a physical contact point is an error");
    close(unconnected).expect("the board closes");

    let empty = created();
    let report = inspect(empty, Some(vec!["testpoints".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a board with no nets has nothing to cover");
    assert_eq!(report["passed"], true);
    close(empty).expect("the board closes");
}

#[test]
fn ipc_d_356_lists_pads_and_vias_in_fixed_width_records() {
    let directory = TempDir::new();
    let handle = created();
    add_full_board(handle);
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 1.0,
            y: 1.0,
            w: 0.8,
            h: 0.8,
            net: Some("VCC".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");

    let path = directory.join("board.d356");
    let report = export(handle, "ipc_d_356", &path).expect("the IPC-D-356 export writes");
    assert_eq!(report["format"], "ipc_d_356");
    let text = fs::read_to_string(&path).expect("the netlist is readable");
    assert_eq!(text.lines().next().map(str::trim_end), Some("C  IPC-D-356A"), "the file starts with its format comment");
    assert!(text.contains("P  UNITS  CUST 1"), "units are metric: {text}");
    assert!(text.contains("P  IMAGE  PRIMARY"), "the primary image is declared: {text}");
    assert!(text.contains("327VCC"), "the pad is a surface-mount record: {text}");
    assert!(text.contains("U1    -1"), "the reference and pin occupy their fields: {text}");
    assert!(text.contains("X+031000Y+021000"), "the pad centre is written in 0.001 mm: {text}");
    assert!(text.contains("317VCC"), "the through via is a plated-hole record: {text}");
    assert!(text.contains("VIA   -1"), "a via names itself in the reference field: {text}");
    assert!(text.contains("X+020000Y+020000"), "the via centre is written in 0.001 mm: {text}");
    assert!(text.trim_end().ends_with("999"), "the file ends with the end-of-job code");
    for line in text.lines() {
        assert_eq!(line.len(), 80, "every record is 80 columns: {line}");
    }
    close(handle).expect("the board closes");
}

#[test]
fn keepouts_reject_copper_on_their_layer() {
    let directory = TempDir::new();
    let path = directory.join("keepouts.opcb");
    let handle = created();
    add_full_board(handle);
    let added = add_keepout(
        handle,
        AddKeepoutRequest {
            layer: Some("F.Cu".to_string()),
            points: vec![[15.0, 5.0], [25.0, 5.0], [25.0, 15.0], [15.0, 15.0]],
        },
    )
    .expect("the keepout is added");
    assert_eq!(added["object"]["id"], "keepout-1");
    assert_eq!(added["object"]["layer"], "F.Cu");
    assert_eq!(added["board"]["counts"]["keepouts"], 1);
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "B.Cu".to_string(),
            width: 0.25,
            from: [10.0, 10.0],
            to: [30.0, 10.0],
        },
    )
    .expect("the bottom track is placed");

    let flat = add_keepout(handle, AddKeepoutRequest { layer: None, points: vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]] })
        .expect_err("a keepout must enclose an area");
    assert_eq!(flat.code, "session/argument-invalid");
    let silk = add_keepout(
        handle,
        AddKeepoutRequest { layer: Some("F.SilkS".to_string()), points: vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]] },
    )
    .expect_err("a keepout needs a copper layer");
    assert_eq!(silk.code, "session/argument-invalid");

    let report = inspect(handle, Some(vec!["keepout".to_string()])).expect("the board is inspected");
    assert_eq!(report["passed"], false);
    let issues = report["issues"].as_array().expect("issues are listed");
    let tracks: Vec<&str> =
        issues.iter().filter_map(|issue| issue["objectId"].as_str()).filter(|id| id.starts_with("track-")).collect();
    assert!(tracks.contains(&"track-1"), "the front track enters the keepout; got {tracks:?}");
    assert!(!tracks.contains(&"track-2"), "a keepout on F.Cu ignores B.Cu copper; got {tracks:?}");

    let resized = update_keepout(
        handle,
        UpdateKeepoutRequest {
            id: "keepout-1".to_string(),
            layer: Some("*".to_string()),
            points: Some(vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]),
        },
    )
    .expect("the keepout is resized");
    assert_eq!(resized["object"]["layer"], "*");
    let empty = update_keepout(handle, UpdateKeepoutRequest { id: "keepout-1".to_string(), ..Default::default() })
        .expect_err("an update needs a field");
    assert_eq!(empty.code, "session/argument-invalid");
    let unknown = update_keepout(
        handle,
        UpdateKeepoutRequest { id: "keepout-9".to_string(), layer: Some("*".to_string()), ..Default::default() },
    )
    .expect_err("an unknown keepout is rejected");
    assert_eq!(unknown.code, "object/not-found");
    let keepouts = keepout_report(handle).expect("the keepout report is built");
    assert_eq!(keepouts["count"], 1);
    assert_eq!(keepouts["totalArea"], 100.0);
    assert_eq!(keepouts["items"][0]["vertices"], 4);
    assert_eq!(keepouts["items"][0]["perimeter"], 40.0);
    assert_eq!(keepouts["items"][0]["layer"], "*");

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["keepouts"], 1);
    delete_keepout(reopened, "keepout-1").expect("the keepout is deleted");
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["keepouts"], 0);
    close(reopened).expect("the board closes");
}

#[test]
fn stitching_vias_follow_the_inset_outline() {
    let directory = TempDir::new();
    let path = directory.join("stitch.opcb");
    let handle = created();
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the ground net is declared");
    let no_outline = stitch_vias(
        handle,
        AddStitchingRequest { net: "GND".to_string(), spacing: 20.0, inset: None, diameter: None, drill: None },
    )
    .expect_err("stitching needs an outline");
    assert_eq!(no_outline.code, "session/argument-invalid");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");
    let missing = stitch_vias(
        handle,
        AddStitchingRequest { net: "NOPE".to_string(), spacing: 20.0, inset: None, diameter: None, drill: None },
    )
    .expect_err("the net must be declared");
    assert_eq!(missing.code, "session/argument-invalid");

    let first = stitch_vias(
        handle,
        AddStitchingRequest { net: "GND".to_string(), spacing: 20.0, inset: Some(1.0), diameter: Some(0.6), drill: Some(0.3) },
    )
    .expect("the fence is generated");
    assert_eq!(first["object"]["vias"], 18, "352 mm of perimeter at 20 mm spacing gives 18 vias");
    assert_eq!(first["object"]["skipped"], 0);
    assert_eq!(first["board"]["counts"]["vias"], 18);
    let revision = first["revision"].as_u64().expect("a revision is reported");

    let second = stitch_vias(
        handle,
        AddStitchingRequest { net: "GND".to_string(), spacing: 20.0, inset: Some(1.0), diameter: Some(0.6), drill: Some(0.3) },
    )
    .expect("a repeat run is harmless");
    assert_eq!(second["object"]["vias"], 0);
    assert_eq!(second["object"]["skipped"], 18);
    assert_eq!(second["revision"].as_u64(), Some(revision), "a repeat run adds no revision");

    let invalid = stitch_vias(
        handle,
        AddStitchingRequest { net: "GND".to_string(), spacing: 20.0, inset: Some(1.0), diameter: Some(0.3), drill: Some(0.3) },
    )
    .expect_err("the drill must be smaller than the pad");
    assert_eq!(invalid.code, "session/argument-invalid");

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    assert_eq!(summary(reopened).expect("the board is listed")["counts"]["vias"], 18);
    close(reopened).expect("the board closes");
}

#[test]
fn save_requires_overwrite_and_leaves_no_temp_file() {
    let directory = TempDir::new();
    let path = directory.join("board.opcb");
    let handle = created();
    save(handle, &path, false).expect("the first save succeeds");
    let refused = save(handle, &path, false).expect_err("a second save needs overwrite");
    assert_eq!(refused.code, "format/invalid");
    save(handle, &path, true).expect("an overwrite succeeds");
    let names: Vec<String> = fs::read_dir(&directory.path)
        .expect("the temp directory is readable")
        .map(|entry| entry.expect("an entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["board.opcb"]);
    close(handle).expect("the board closes");
}

#[test]
fn documents_without_design_rules_open_with_defaults() {
    let directory = TempDir::new();
    let path = directory.join("legacy.opcb");
    let text = serde_json::to_string(&Document::new(crafted())).expect("the document serializes");
    let value: Value = serde_json::from_str(&text).expect("the document parses");
    assert!(value["document"].get("rules").is_none(), "default rules stay out of the written document");
    fs::write(&path, text).expect("the document is written");

    let handle = handle_of(&open(&path).expect("a document without a rules key opens"));
    let rules = store::read(handle, |board| board.rules).expect("the board is readable");
    assert_eq!(rules.hole_clearance, 0.25);
    assert_eq!(rules.copper_to_edge, 0.3);
    assert_eq!(rules.component_to_edge, 0.25);
    assert_eq!(rules.hole_to_edge, 0.3);
    assert_eq!(rules.solder_mask_expansion, 0.05);
    close(handle).expect("the board closes");
}

#[test]
fn design_rules_are_editable_persistent_and_drive_solder_mask() {
    let directory = TempDir::new();
    let handle = created();
    let initial = store::summary_of(handle).expect("the board is listed");
    assert_eq!(initial["rules"]["holeClearance"], 0.25);
    assert_eq!(initial["rules"]["solderMaskExpansion"], 0.05);

    let updated = set_rules(
        handle,
        SetRulesRequest { hole_clearance: Some(0.3), solder_mask_expansion: Some(0.1), ..Default::default() },
    )
    .expect("the rules are updated");
    assert_eq!(updated["revision"], 2);
    assert_eq!(updated["object"]["holeClearance"], 0.3);
    assert_eq!(updated["object"]["copperToEdge"], 0.3, "untouched rules keep their value");
    assert_eq!(updated["board"]["rules"]["solderMaskExpansion"], 0.1);

    let empty = set_rules(handle, SetRulesRequest::default()).expect_err("an empty update is rejected");
    assert_eq!(empty.code, "session/argument-invalid");
    let negative = set_rules(handle, SetRulesRequest { hole_to_edge: Some(-1.0), ..Default::default() })
        .expect_err("a negative rule is rejected");
    assert_eq!(negative.code, "session/argument-invalid");
    assert_eq!(store::summary_of(handle).expect("the board is listed")["revision"], 2, "rejected edits keep the revision");

    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN32".to_string()),
            x: Some(10.0),
            y: Some(10.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [40.0, 30.0] }).expect("the outline is set");

    let gbr = directory.join("gerber");
    export(handle, "gbr", &gbr).expect("Gerber export writes");
    let front = fs::read_to_string(gbr.join("F_Mask.gbr")).expect("the front mask is readable");
    assert!(front.contains("%ADD10R,1.200000X1.200000*%"), "the mask opening adds twice the expansion per side: {front}");
    assert!(front.contains("X10000000Y10000000D03*"), "the pad flashes on the front mask");
    let back = fs::read_to_string(gbr.join("B_Mask.gbr")).expect("the back mask is readable");
    assert!(!back.contains("D03*"), "a front pad opens no back mask and vias stay tented");

    let path = directory.join("rules.opcb");
    save(handle, &path, false).expect("the board saves");
    let reopened = handle_of(&open(&path).expect("the board reopens"));
    let rules = store::summary_of(reopened).expect("the reopened board is listed");
    assert_eq!(rules["rules"]["holeClearance"], 0.3);
    assert_eq!(rules["rules"]["solderMaskExpansion"], 0.1);
    close(reopened).expect("the reopened board closes");
    close(handle).expect("the board closes");

    let mut invalid = crafted();
    invalid.rules.solder_mask_expansion = -0.1;
    let invalid_path = directory.join("invalid-rules.opcb");
    fs::write(&invalid_path, serde_json::to_string(&Document::new(invalid)).expect("the document serializes"))
        .expect("the invalid document is written");
    assert_eq!(open(&invalid_path).expect_err("a negative solder-mask rule is rejected").code, "format/invalid");
}

#[test]
fn stackup_drives_impedance_and_width_conversion() {
    let directory = TempDir::new();
    let handle = created();
    assert!(summary(handle).expect("the board is listed")["stackup"].is_null(), "a new board has no stackup");

    let updated =
        set_stackup(handle, SetStackupRequest { copper_thickness: 0.035, dielectric_height: 0.2, dielectric_constant: 4.2 })
            .expect("the stackup is declared");
    assert_eq!(updated["revision"], 2);
    assert_eq!(updated["object"]["dielectricHeight"], 0.2);
    assert_eq!(updated["board"]["stackup"]["copperThickness"], 0.035);

    let microstrip = stackup_impedance(
        handle,
        ImpedanceRequest {
            structure: Some("microstrip".to_string()),
            width: 0.2,
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("microstrip impedance is computed");
    let value = microstrip["impedance"].as_f64().expect("a number");
    assert!((50.0..80.0).contains(&value), "microstrip impedance is plausible: {value}");
    assert_eq!(microstrip["structure"], "microstrip");
    assert_eq!(microstrip["dielectricHeight"], 0.2);

    let stripline = stackup_impedance(
        handle,
        ImpedanceRequest {
            structure: Some("stripline".to_string()),
            width: 0.2,
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("stripline impedance is computed");
    let stripline_value = stripline["impedance"].as_f64().expect("a number");
    assert!(stripline_value < value, "a stripline is lower impedance than a microstrip at the same width");

    let converted = stackup_width(
        handle,
        WidthRequest {
            structure: Some("microstrip".to_string()),
            impedance: 50.0,
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the width for 50 ohm is solved");
    let width = converted["width"].as_f64().expect("a number");
    assert!(width > 0.0 && width < 5.0, "the solved width is bounded: {width}");
    let round_trip = stackup_impedance(
        handle,
        ImpedanceRequest {
            structure: Some("microstrip".to_string()),
            width,
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the solved width is evaluated");
    let achieved = round_trip["impedance"].as_f64().expect("a number");
    assert!((achieved - 50.0).abs() < 0.01, "the solved width reaches the target: {achieved}");

    let path = directory.join("stackup.opcb");
    save(handle, &path, false).expect("the stackup board saves");
    let reopened = handle_of(&open(&path).expect("the stackup board reopens"));
    let stackup = summary(reopened).expect("the reopened board is listed")["stackup"].clone();
    assert_eq!(stackup["dielectricHeight"], 0.2);
    assert_eq!(stackup["dielectricConstant"], 4.2);
    close(reopened).expect("the reopened board closes");
    close(handle).expect("the board closes");

    let bare = created();
    let missing = stackup_impedance(
        bare,
        ImpedanceRequest {
            structure: None,
            width: 0.2,
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect_err("a board without a stackup cannot compute impedance");
    assert_eq!(missing.code, "session/argument-invalid");
    let invalid = stackup_impedance(
        bare,
        ImpedanceRequest {
            structure: Some("coplanar".to_string()),
            width: 0.2,
            gap: None,
            dielectric_height: Some(0.2),
            dielectric_constant: Some(4.2),
            copper_thickness: None,
        },
    )
    .expect_err("an unknown structure is rejected");
    assert_eq!(invalid.code, "session/argument-invalid");
    close(bare).expect("the bare board closes");
}

#[test]
fn impedance_table_suggests_widths_for_each_target() {
    let handle = created();
    set_stackup(handle, SetStackupRequest { copper_thickness: 0.035, dielectric_height: 0.2, dielectric_constant: 4.2 })
        .expect("the stackup is declared");

    let report = impedance_table(
        handle,
        ImpedanceTableRequest {
            structure: Some("microstrip".to_string()),
            targets: Some(vec![50.0, 75.0]),
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the table is produced");
    assert_eq!(report["revision"], 2);
    assert_eq!(report["units"], "mm");
    assert_eq!(report["structure"], "microstrip");
    assert_eq!(report["targets"], 2);
    let rows = report["rows"].as_array().expect("rows is an array");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["targetImpedance"], 50.0);
    let wide = rows[0]["width"].as_f64().expect("the first width is a number");
    let narrow = rows[1]["width"].as_f64().expect("the second width is a number");
    assert!(wide > narrow, "a lower target impedance needs a wider trace");
    for row in rows {
        let target = row["targetImpedance"].as_f64().expect("a target");
        let achieved = row["impedance"].as_f64().expect("an achieved impedance");
        assert!((achieved - target).abs() < 0.01, "the suggested width reaches {target} ohm: {achieved}");
    }

    let default = impedance_table(
        handle,
        ImpedanceTableRequest {
            structure: None,
            targets: None,
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the default target is reported");
    assert_eq!(default["targets"], 1);
    assert_eq!(default["rows"][0]["targetImpedance"], 50.0);
    close(handle).expect("the board closes");
}

#[test]
fn impedance_table_rejects_empty_and_unreachable_targets() {
    let handle = created();
    let request = |targets: Option<Vec<f64>>| ImpedanceTableRequest {
        structure: None,
        targets,
        gap: None,
        dielectric_height: Some(0.2),
        dielectric_constant: Some(4.2),
        copper_thickness: None,
    };

    let empty = impedance_table(handle, request(Some(Vec::new()))).expect_err("an empty target list is rejected");
    assert_eq!(empty.code, "session/argument-invalid");

    let unreachable = impedance_table(handle, request(Some(vec![1000.0]))).expect_err("an unreachable target is rejected");
    assert_eq!(unreachable.code, "session/argument-invalid");

    let negative = impedance_table(handle, request(Some(vec![-50.0]))).expect_err("a negative target is rejected");
    assert_eq!(negative.code, "session/argument-invalid");
    close(handle).expect("the board closes");
}

#[test]
fn impedance_report_lists_net_and_pair_rows_from_the_stored_stackup() {
    let handle = created();
    set_stackup(handle, SetStackupRequest { copper_thickness: 0.035, dielectric_height: 0.2, dielectric_constant: 4.2 })
        .expect("the stackup is declared");
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("the first net is declared");
    add_net(handle, AddNetRequest { name: "B".to_string() }).expect("the second net is declared");
    add_layer(handle, AddLayerRequest { name: "Inner1".to_string(), kind: Some("copper".to_string()), color: None })
        .expect("the inner copper layer is added");
    for (net, layer, from, to, width) in [
        ("A", "F.Cu", [10.0, 10.0], [30.0, 10.0], 0.25),
        ("A", "Inner1", [10.0, 11.0], [30.0, 11.0], 0.3),
        ("B", "B.Cu", [10.0, 12.0], [30.0, 12.0], 0.2),
    ] {
        add_track(handle, AddTrackRequest { net: Some(net.to_string()), layer: layer.to_string(), width, from, to })
            .expect("the track is routed");
    }
    add_differential_pair(
        handle,
        AddDifferentialPairRequest {
            positive: "A".to_string(),
            negative: "B".to_string(),
            width: 0.15,
            gap: 0.2,
            max_skew: 0.5,
        },
    )
    .expect("the pair is declared");

    let report = impedance_report(handle, ImpedanceReportRequest::default()).expect("the report is produced");
    assert_eq!(report["units"], "mm");
    assert_eq!(report["structure"], "auto");
    assert_eq!(report["copperThickness"], 0.035);
    assert_eq!(report["dielectricHeight"], 0.2);
    assert_eq!(report["dielectricConstant"], 4.2);
    assert_eq!(report["netCount"], 2, "the net without copper tracks does not appear");
    assert_eq!(report["pairCount"], 1);
    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets[0]["net"], "A");
    assert_eq!(nets[0]["tracks"], 2);
    assert_eq!(nets[0]["widthMin"], 0.25);
    assert_eq!(nets[0]["widthMax"], 0.3);
    assert_eq!(nets[0]["structures"], json!(["microstrip", "stripline"]), "auto follows the copper layer");

    // Each row matches the same value computed directly through the table and
    // impedance helpers on the stored stackup.
    let strip = stackup_impedance(
        handle,
        ImpedanceRequest {
            structure: Some("microstrip".to_string()),
            width: 0.25,
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the microstrip value is computed");
    let inner = stackup_impedance(
        handle,
        ImpedanceRequest {
            structure: Some("stripline".to_string()),
            width: 0.3,
            gap: None,
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the stripline value is computed");
    assert_eq!(nets[0]["impedanceMax"], strip["impedance"], "the microstrip track sets the maximum");
    assert_eq!(nets[0]["impedanceMin"], inner["impedance"], "the inner-layer track sets the minimum");
    let pair_direct = stackup_impedance(
        handle,
        ImpedanceRequest {
            structure: Some("diffMicrostrip".to_string()),
            width: 0.15,
            gap: Some(0.2),
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the pair value is computed");
    let pair = &report["pairs"][0];
    assert_eq!(pair["id"], "dp-1");
    assert_eq!(pair["positive"], "A");
    assert_eq!(pair["negative"], "B");
    assert_eq!(pair["structure"], "diffMicrostrip");
    assert_eq!(pair["width"], 0.15);
    assert_eq!(pair["gap"], 0.2);
    assert_eq!(pair["impedance"], pair_direct["impedance"], "the pair row matches the direct calculation");
    assert_eq!(pair["oddImpedance"], pair_direct["oddImpedance"]);
    assert_eq!(pair["evenImpedance"], pair_direct["evenImpedance"]);

    let again = impedance_report(handle, ImpedanceReportRequest::default()).expect("the report is produced again");
    assert_eq!(again, report, "the report is deterministic");
    close(handle).expect("the board closes");
}

#[test]
fn impedance_report_applies_a_declared_structure_to_every_row() {
    let handle = created();
    set_stackup(handle, SetStackupRequest { copper_thickness: 0.035, dielectric_height: 0.2, dielectric_constant: 4.2 })
        .expect("the stackup is declared");
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("the net is declared");
    add_layer(handle, AddLayerRequest { name: "Inner1".to_string(), kind: Some("copper".to_string()), color: None })
        .expect("the inner copper layer is added");
    for (layer, from, to, width) in [("F.Cu", [10.0, 10.0], [30.0, 10.0], 0.25), ("Inner1", [10.0, 11.0], [30.0, 11.0], 0.3)] {
        add_track(handle, AddTrackRequest { net: Some("A".to_string()), layer: layer.to_string(), width, from, to })
            .expect("the track is routed");
    }
    add_differential_pair(
        handle,
        AddDifferentialPairRequest {
            positive: "A".to_string(),
            negative: "B".to_string(),
            width: 0.15,
            gap: 0.2,
            max_skew: 0.5,
        },
    )
    .expect_err("the negative half is not declared yet");

    for structure in ["stripline", "diffStripline"] {
        let report =
            impedance_report(handle, ImpedanceReportRequest { structure: Some(structure.to_string()), ..Default::default() })
                .expect("the report is produced");
        assert_eq!(report["structure"], structure);
        let nets = report["nets"].as_array().expect("nets is an array");
        assert_eq!(nets[0]["structures"], json!(["stripline"]), "net rows use the single-ended geometry");
    }
    close(handle).expect("the board closes");
}

#[test]
fn impedance_report_requires_a_stackup_or_overrides() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("the net is declared");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("A".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.0, 10.0],
            to: [30.0, 10.0],
        },
    )
    .expect("the track is routed");

    let error = impedance_report(handle, ImpedanceReportRequest::default()).expect_err("a board without a stackup is rejected");
    assert_eq!(error.code, "session/argument-invalid");

    let report = impedance_report(
        handle,
        ImpedanceReportRequest {
            structure: Some("auto".to_string()),
            dielectric_height: Some(0.2),
            dielectric_constant: Some(4.2),
            copper_thickness: None,
        },
    )
    .expect("explicit values replace the missing stackup");
    assert_eq!(report["structure"], "auto");
    assert_eq!(report["dielectricHeight"], 0.2);
    assert_eq!(report["dielectricConstant"], 4.2);
    assert_eq!(report["netCount"], 1);
    close(handle).expect("the board closes");
}

#[test]
fn differential_impedance_reports_modes_and_solves_width() {
    let handle = created();
    set_stackup(handle, SetStackupRequest { copper_thickness: 0.035, dielectric_height: 0.2, dielectric_constant: 4.2 })
        .expect("the stackup is declared");
    let request = |structure: &str, width: f64, gap: Option<f64>| ImpedanceRequest {
        structure: Some(structure.to_string()),
        width,
        gap,
        dielectric_height: None,
        dielectric_constant: None,
        copper_thickness: None,
    };

    let pair = stackup_impedance(handle, request("diffMicrostrip", 0.15, Some(0.2))).expect("the pair is evaluated");
    assert_eq!(pair["structure"], "diffMicrostrip");
    assert_eq!(pair["gap"], 0.2);
    let odd = pair["oddImpedance"].as_f64().expect("an odd-mode impedance");
    let even = pair["evenImpedance"].as_f64().expect("an even-mode impedance");
    let differential = pair["impedance"].as_f64().expect("a differential impedance");
    assert!(even > odd, "even mode is higher than odd mode: {even} > {odd}");
    assert!((differential - 2.0 * odd).abs() < 1e-6, "the differential value is twice the odd mode");
    assert!((differential - 122.0).abs() < 5.0, "the closed form stays in a plausible range: {differential}");

    let tight = stackup_impedance(handle, request("diffMicrostrip", 0.15, Some(0.1))).expect("the tight pair is evaluated");
    let tight_differential = tight["impedance"].as_f64().expect("a differential impedance");
    assert!(tight_differential < differential, "a tighter gap couples more and lowers the differential impedance");

    let stripline =
        stackup_impedance(handle, request("diffStripline", 0.15, Some(0.2))).expect("the stripline pair is evaluated");
    let stripline_differential = stripline["impedance"].as_f64().expect("a differential impedance");
    assert!(stripline_differential < differential, "a stripline pair is lower impedance than a microstrip pair");

    let solved = stackup_width(
        handle,
        WidthRequest {
            structure: Some("diffMicrostrip".to_string()),
            impedance: 90.0,
            gap: Some(0.2),
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the pair width is solved");
    assert_eq!(solved["gap"], 0.2);
    let width = solved["width"].as_f64().expect("a solved width");
    assert!(width > 0.0 && width < 5.0, "the solved width is bounded: {width}");
    let round_trip =
        stackup_impedance(handle, request("diffMicrostrip", width, Some(0.2))).expect("the solved width is evaluated");
    let achieved = round_trip["impedance"].as_f64().expect("a differential impedance");
    assert!((achieved - 90.0).abs() < 0.02, "the solved width reaches the differential target: {achieved}");

    let table = impedance_table(
        handle,
        ImpedanceTableRequest {
            structure: Some("diffMicrostrip".to_string()),
            targets: Some(vec![80.0, 100.0]),
            gap: Some(0.2),
            dielectric_height: None,
            dielectric_constant: None,
            copper_thickness: None,
        },
    )
    .expect("the differential table is produced");
    assert_eq!(table["gap"], 0.2);
    let rows = table["rows"].as_array().expect("rows is an array");
    assert_eq!(rows.len(), 2);
    assert!(rows[0]["oddImpedance"].is_number() && rows[0]["evenImpedance"].is_number());
    assert!(rows[0]["width"].as_f64().expect("a width") > rows[1]["width"].as_f64().expect("a width"));

    let missing = stackup_impedance(handle, request("diffMicrostrip", 0.15, None)).expect_err("a pair needs a gap");
    assert_eq!(missing.code, "session/argument-invalid");
    let stray = stackup_impedance(handle, request("microstrip", 0.15, Some(0.2))).expect_err("a single trace takes no gap");
    assert_eq!(stray.code, "session/argument-invalid");
    let zero_gap = stackup_impedance(handle, request("diffMicrostrip", 0.15, Some(0.0))).expect_err("a zero gap is rejected");
    assert_eq!(zero_gap.code, "session/argument-invalid");
    close(handle).expect("the board closes");
}

#[test]
fn waivers_and_severity_overrides_change_the_verdict_and_persist() {
    let directory = TempDir::new();
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.05,
            from: [1.0, 1.0],
            to: [2.0, 1.0],
        },
    )
    .expect("the narrow track is placed");

    let checks = Some(vec!["track-width".to_string()]);
    let report = inspect(handle, checks.clone()).expect("the board is inspected");
    assert_eq!(report["passed"], false);
    assert_eq!(report["active"], 1);
    assert_eq!(report["waived"], 0);
    assert_eq!(report["issues"][0]["code"], "drc/track-width-too-small");
    assert_eq!(report["issues"][0]["severity"], "error");

    set_severity(handle, SetSeverityRequest { code: "drc/track-width-too-small".to_string(), severity: "info".to_string() })
        .expect("the severity is overridden");
    let report = inspect(handle, checks.clone()).expect("the board is inspected");
    assert_eq!(report["passed"], true, "an info finding does not block the board");
    assert_eq!(report["issues"][0]["severity"], "info");
    delete_severity(handle, "drc/track-width-too-small").expect("the override is removed");
    assert_eq!(inspect(handle, checks.clone()).expect("the board is inspected")["passed"], false);

    let waiver = add_waiver(
        handle,
        AddWaiverRequest {
            code: "drc/track-width-too-small".to_string(),
            object: Some("track-1".to_string()),
            reason: Some("known stub".to_string()),
        },
    )
    .expect("the finding is waived");
    assert_eq!(waiver["object"]["id"], "waiver-1");
    let report = inspect(handle, checks.clone()).expect("the board is inspected");
    assert_eq!(report["passed"], true, "a waived error does not block the board");
    assert_eq!(report["waived"], 1);
    assert_eq!(report["active"], 0);
    assert_eq!(report["issues"][0]["waived"], true);
    assert_eq!(report["issues"][0]["waiverId"], "waiver-1");
    assert_eq!(report["issues"][0]["waiverReason"], "known stub");

    let duplicate = add_waiver(
        handle,
        AddWaiverRequest { code: "drc/track-width-too-small".to_string(), object: Some("track-1".to_string()), reason: None },
    )
    .expect_err("the same code and object cannot be waived twice");
    assert_eq!(duplicate.code, "object/duplicate");
    let bad = set_severity(handle, SetSeverityRequest { code: "drc/x".to_string(), severity: "fatal".to_string() })
        .expect_err("an unknown severity is rejected");
    assert_eq!(bad.code, "session/argument-invalid");

    set_severity(handle, SetSeverityRequest { code: "drc/dangling-copper".to_string(), severity: "error".to_string() })
        .expect("a second override is stored");
    let summary = store::summary_of(handle).expect("the board is listed");
    assert_eq!(summary["counts"]["waivers"], 1);
    assert_eq!(summary["waivers"][0]["code"], "drc/track-width-too-small");
    assert_eq!(summary["severityOverrides"]["drc/dangling-copper"], "error");

    let path = directory.join("policy.opcb");
    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the board reopens"));
    let summary = store::summary_of(reopened).expect("the reopened board is listed");
    assert_eq!(summary["waivers"][0]["id"], "waiver-1");
    assert_eq!(summary["severityOverrides"]["drc/dangling-copper"], "error");
    assert_eq!(inspect(reopened, checks.clone()).expect("the reopened board is inspected")["passed"], true);

    assert_eq!(delete_waiver(reopened, "waiver-1").expect("the waiver is removed")["deleted"], "waiver-1");
    assert_eq!(inspect(reopened, checks).expect("the board is inspected")["passed"], false);
    close(reopened).expect("the reopened board closes");

    let mut invalid = crafted();
    invalid.severity_overrides.insert("drc/x".to_string(), "fatal".to_string());
    let invalid_path = directory.join("invalid-severity.opcb");
    fs::write(&invalid_path, serde_json::to_string(&Document::new(invalid)).expect("the document serializes"))
        .expect("the invalid document is written");
    assert_eq!(open(&invalid_path).expect_err("an unknown persisted severity is rejected").code, "format/invalid");
}

#[test]
fn open_rejects_wrong_tag_version_and_broken_documents() {
    let directory = TempDir::new();
    let mut board = crafted();
    board.nets.push(Net { id: "net-1".to_string(), name: "VCC".to_string() });
    board.footprints.push(Footprint {
        id: "fp-1".to_string(),
        ref_: "U1".to_string(),
        value: "MCU".to_string(),
        footprint: "QFN32".to_string(),
        x: 0.0,
        y: 0.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: Vec::new(),
    });

    let write = |name: &str, document: &Document| {
        let path = directory.join(name);
        fs::write(&path, serde_json::to_string(document).expect("the document serializes")).expect("the document is written");
        path
    };

    let tag_path = write("tag.opcb", &Document { format: "other".to_string(), version: 1, document: board.clone() });
    assert_eq!(open(&tag_path).expect_err("a wrong tag is rejected").code, "format/invalid");

    let version_path =
        write("version.opcb", &Document { format: "open-pcb-board".to_string(), version: 2, document: board.clone() });
    assert_eq!(open(&version_path).expect_err("a newer version is rejected").code, "format/unsupported-version");

    let mut duplicate = board.clone();
    duplicate.footprints.push(duplicate.footprints[0].clone());
    let duplicate_path = write("duplicate.opcb", &Document::new(duplicate));
    assert_eq!(open(&duplicate_path).expect_err("a duplicate reference is rejected").code, "format/invalid");

    let mut missing_layer = board.clone();
    missing_layer.footprints[0].layer = "X.Cu".to_string();
    let missing_layer_path = write("layer.opcb", &Document::new(missing_layer));
    assert_eq!(open(&missing_layer_path).expect_err("a missing layer is rejected").code, "format/invalid");

    let mut missing_net = board.clone();
    missing_net.footprints[0].pads.push(Pad {
        number: "1".to_string(),
        x: 0.0,
        y: 0.0,
        w: 0.5,
        h: 0.5,
        net: Some("GND".to_string()),
        drill: None,
    });
    let missing_net_path = write("net.opcb", &Document::new(missing_net));
    assert_eq!(open(&missing_net_path).expect_err("an undeclared pad net is rejected").code, "format/invalid");

    let mut negative = board;
    negative.tracks.push(Track {
        id: "track-1".to_string(),
        net: Some("VCC".to_string()),
        layer: "F.Cu".to_string(),
        width: -1.0,
        from: [0.0, 0.0],
        to: [1.0, 1.0],
        teardrop: false,
    });
    let negative_path = write("negative.opcb", &Document::new(negative));
    assert_eq!(open(&negative_path).expect_err("a negative width is rejected").code, "format/invalid");

    let garbage = directory.join("garbage.opcb");
    fs::write(&garbage, "{not json").expect("garbage is written");
    assert_eq!(open(&garbage).expect_err("garbage is rejected").code, "format/invalid");
    assert_eq!(open(&directory.join("missing.opcb")).expect_err("a missing file is rejected").code, "format/invalid");
}

#[test]
fn drc_flags_duplicate_references_and_stackup() {
    let mut board = crafted();
    for reference in ["U1", "U1"] {
        board.footprints.push(Footprint {
            id: format!("fp-{}", board.footprints.len() + 1),
            ref_: reference.to_string(),
            value: String::new(),
            footprint: String::new(),
            x: 0.0,
            y: 0.0,
            rotation: 0.0,
            layer: "F.Cu".to_string(),
            pads: Vec::new(),
        });
    }
    board.layers.retain(|layer| layer.id != "B.Cu");
    let handle = store::insert(board);

    let report = inspect(handle, Some(vec!["drc".to_string(), "stackup".to_string()])).expect("checks run");
    assert_eq!(report["passed"], false);
    let found = codes(&report);
    assert_eq!(found.iter().filter(|code| **code == "drc/duplicate-reference").count(), 2);
    assert!(found.contains(&"drc/stackup-missing-layer"));
    assert_eq!(report["issues"][0]["objectId"], "fp-1");
    assert_eq!(report["issues"][0]["layerId"], "F.Cu");
    close(handle).expect("the board closes");
}

#[test]
fn drc_flags_track_rules_and_connectivity() {
    let mut board = crafted();
    board.nets.push(Net { id: "net-1".to_string(), name: "VCC".to_string() });
    board.tracks.push(Track {
        id: "track-missing".to_string(),
        net: Some("VCC".to_string()),
        layer: "X.Cu".to_string(),
        width: 0.25,
        from: [0.0, 0.0],
        to: [1.0, 0.0],
        teardrop: false,
    });
    board.tracks.push(Track {
        id: "track-silk".to_string(),
        net: None,
        layer: "F.SilkS".to_string(),
        width: 0.25,
        from: [0.0, 5.0],
        to: [1.0, 5.0],
        teardrop: false,
    });
    board.tracks.push(Track {
        id: "track-thin".to_string(),
        net: Some("GND".to_string()),
        layer: "F.Cu".to_string(),
        width: 0.1,
        from: [0.0, 10.0],
        to: [1.0, 10.0],
        teardrop: false,
    });
    let handle = store::insert(board);

    let report = inspect(handle, Some(vec!["drc".to_string(), "connectivity".to_string()])).expect("checks run");
    let found = codes(&report);
    for expected in ["drc/track-layer-missing", "drc/track-width-too-small", "drc/track-net-undeclared"] {
        assert!(found.contains(&expected), "{expected} is reported; got {found:?}");
    }
    assert!(
        !found.contains(&"drc/track-layer-not-copper"),
        "a silkscreen legend track is not a copper rule violation; got {found:?}"
    );
    assert_eq!(report["passed"], false);
    close(handle).expect("the board closes");
}

#[test]
fn drc_flags_via_rules_and_netlist() {
    let mut board = crafted();
    board.vias.push(Via {
        id: "via-drill".to_string(),
        net: Some("GND".to_string()),
        at: [0.0, 0.0],
        diameter: 0.6,
        drill: 0.0,
        from_layer: None,
        to_layer: None,
        micro: false,
        back_drill: None,
    });
    board.vias.push(Via {
        id: "via-ring".to_string(),
        net: None,
        at: [5.0, 0.0],
        diameter: 0.3,
        drill: 0.3,
        from_layer: None,
        to_layer: None,
        micro: false,
        back_drill: None,
    });
    board.footprints.push(Footprint {
        id: "fp-1".to_string(),
        ref_: "U1".to_string(),
        value: String::new(),
        footprint: String::new(),
        x: 0.0,
        y: 0.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 0.5, h: 0.5, net: Some("GND".to_string()), drill: None }],
    });
    board.nets.push(Net { id: "net-1".to_string(), name: "VCC".to_string() });
    board.nets.push(Net { id: "net-1".to_string(), name: "VCC".to_string() });
    let handle = store::insert(board);

    let report = inspect(handle, Some(vec!["connectivity".to_string(), "annular_rings".to_string(), "netlist".to_string()]))
        .expect("checks run");
    let found = codes(&report);
    for expected in [
        "drc/via-net-undeclared",
        "drc/via-annular-ring-invalid",
        "drc/pad-net-undeclared",
        "drc/duplicate-net-id",
        "drc/duplicate-net-name",
    ] {
        assert!(found.contains(&expected), "{expected} is reported; got {found:?}");
    }
    assert_eq!(report["passed"], false);
    close(handle).expect("the board closes");
}

#[test]
fn drc_flags_clearance_between_tracks() {
    let mut board = crafted();
    board.nets.push(Net { id: "net-1".to_string(), name: "A".to_string() });
    board.nets.push(Net { id: "net-2".to_string(), name: "B".to_string() });
    board.tracks.push(Track {
        id: "track-a".to_string(),
        net: Some("A".to_string()),
        layer: "F.Cu".to_string(),
        width: 0.25,
        from: [0.0, 0.0],
        to: [10.0, 0.0],
        teardrop: false,
    });
    board.tracks.push(Track {
        id: "track-b".to_string(),
        net: Some("B".to_string()),
        layer: "F.Cu".to_string(),
        width: 0.25,
        from: [0.0, 0.25],
        to: [5.0, 0.25],
        teardrop: false,
    });
    board.tracks.push(Track {
        id: "track-a-extension".to_string(),
        net: Some("A".to_string()),
        layer: "F.Cu".to_string(),
        width: 0.25,
        from: [10.0, 0.0],
        to: [20.0, 0.0],
        teardrop: false,
    });
    board.tracks.push(Track {
        id: "track-b-ok".to_string(),
        net: Some("B".to_string()),
        layer: "F.Cu".to_string(),
        width: 0.25,
        from: [0.0, 10.0],
        to: [10.0, 10.0],
        teardrop: false,
    });
    let handle = store::insert(board);

    let report = inspect(handle, Some(vec!["clearance".to_string()])).expect("clearance runs");
    let found = codes(&report);
    assert_eq!(found, ["drc/clearance-violation"]);
    assert_eq!(report["issues"][0]["layerId"], "F.Cu");
    assert_eq!(report["issues"][0]["objectId"], "track-a");
    close(handle).expect("the board closes");
}

#[test]
fn exports_write_real_content_and_reject_unimplemented_targets() {
    let directory = TempDir::new();
    let handle = created();
    add_full_board(handle);
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("R1".to_string()),
            value: Some("10k".to_string()),
            footprint: Some("0402".to_string()),
            x: Some(10.0),
            y: Some(5.0),
            rotation: Some(90.0),
            layer: Some("B.Cu".to_string()),
        },
    )
    .expect("the resistor is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");

    let drill = directory.join("blinker.drl");
    let report = export(handle, "drl", &drill).expect("drill export writes");
    assert_eq!(report["format"], "drl");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    for marker in ["M48", "METRIC,TZ", "T1C0.300", "X20.000Y20.000", "M30"] {
        assert!(text.contains(marker), "drill file contains {marker}");
    }

    let bom = directory.join("blinker.bom.csv");
    export(handle, "bom", &bom).expect("BOM export writes");
    let text = fs::read_to_string(&bom).expect("the BOM is readable");
    assert!(text.starts_with("Quantity,Value,Footprint,Designators\n"));
    assert!(text.contains("1,MCU,QFN32,U1"));
    assert!(text.contains("1,10k,0402,R1"));

    let pos = directory.join("blinker.pos.csv");
    export(handle, "pos", &pos).expect("position export writes");
    let text = fs::read_to_string(&pos).expect("the position file is readable");
    assert!(text.starts_with("Designator,Val,Package,Mid X,Mid Y,Rotation,Side,Layer\n"));
    assert!(text.contains("U1,MCU,QFN32,30.000,20.000,0.000,Top,F.Cu"));
    assert!(text.contains("R1,10k,0402,10.000,5.000,90.000,Bottom,B.Cu"));

    let placement = directory.join("blinker.placement.json");
    let report = export(handle, "placement", &placement).expect("placement export writes");
    assert_eq!(report["format"], "placement");
    let document: Value = serde_json::from_str(&fs::read_to_string(&placement).expect("the placement file is readable"))
        .expect("the placement file is JSON");
    assert_eq!(document["units"], "mm");
    let records = document["placements"].as_array().expect("placements is an array");
    assert_eq!(records.len(), 2, "one record per footprint");
    assert_eq!(records[0]["ref"], "R1", "records are sorted by reference");
    assert_eq!(records[0]["value"], "10k");
    assert_eq!(records[0]["footprint"], "0402");
    assert_eq!(records[0]["x"], 10.0);
    assert_eq!(records[0]["y"], 5.0);
    assert_eq!(records[0]["rotation"], 90.0);
    assert_eq!(records[0]["layer"], "B.Cu");
    assert_eq!(records[0]["side"], "Bottom");
    assert_eq!(records[1]["ref"], "U1");
    assert_eq!(records[1]["pads"], 1, "the pad count comes from the document");

    let svg = directory.join("blinker.svg");
    export(handle, "svg", &svg).expect("SVG export writes");
    let text = fs::read_to_string(&svg).expect("the SVG is readable");
    for marker in ["<svg", "<line", "<path", "<ellipse", "#c87533", "#d9b44a", "stroke-width"] {
        assert!(text.contains(marker), "SVG contains {marker}");
    }
    assert!(text.contains("width=\""), "the SVG declares its canvas width");
    assert!(text.contains("height=\""), "the SVG declares its canvas height");

    let gbr = directory.join("gerber");
    let report = export(handle, "gbr", &gbr).expect("Gerber export writes a directory");
    assert_eq!(report["format"], "gbr");
    assert_eq!(report["path"], gbr.display().to_string());
    let artifacts = report["artifacts"].as_array().expect("artifacts is an array");
    assert_eq!(artifacts.len(), 9);
    let names: Vec<&str> = artifacts.iter().map(|item| item["name"].as_str().expect("a name")).collect();
    assert_eq!(
        names,
        [
            "F_Cu.gbr",
            "B_Cu.gbr",
            "F_SilkS.gbr",
            "B_SilkS.gbr",
            "F_Mask.gbr",
            "B_Mask.gbr",
            "F_Paste.gbr",
            "B_Paste.gbr",
            "Edge_Cuts.gbr"
        ]
    );
    for name in names {
        let path = gbr.join(name);
        assert!(path.is_file(), "{name} is written");
        let text = fs::read_to_string(&path).expect("the Gerber file is readable");
        assert!(text.contains("%FSLAX46Y46*%"), "{name} declares the coordinate format");
        assert!(text.contains("%MOMM*%"), "{name} declares millimetres");
        assert!(text.contains("M02*"), "{name} ends with M02");
    }
    let copper = fs::read_to_string(gbr.join("F_Cu.gbr")).expect("the copper layer is readable");
    assert!(copper.contains("%TF.FileFunction,Copper,L1,Top*%"), "the top copper function is declared");
    assert!(copper.contains("%TA.AperFunction,Conductor*%"), "copper apertures declare their function");
    assert!(copper.contains("%ADD10C,0.250000*%"), "the track aperture is sized to its width");
    assert!(copper.contains("D01*"), "the track is a draw");
    assert!(copper.contains("D03*"), "the via is a flash");
    let bottom = fs::read_to_string(gbr.join("B_Cu.gbr")).expect("the bottom copper layer is readable");
    assert!(bottom.contains("%TF.FileFunction,Copper,L2,Bot*%"), "the bottom copper function is declared");
    assert!(bottom.contains("D03*"), "a through via is flashed on both outer layers");
    assert!(
        fs::read_to_string(gbr.join("F_SilkS.gbr"))
            .expect("the front silkscreen is readable")
            .contains("%TF.FileFunction,Silkscreen,Top*%")
    );
    assert!(
        fs::read_to_string(gbr.join("B_Mask.gbr"))
            .expect("the back mask is readable")
            .contains("%TF.FileFunction,Soldermask,Bot*%")
    );
    let mask = fs::read_to_string(gbr.join("F_Mask.gbr")).expect("the front solder mask is readable");
    assert!(mask.contains("%ADD10R,0.600000X0.600000*%"), "the opening is the pad plus twice the 0.05mm expansion");
    assert!(mask.contains("X30000000Y20000000D03*"), "the front pad opens the front mask");
    let back_mask = fs::read_to_string(gbr.join("B_Mask.gbr")).expect("the back solder mask is readable");
    assert!(!back_mask.contains("D03*"), "a footprint without pads opens no mask and vias stay tented");
    let paste = fs::read_to_string(gbr.join("F_Paste.gbr")).expect("the front paste layer is readable");
    assert!(paste.contains("%TF.FileFunction,Paste,Top*%"), "the paste function is declared");
    assert!(paste.contains("%TA.AperFunction,Paste*%"), "paste apertures declare their function");
    assert!(paste.contains("%ADD10R,0.500000X0.500000*%"), "the stencil window is the pad size, without mask expansion");
    assert!(paste.contains("X30000000Y20000000D03*"), "the front pad opens the front paste layer");
    let back_paste = fs::read_to_string(gbr.join("B_Paste.gbr")).expect("the back paste layer is readable");
    assert!(!back_paste.contains("D03*"), "a front pad opens no back paste and vias stay tented");
    let edges = fs::read_to_string(gbr.join("Edge_Cuts.gbr")).expect("edge cuts are readable");
    assert!(edges.contains("%TF.FileFunction,Profile,NP*%"), "edge cuts declare the profile function");
    assert_eq!(edges.matches("D01*").count(), 4, "the outline is four draws");

    let pdf = directory.join("blinker.pdf");
    let report = export(handle, "pdf", &pdf).expect("PDF export writes");
    assert_eq!(report["format"], "pdf");
    assert_eq!(report["editable"], false);
    assert_eq!(report["path"], pdf.display().to_string());
    assert_eq!(report["bytes"], fs::metadata(&pdf).expect("the PDF exists").len());
    let bytes = fs::read(&pdf).expect("the PDF is readable");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.starts_with("%PDF-1."), "the PDF starts with its version header");
    assert!(text.contains("/Type /Catalog"));
    assert!(text.contains("/Type /Page"));
    assert!(text.contains("/Count 1"), "the PDF is a single page");
    assert!(text.contains("/MediaBox [0 0 841.89 595.28]"), "the page keeps its A4 landscape size");
    assert!(text.contains("startxref"));
    assert!(text.contains("trailer"));
    assert!(text.contains("%%EOF"));
    let again = directory.join("blinker-again.pdf");
    export(handle, "pdf", &again).expect("a second PDF export writes");
    assert_eq!(fs::read(&pdf).expect("the first PDF is readable"), fs::read(&again).expect("the second PDF is readable"));

    let kicad = directory.join("blinker.kicad_pcb");
    let report = export(handle, "kicad_pcb", &kicad).expect("KiCad export writes");
    assert_eq!(report["format"], "kicad_pcb");
    let text = fs::read_to_string(&kicad).expect("the KiCad board is readable");
    assert!(text.starts_with("(kicad_pcb"), "the document is an S-expression");
    for marker in [
        "(version 20221018)",
        "(generator open-pcb)",
        "(gr_rect",
        "(segment",
        "(via",
        "(footprint \"0402\"",
        "(property \"Reference\" \"R1\"",
        "(pad \"1\" smd rect",
        "(layer \"B.Cu\")",
    ] {
        assert!(text.contains(marker), "KiCad output contains {marker}");
    }

    let ipc = directory.join("blinker.ipc2581");
    let report = export(handle, "ipc2581", &ipc).expect("IPC-2581 export writes");
    assert_eq!(report["format"], "ipc2581");
    let text = fs::read_to_string(&ipc).expect("the IPC-2581 document is readable");
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    for marker in [
        "<IPC-2581 revision=\"B\"",
        "<Datum",
        "<Profile>",
        "<Package name=\"0402\"",
        "<Pin number=\"1\"",
        "<Rect width=\"0.5\" height=\"0.5\"/>",
        "<Component ref=\"R1\"",
        "<LayerFeature layerRef=\"F.Cu\" kind=\"copper\">",
        "<Hole",
    ] {
        assert!(text.contains(marker), "IPC-2581 output contains {marker}");
    }

    let step = directory.join("blinker.step");
    let refused = export(handle, "step", &step).expect_err("STEP is not implemented");
    assert_eq!(refused.code, "capability/not-implemented");
    assert!(!step.exists());

    close(handle).expect("the board closes");
}

#[test]
fn pdf_export_reports_characters_the_base_font_cannot_encode() {
    let directory = TempDir::new();
    let path = directory.join("unicode.pdf");
    let handle = create(CreateRequest { title: Some("电源板".to_string()), template: Some("blank".to_string()) })
        .expect("the board is created")["handle"]
        .as_u64()
        .expect("the handle is a number");
    add_full_board(handle);

    let report = export(handle, "pdf", &path).expect("PDF export writes");
    assert_eq!(report["losses"][0]["code"], "pdf/text-encoding");
    assert!(report["losses"][0]["detail"].as_str().expect("detail").starts_with("3 character(s)"));
    let bytes = fs::read(&path).expect("the PDF is readable");
    assert!(String::from_utf8_lossy(&bytes).contains("???"), "the Chinese title degrades to question marks");

    close(handle).expect("the board closes");
}

fn outlined_board(width: f64, height: f64) -> Board {
    let mut board = crafted();
    board.outline = Some(Outline { from: [0.0, 0.0], to: [width, height] });
    board
}

#[test]
fn pdf_scales_a_small_board_to_fill_the_printable_area() {
    let board = outlined_board(72.0, 52.0);
    let page = crate::export::pdf_page(&board);
    assert_eq!(page.width, 841.89, "a small board keeps the A4 landscape width");
    assert_eq!(page.height, 595.28, "a small board keeps the A4 landscape height");
    assert!(page.scale > crate::export::PT_PER_MM, "the board is enlarged instead of drawn at 1 pt per mm");

    let width_ratio = page.board_width / page.printable_width();
    let height_ratio = page.board_height / page.printable_height();
    assert!(width_ratio >= 0.70, "the board covers at least 70% of the printable width, got {width_ratio}");
    assert!(height_ratio >= 0.70, "the board covers at least 70% of the printable height, got {height_ratio}");
    assert!(height_ratio > 0.99, "the limiting axis fills the printable height, got {height_ratio}");

    let left_gap = page.board_left;
    let right_gap = page.width - page.board_left - page.board_width;
    assert!((left_gap - right_gap).abs() < 0.01, "the board is horizontally centred: {left_gap} vs {right_gap}");
    let bottom_gap = page.board_bottom - (page.legend_band + crate::export::PDF_MARGIN_PT);
    let top_gap =
        page.height - crate::export::PDF_TITLE_BAND_PT - crate::export::PDF_MARGIN_PT - (page.board_bottom + page.board_height);
    assert!((bottom_gap - top_gap).abs() < 0.01, "the board is vertically centred: {bottom_gap} vs {top_gap}");

    let handle = store::insert(board);
    let directory = TempDir::new();
    let path = directory.join("small.pdf");
    export(handle, "pdf", &path).expect("PDF export writes");
    let text = String::from_utf8_lossy(&fs::read(&path).expect("the PDF is readable")).to_string();
    assert!(text.contains("/MediaBox [0 0 841.89 595.28]"), "the fitted board stays on one A4 landscape page");
    close(handle).expect("the board closes");
}

#[test]
fn pdf_grows_the_page_for_a_board_larger_than_a4() {
    let board = outlined_board(400.0, 300.0);
    let page = crate::export::pdf_page(&board);
    assert!(page.width > 841.89, "the page grows past the A4 width: {}", page.width);
    assert!(page.height > 595.28, "the page grows past the A4 height: {}", page.height);
    assert!((page.scale - crate::export::PT_PER_MM).abs() < 1e-6, "the oversized board keeps 1 pt per mm instead of shrinking");
    assert!((page.board_width - 400.0 * crate::export::PT_PER_MM).abs() < 0.05, "the board keeps its millimetre width");
    assert!((page.board_height - 300.0 * crate::export::PT_PER_MM).abs() < 0.05, "the board keeps its millimetre height");
    assert!(
        page.width >= page.board_width + crate::export::PDF_MARGIN_PT * 2.0 - 0.01,
        "the grown page still leaves its margin around the board"
    );

    let handle = store::insert(board);
    let directory = TempDir::new();
    let path = directory.join("large.pdf");
    export(handle, "pdf", &path).expect("PDF export writes");
    let text = String::from_utf8_lossy(&fs::read(&path).expect("the PDF is readable")).to_string();
    let num = |value: f64| {
        let text = format!("{value:.3}");
        let trimmed = text.trim_end_matches('0').trim_end_matches('.').to_string();
        if trimmed.is_empty() { "0".to_string() } else { trimmed }
    };
    let media_box = format!("/MediaBox [0 0 {} {}]", num(page.width), num(page.height));
    assert!(text.contains(&media_box), "the PDF page matches the planned size: {media_box}");
    close(handle).expect("the board closes");
}

#[test]
fn gerber_writes_x2_object_attributes_and_closes_each_block() {
    let directory = TempDir::new();
    let handle = created();
    add_full_board(handle);
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: Some("VCC".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    let target = directory.join("gerber");
    export(handle, "gbr", &target).expect("Gerber export writes a directory");

    let copper = fs::read_to_string(target.join("F_Cu.gbr")).expect("the copper layer is readable");
    assert!(copper.contains("%TF.FileFunction,Copper,L1,Top*%"), "the file attribute is preserved");
    assert!(copper.contains("%TA.AperFunction,Conductor*%"), "the aperture attribute is preserved");
    assert!(
        copper.contains("%TO.N,VCC*%\nD10*\nX10000000Y10000000D02*\nX30000000Y10000000D01*\n%TD*%\n"),
        "the track declares its net and closes the block: {copper}"
    );
    assert!(
        copper.contains("%TO.N,VCC*%\n%TO.C,U1*%\n%TO.P,U1,1*%\nD11*\nX30000000Y20000000D03*\n%TD*%\n"),
        "the pad declares its net, component and pin: {copper}"
    );
    assert!(
        copper.contains("%TO.N,VCC*%\nD12*\nX20000000Y20000000D03*\n%TD*%\n"),
        "the via declares its net and closes the block: {copper}"
    );
    assert_eq!(copper.matches("%TD*%").count(), 4, "the aperture block and each of the three objects close once");

    let bottom = fs::read_to_string(target.join("B_Cu.gbr")).expect("the bottom copper layer is readable");
    assert!(bottom.contains("%TO.N,VCC*%"), "a through via declares its net on every covered layer");
    let silk = fs::read_to_string(target.join("F_SilkS.gbr")).expect("the silkscreen layer is readable");
    assert!(!silk.contains("%TO."), "non-copper layers keep no object attributes");

    close(handle).expect("the board closes");
}

#[test]
fn drill_map_writes_holes_tool_legend_and_back_drill_marks() {
    let directory = TempDir::new();
    let handle = created();
    add_full_board(handle);
    set_via_back_drill(handle, SetViaBackDrillRequest { id: "via-1".to_string(), depth: Some(0.8), stub: Some(0.2) })
        .expect("the via is back drilled");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [30.0, 30.0], diameter: 0.9, drill: 0.5, ..Default::default() },
    )
    .expect("the larger via is placed");

    let target = directory.join("blinker.drill-map.svg");
    let report = export(handle, "drill_map", &target).expect("the drill map writes");
    assert_eq!(report["format"], "drill_map");
    let text = fs::read_to_string(&target).expect("the drill map is readable");
    for marker in [
        "<?xml",
        "<svg",
        "OpenPCB drill map",
        "T1 dia 0.300 mm plated x1",
        "T2 dia 0.500 mm plated x1",
        "back drill x1",
        "stroke-dasharray",
    ] {
        assert!(text.contains(marker), "drill map contains {marker}");
    }
    assert_eq!(text.matches("<circle").count(), 3, "two holes and one back-drill ring");
    close(handle).expect("the board closes");
}

#[test]
fn gerber_rejects_a_board_without_outline_and_a_file_target() {
    let directory = TempDir::new();
    let handle = created();
    let target = directory.join("gerber");
    let refused = export(handle, "gbr", &target).expect_err("Gerber needs an outline");
    assert_eq!(refused.code, "format/invalid");
    assert!(!target.exists());

    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    let file = directory.join("not-a-directory.gbr");
    fs::write(&file, "occupied").expect("a file exists at the target");
    let refused = export(handle, "gbr", &file).expect_err("a file target is rejected");
    assert_eq!(refused.code, "format/invalid");
    close(handle).expect("the board closes");
}

#[test]
fn gerber_flashes_rectangular_pads() {
    let directory = TempDir::new();
    let mut board = crafted();
    board.footprints.push(Footprint {
        id: "fp-1".to_string(),
        ref_: "U1".to_string(),
        value: String::new(),
        footprint: String::new(),
        x: 10.0,
        y: 10.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 0.5, h: 0.25, net: None, drill: None }],
    });
    board.outline = Some(crate::board::Outline { from: [0.0, 0.0], to: [50.0, 40.0] });
    let handle = store::insert(board);

    let target = directory.join("gerber");
    export(handle, "gbr", &target).expect("Gerber export writes");
    let copper = fs::read_to_string(target.join("F_Cu.gbr")).expect("the copper layer is readable");
    assert!(copper.contains("R,0.500000X0.250000"), "the pad uses a rectangular aperture");
    assert!(copper.contains("D03*"), "the pad is flashed");
    close(handle).expect("the board closes");
}

#[test]
fn silkscreen_flags_missing_text_and_pads_outside_the_outline() {
    let mut board = crafted();
    board.outline = Some(crate::board::Outline { from: [0.0, 0.0], to: [10.0, 10.0] });
    board.footprints.push(Footprint {
        id: "fp-1".to_string(),
        ref_: "U1".to_string(),
        value: String::new(),
        footprint: String::new(),
        x: 5.0,
        y: 5.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0, net: None, drill: None }],
    });
    let handle = store::insert(board.clone());
    let report = inspect(handle, Some(vec!["silkscreen".to_string()])).expect("the silkscreen check runs");
    assert_eq!(codes(&report), ["silkscreen/no-reference-text"]);
    assert_eq!(report["issues"][0]["severity"], "warning");
    assert_eq!(report["issues"][0]["layerId"], "F.SilkS");
    assert_eq!(report["passed"], true, "a missing reference text is only a warning");
    close(handle).expect("the board closes");

    let mut outside = board;
    outside.footprints[0].pads[0].x = 5.0;
    let handle = store::insert(outside);
    let report = inspect(handle, Some(vec!["silkscreen".to_string()])).expect("the silkscreen check runs");
    let found = codes(&report);
    assert!(found.contains(&"silkscreen/pad-outside-outline"), "got {found:?}");
    assert_eq!(report["passed"], false, "a pad outside the outline is an error");
    close(handle).expect("the board closes");

    let mut bare = crafted();
    bare.footprints.push(Footprint {
        id: "fp-1".to_string(),
        ref_: "U1".to_string(),
        value: String::new(),
        footprint: String::new(),
        x: 5.0,
        y: 5.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0, net: None, drill: None }],
    });
    let handle = store::insert(bare);
    let report = inspect(handle, Some(vec!["silkscreen".to_string()])).expect("the silkscreen check runs");
    let found = codes(&report);
    assert!(found.contains(&"silkscreen/no-reference-text"));
    assert!(found.contains(&"silkscreen/outline-missing"), "got {found:?}");
    assert_eq!(report["passed"], true, "skipping geometry is reported as a warning");
    close(handle).expect("the board closes");
}

#[test]
fn courtyard_reports_overlap_and_out_of_board_warnings() {
    let mut board = crafted();
    board.outline = Some(crate::board::Outline { from: [0.0, 0.0], to: [50.0, 40.0] });
    for (index, x) in [10.0, 12.0].into_iter().enumerate() {
        board.footprints.push(Footprint {
            id: format!("fp-{}", index + 1),
            ref_: format!("U{}", index + 1),
            value: String::new(),
            footprint: String::new(),
            x,
            y: 10.0,
            rotation: 0.0,
            layer: "F.Cu".to_string(),
            pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 2.0, h: 2.0, net: None, drill: None }],
        });
    }
    let handle = store::insert(board);
    let report = inspect(handle, Some(vec!["courtyard".to_string()])).expect("the courtyard check runs");
    let found = codes(&report);
    assert!(found.contains(&"courtyard/overlap"), "got {found:?}");
    assert_eq!(report["passed"], false, "overlapping courtyards are an error");
    close(handle).expect("the board closes");

    let mut board = crafted();
    board.outline = Some(crate::board::Outline { from: [0.0, 0.0], to: [50.0, 40.0] });
    board.footprints.push(Footprint {
        id: "fp-1".to_string(),
        ref_: "U1".to_string(),
        value: String::new(),
        footprint: String::new(),
        x: 49.0,
        y: 10.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 2.0, h: 2.0, net: None, drill: None }],
    });
    let handle = store::insert(board);
    let report = inspect(handle, Some(vec!["courtyard".to_string()])).expect("the courtyard check runs");
    assert_eq!(codes(&report), ["courtyard/pad-outside-board"]);
    assert_eq!(report["passed"], true, "a courtyard outside the board is only a warning");
    close(handle).expect("the board closes");

    let mut bare = crafted();
    bare.footprints.push(Footprint {
        id: "fp-1".to_string(),
        ref_: "U1".to_string(),
        value: String::new(),
        footprint: String::new(),
        x: 5.0,
        y: 5.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0, net: None, drill: None }],
    });
    let handle = store::insert(bare);
    let report = inspect(handle, Some(vec!["courtyard".to_string()])).expect("the courtyard check runs");
    assert_eq!(codes(&report), ["courtyard/outline-missing"]);
    assert_eq!(report["passed"], true, "skipping geometry is reported as a warning");
    close(handle).expect("the board closes");
}

#[test]
fn editing_validates_arguments_and_tracks_revisions() {
    let handle = created();
    assert_eq!(summary(handle).expect("the board is open")["revision"], 1);

    let net = add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    assert_eq!(net["object"]["id"], "net-1");
    assert_eq!(net["revision"], 2);
    assert_eq!(
        add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect_err("duplicates are rejected").code,
        "session/argument-invalid"
    );

    let footprint = add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: None,
            footprint: None,
            x: None,
            y: None,
            rotation: None,
            layer: None,
        },
    )
    .expect("the footprint is placed");
    assert_eq!(footprint["object"]["id"], "fp-1");
    assert_eq!(footprint["object"]["layer"], "F.Cu");
    assert_eq!(footprint["revision"], 3);

    let duplicate = add_footprint(handle, AddFootprintRequest { reference: Some("U1".to_string()), ..Default::default() })
        .expect_err("duplicate references are rejected");
    assert_eq!(duplicate.code, "session/argument-invalid");

    let bad_layer = add_track(
        handle,
        AddTrackRequest { net: None, layer: "X.Cu".to_string(), width: 0.25, from: [0.0, 0.0], to: [1.0, 0.0] },
    )
    .expect_err("unknown layers are rejected");
    assert_eq!(bad_layer.code, "session/argument-invalid");

    let bad_net = add_track(
        handle,
        AddTrackRequest {
            net: Some("GND".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 0.0],
            to: [1.0, 0.0],
        },
    )
    .expect_err("undeclared nets are rejected");
    assert_eq!(bad_net.code, "session/argument-invalid");

    let thin = add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.0,
            from: [0.0, 0.0],
            to: [1.0, 0.0],
        },
    )
    .expect_err("non-positive widths are rejected");
    assert_eq!(thin.code, "session/argument-invalid");

    assert_eq!(
        add_via(handle, AddViaRequest { net: None, at: [0.0, 0.0], diameter: 0.6, drill: 0.0, ..Default::default() })
            .expect_err("a zero drill is rejected")
            .code,
        "session/argument-invalid"
    );

    let track = add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 0.0],
            to: [5.0, 0.0],
        },
    )
    .expect("the track is routed");
    assert_eq!(track["object"]["id"], "track-1");

    let deleted = delete_footprint(handle, "fp-1").expect("the footprint is deleted");
    assert_eq!(deleted["deleted"], "fp-1");
    assert_eq!(delete_footprint(handle, "fp-1").expect_err("a missing id is reported").code, "object/not-found");

    assert_eq!(summary(999_999).expect_err("unknown handles are rejected").code, "board/not-found");
    close(handle).expect("the board closes");
    assert_eq!(close(handle).expect_err("a closed handle is rejected").code, "board/not-found");
}

#[test]
fn template_other_than_blank_is_not_implemented() {
    let error =
        create(CreateRequest { title: None, template: Some("blinker".to_string()) }).expect_err("templates are limited");
    assert_eq!(error.code, "capability/not-implemented");
}

#[test]
fn undo_and_redo_restore_snapshots_and_bump_revision() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");

    let state = summary(handle).expect("the board is open");
    assert_eq!(state["counts"]["nets"], 1);
    assert_eq!(state["canUndo"], true);
    assert_eq!(state["canRedo"], false);

    let undone = undo(handle).expect("the net addition is reverted");
    assert_eq!(undone["board"]["counts"]["nets"], 0);
    assert_eq!(undone["board"]["revision"], 3, "an undo commits a new revision");
    assert_eq!(undone["board"]["canUndo"], false);
    assert_eq!(undone["board"]["canRedo"], true);
    assert_eq!(undo(handle).expect_err("history is empty").code, "board/history-empty");

    let redone = redo(handle).expect("the net addition is reapplied");
    assert_eq!(redone["board"]["counts"]["nets"], 1);
    assert_eq!(redone["board"]["revision"], 4);
    assert_eq!(redo(handle).expect_err("redo is empty").code, "board/history-empty");

    let directory = TempDir::new();
    let path = directory.join("blinker.opcb");
    save(handle, &path, false).expect("the board saves");
    let undone = undo(handle).expect("save did not add a history step");
    assert_eq!(undone["board"]["counts"]["nets"], 0, "undo returns to the snapshot before the net");
    close(handle).expect("the board closes");
}

#[test]
fn history_is_bounded_to_the_last_thirty_two_edits() {
    let handle = created();
    for index in 0..40 {
        add_net(handle, AddNetRequest { name: format!("N{index}") }).expect("the net is declared");
    }
    for _ in 0..32 {
        undo(handle).expect("a bounded snapshot is restored");
    }
    assert_eq!(undo(handle).expect_err("older snapshots are dropped").code, "board/history-empty");
    assert_eq!(summary(handle).expect("the board is open")["counts"]["nets"], 8);
    close(handle).expect("the board closes");
}

#[test]
fn layers_are_added_renamed_and_lock_edits() {
    let handle = created();
    let added =
        add_layer(handle, AddLayerRequest { name: "Inner1".to_string(), kind: Some("copper".to_string()), color: None })
            .expect("the layer is added");
    assert_eq!(added["object"]["id"], "Inner1");
    assert_eq!(added["object"]["kind"], "copper");
    assert_eq!(added["object"]["visible"], true);
    assert_eq!(added["object"]["locked"], false);

    assert_eq!(
        add_layer(handle, AddLayerRequest { name: "Inner1".to_string(), ..Default::default() })
            .expect_err("duplicates are rejected")
            .code,
        "session/argument-invalid"
    );

    add_track(
        handle,
        AddTrackRequest { net: None, layer: "Inner1".to_string(), width: 0.25, from: [5.0, 5.0], to: [10.0, 5.0] },
    )
    .expect("the track is routed on the new layer");

    let renamed = update_layer(
        handle,
        UpdateLayerRequest { id: "Inner1".to_string(), name: Some("Inner2".to_string()), ..Default::default() },
    )
    .expect("the layer is renamed");
    assert_eq!(renamed["object"]["id"], "Inner2");

    let directory = TempDir::new();
    let path = directory.join("layers.opcb");
    save(handle, &path, false).expect("the board saves");
    let text = fs::read_to_string(&path).expect("the document is readable");
    let document: Value = serde_json::from_str(&text).expect("the document is JSON");
    assert_eq!(document["document"]["tracks"][0]["layer"], "Inner2", "the track follows the rename");

    let hidden =
        update_layer(handle, UpdateLayerRequest { id: "Inner2".to_string(), visible: Some(false), ..Default::default() })
            .expect("visibility is set");
    assert_eq!(hidden["object"]["visible"], false);

    update_layer(handle, UpdateLayerRequest { id: "Inner2".to_string(), locked: Some(true), ..Default::default() })
        .expect("the layer is locked");
    assert_eq!(
        add_track(
            handle,
            AddTrackRequest { net: None, layer: "Inner2".to_string(), width: 0.25, from: [0.0, 0.0], to: [1.0, 0.0] },
        )
        .expect_err("a locked layer rejects new tracks")
        .code,
        "board/layer-locked"
    );
    assert_eq!(delete_track(handle, "track-1").expect_err("a locked layer rejects deletes").code, "board/layer-locked");

    update_layer(handle, UpdateLayerRequest { id: "Inner2".to_string(), locked: Some(false), ..Default::default() })
        .expect("the layer is unlocked");
    delete_track(handle, "track-1").expect("the track is deleted");
    close(handle).expect("the board closes");
}

#[test]
fn footprint_moves_updates_and_track_and_via_deletion() {
    let handle = created();
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN32".to_string()),
            x: Some(30.0),
            y: Some(20.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the footprint is placed");
    add_track(handle, AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.25, from: [0.0, 0.0], to: [5.0, 0.0] })
        .expect("the track is routed");
    add_via(handle, AddViaRequest { net: None, at: [2.0, 2.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the via is placed");

    let moved = move_footprint(
        handle,
        MoveFootprintRequest { id: "fp-1".to_string(), x: Some(40.0), y: Some(25.0), rotation: Some(90.0), layer: None },
    )
    .expect("the footprint moves");
    assert_eq!(moved["object"]["x"], 40.0);
    assert_eq!(moved["object"]["y"], 25.0);
    assert_eq!(moved["object"]["rotation"], 90.0);

    let updated = update_footprint(
        handle,
        UpdateFootprintRequest {
            id: "fp-1".to_string(),
            value: Some("SAMD21".to_string()),
            footprint: Some("TQFP32".to_string()),
        },
    )
    .expect("the value and package update");
    assert_eq!(updated["object"]["value"], "SAMD21");
    assert_eq!(updated["object"]["footprint"], "TQFP32");

    assert_eq!(
        move_footprint(handle, MoveFootprintRequest { id: "fp-404".to_string(), x: Some(1.0), ..Default::default() })
            .expect_err("a missing footprint is reported")
            .code,
        "object/not-found"
    );

    update_layer(handle, UpdateLayerRequest { id: "F.Cu".to_string(), locked: Some(true), ..Default::default() })
        .expect("the copper layer is locked");
    assert_eq!(
        move_footprint(handle, MoveFootprintRequest { id: "fp-1".to_string(), x: Some(1.0), ..Default::default() })
            .expect_err("a locked layer rejects moves")
            .code,
        "board/layer-locked"
    );
    assert_eq!(
        update_footprint(
            handle,
            UpdateFootprintRequest { id: "fp-1".to_string(), value: Some("X".to_string()), ..Default::default() }
        )
        .expect_err("a locked layer rejects updates")
        .code,
        "board/layer-locked"
    );
    assert_eq!(delete_footprint(handle, "fp-1").expect_err("a locked layer rejects deletes").code, "board/layer-locked");
    assert_eq!(
        add_via(handle, AddViaRequest { net: None, at: [1.0, 1.0], diameter: 0.6, drill: 0.3, ..Default::default() })
            .expect_err("a locked copper layer rejects new vias")
            .code,
        "board/layer-locked"
    );
    assert_eq!(delete_via(handle, "via-1").expect_err("a locked copper layer rejects via deletes").code, "board/layer-locked");
    update_layer(handle, UpdateLayerRequest { id: "F.Cu".to_string(), locked: Some(false), ..Default::default() })
        .expect("the copper layer is unlocked");

    assert_eq!(delete_track(handle, "track-1").expect("the track is deleted")["deleted"], "track-1");
    assert_eq!(delete_via(handle, "via-1").expect("the via is deleted")["deleted"], "via-1");
    assert_eq!(delete_track(handle, "track-1").expect_err("a missing track is reported").code, "object/not-found");
    close(handle).expect("the board closes");
}

#[test]
fn tracks_update_in_place_and_validate_requests() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    add_layer(handle, AddLayerRequest { name: "Inner1".to_string(), kind: Some("copper".to_string()), color: None })
        .expect("the layer is added");
    let added = add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 0.0],
            to: [5.0, 0.0],
        },
    )
    .expect("the track is routed");
    assert_eq!(added["object"]["id"], "track-1");

    let updated = update_track(
        handle,
        UpdateTrackRequest {
            id: "track-1".to_string(),
            width: Some(0.5),
            layer: Some("Inner1".to_string()),
            ..Default::default()
        },
    )
    .expect("the width and layer update in place");
    assert_eq!(updated["object"]["width"], 0.5);
    assert_eq!(updated["object"]["layer"], "Inner1");
    assert_eq!(updated["object"]["net"], "VCC");
    assert_eq!(updated["object"]["from"][0], 0.0);
    assert_eq!(updated["revision"], 6, "an in-place edit bumps the revision once");

    assert_eq!(
        update_track(handle, UpdateTrackRequest { id: "track-404".to_string(), width: Some(0.3), ..Default::default() })
            .expect_err("a missing id is reported")
            .code,
        "object/not-found"
    );
    assert_eq!(
        update_track(handle, UpdateTrackRequest { id: "track-1".to_string(), ..Default::default() })
            .expect_err("an empty patch is rejected")
            .code,
        "session/argument-invalid"
    );
    assert_eq!(
        update_track(
            handle,
            UpdateTrackRequest { id: "track-1".to_string(), net: Some("NOPE".to_string()), ..Default::default() }
        )
        .expect_err("an undeclared net is rejected")
        .code,
        "session/argument-invalid"
    );

    update_layer(handle, UpdateLayerRequest { id: "Inner1".to_string(), locked: Some(true), ..Default::default() })
        .expect("the old layer is locked");
    assert_eq!(
        update_track(handle, UpdateTrackRequest { id: "track-1".to_string(), width: Some(0.3), ..Default::default() })
            .expect_err("a locked old layer rejects the edit")
            .code,
        "board/layer-locked"
    );
    update_layer(handle, UpdateLayerRequest { id: "Inner1".to_string(), locked: Some(false), ..Default::default() })
        .expect("the old layer is unlocked");
    update_layer(handle, UpdateLayerRequest { id: "B.Cu".to_string(), locked: Some(true), ..Default::default() })
        .expect("the target layer is locked");
    assert_eq!(
        update_track(
            handle,
            UpdateTrackRequest { id: "track-1".to_string(), layer: Some("B.Cu".to_string()), ..Default::default() }
        )
        .expect_err("a locked target layer rejects the edit")
        .code,
        "board/layer-locked"
    );
    close(handle).expect("the board closes");
}

#[test]
fn tracks_split_at_a_point_and_validate_the_split_point() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 0.0],
            to: [10.0, 0.0],
        },
    )
    .expect("the track is routed");

    let split = split_track(handle, SplitTrackRequest { id: "track-1".to_string(), x: 4.0, y: 0.0 })
        .expect("the track splits on the segment");
    assert_eq!(split["first"]["id"], "track-1");
    assert_eq!(split["first"]["from"], json!([0.0, 0.0]));
    assert_eq!(split["first"]["to"], json!([4.0, 0.0]));
    assert_eq!(split["second"]["id"], "track-2");
    assert_eq!(split["second"]["from"], json!([4.0, 0.0]));
    assert_eq!(split["second"]["to"], json!([10.0, 0.0]));
    assert_eq!(split["second"]["net"], "VCC");
    assert_eq!(split["second"]["width"], 0.25);
    assert_eq!(split["revision"], 4, "the split bumps the revision once");
    assert_eq!(split["canRedo"], false);
    assert_eq!(summary(handle).expect("the board is listed")["counts"]["tracks"], 2);

    assert_eq!(
        split_track(handle, SplitTrackRequest { id: "track-404".to_string(), x: 1.0, y: 0.0 })
            .expect_err("a missing id is reported")
            .code,
        "object/not-found"
    );
    assert_eq!(
        split_track(handle, SplitTrackRequest { id: "track-1".to_string(), x: 0.0, y: 0.0 })
            .expect_err("the start endpoint is rejected")
            .code,
        "session/argument-invalid"
    );
    assert_eq!(
        split_track(handle, SplitTrackRequest { id: "track-1".to_string(), x: 4.0, y: 0.0 })
            .expect_err("the split of the second segment hits an endpoint")
            .code,
        "session/argument-invalid"
    );
    assert_eq!(
        split_track(handle, SplitTrackRequest { id: "track-1".to_string(), x: 2.0, y: 1.0 })
            .expect_err("a point off the segment is rejected")
            .code,
        "session/argument-invalid"
    );
    assert_eq!(
        split_track(handle, SplitTrackRequest { id: "track-1".to_string(), x: f64::NAN, y: 0.0 })
            .expect_err("a non-finite point is rejected")
            .code,
        "session/argument-invalid"
    );

    update_layer(handle, UpdateLayerRequest { id: "F.Cu".to_string(), locked: Some(true), ..Default::default() })
        .expect("the layer is locked");
    assert_eq!(
        split_track(handle, SplitTrackRequest { id: "track-1".to_string(), x: 2.0, y: 0.0 })
            .expect_err("a locked layer rejects the split")
            .code,
        "board/layer-locked"
    );
    close(handle).expect("the board closes");
}

#[test]
fn slots_update_in_place_and_reject_coincident_endpoints() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    add_slot(handle, AddSlotRequest { net: Some("VCC".to_string()), from: [0.0, 0.0], to: [5.0, 0.0], width: 0.8 })
        .expect("the slot is added");

    let updated = update_slot(
        handle,
        UpdateSlotRequest {
            id: "slot-1".to_string(),
            net: Some("GND".to_string()),
            from: Some([1.0, 1.0]),
            to: Some([6.0, 1.0]),
            width: Some(1.2),
        },
    )
    .expect("every field updates in place");
    assert_eq!(updated["object"]["id"], "slot-1");
    assert_eq!(updated["object"]["net"], "GND");
    assert_eq!(updated["object"]["from"], json!([1.0, 1.0]));
    assert_eq!(updated["object"]["to"], json!([6.0, 1.0]));
    assert_eq!(updated["object"]["width"], 1.2);
    assert_eq!(updated["revision"], 5, "an in-place edit bumps the revision once");

    assert_eq!(
        update_slot(handle, UpdateSlotRequest { id: "slot-404".to_string(), width: Some(1.0), ..Default::default() })
            .expect_err("a missing id is reported")
            .code,
        "object/not-found"
    );
    assert_eq!(
        update_slot(handle, UpdateSlotRequest { id: "slot-1".to_string(), ..Default::default() })
            .expect_err("an empty patch is rejected")
            .code,
        "session/argument-invalid"
    );
    assert_eq!(
        update_slot(
            handle,
            UpdateSlotRequest { id: "slot-1".to_string(), net: Some("NOPE".to_string()), ..Default::default() }
        )
        .expect_err("an undeclared net is rejected")
        .code,
        "session/argument-invalid"
    );
    assert_eq!(
        update_slot(handle, UpdateSlotRequest { id: "slot-1".to_string(), to: Some([1.0, 1.0]), ..Default::default() })
            .expect_err("coincident endpoints are rejected")
            .code,
        "session/argument-invalid"
    );
    assert_eq!(
        update_slot(handle, UpdateSlotRequest { id: "slot-1".to_string(), width: Some(0.0), ..Default::default() })
            .expect_err("a zero width is rejected")
            .code,
        "session/argument-invalid"
    );

    let unchanged = update_slot(handle, UpdateSlotRequest { id: "slot-1".to_string(), width: Some(1.0), ..Default::default() })
        .expect("the slot still updates after the rejected requests");
    assert_eq!(unchanged["object"]["from"], json!([1.0, 1.0]), "a rejected request left the endpoints untouched");
    assert_eq!(unchanged["object"]["net"], "GND");
    close(handle).expect("the board closes");
}

#[test]
fn vias_update_in_place_and_validate_requests() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [2.0, 2.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");

    let updated = update_via(
        handle,
        UpdateViaRequest { id: "via-1".to_string(), net: Some("GND".to_string()), drill: Some(0.25), ..Default::default() },
    )
    .expect("the net and drill update in place");
    assert_eq!(updated["object"]["net"], "GND");
    assert_eq!(updated["object"]["drill"], 0.25);
    assert_eq!(updated["object"]["diameter"], 0.6);
    assert_eq!(updated["object"]["at"][0], 2.0);
    assert_eq!(updated["revision"], 5, "an in-place edit bumps the revision once");

    assert_eq!(
        update_via(handle, UpdateViaRequest { id: "via-404".to_string(), drill: Some(0.2), ..Default::default() })
            .expect_err("a missing id is reported")
            .code,
        "object/not-found"
    );
    assert_eq!(
        update_via(handle, UpdateViaRequest { id: "via-1".to_string(), ..Default::default() })
            .expect_err("an empty patch is rejected")
            .code,
        "session/argument-invalid"
    );
    assert_eq!(
        update_via(handle, UpdateViaRequest { id: "via-1".to_string(), net: Some("NOPE".to_string()), ..Default::default() })
            .expect_err("an undeclared net is rejected")
            .code,
        "session/argument-invalid"
    );

    update_layer(handle, UpdateLayerRequest { id: "F.Cu".to_string(), locked: Some(true), ..Default::default() })
        .expect("the copper layer is locked");
    assert_eq!(
        update_via(handle, UpdateViaRequest { id: "via-1".to_string(), drill: Some(0.2), ..Default::default() })
            .expect_err("a locked layer rejects the edit")
            .code,
        "board/layer-locked"
    );
    close(handle).expect("the board closes");
}

#[test]
fn outline_validation_and_outside_warnings() {
    let handle = created();
    let invalid = set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [0.0, 80.0] })
        .expect_err("a zero-width outline is rejected");
    assert_eq!(invalid.code, "session/argument-invalid");

    let outline = set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");
    assert_eq!(outline["object"]["to"][0], 100.0);
    assert_eq!(summary(handle).expect("the board is open")["outline"]["from"][1], 0.0);

    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(200.0), y: Some(200.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.25, from: [200.0, 0.0], to: [210.0, 0.0] },
    )
    .expect("the track is routed");
    add_via(handle, AddViaRequest { net: None, at: [300.0, 300.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the via is placed");

    let report = inspect(handle, Some(vec!["outline".to_string()])).expect("the outline check runs");
    let found = codes(&report);
    for expected in ["drc/footprint-outside-outline", "drc/track-outside-outline", "drc/via-outside-outline"] {
        assert!(found.contains(&expected), "{expected} is reported; got {found:?}");
    }
    assert_eq!(report["passed"], true, "outside warnings do not fail the board");

    let drc = inspect(handle, Some(vec!["drc".to_string()])).expect("the drc check runs");
    assert!(codes(&drc).contains(&"drc/footprint-no-pads"), "a footprint without pads is a warning");
    assert_eq!(drc["passed"], true);
    close(handle).expect("the board closes");
}

#[test]
fn outline_and_pad_findings_are_stable() {
    let mut board = crafted();
    board.footprints.push(Footprint {
        id: "fp-1".to_string(),
        ref_: "U1".to_string(),
        value: String::new(),
        footprint: String::new(),
        x: 0.0,
        y: 0.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: Vec::new(),
    });
    board.outline = Some(crate::board::Outline { from: [10.0, 10.0], to: [10.0, 20.0] });
    let handle = store::insert(board);
    let report = inspect(handle, Some(vec!["outline".to_string()])).expect("the outline check runs");
    let found = codes(&report);
    assert!(found.contains(&"drc/outline-invalid"));
    assert_eq!(report["passed"], false);
    close(handle).expect("the board closes");

    let board = crafted();
    let handle = store::insert(board);
    let report = inspect(handle, Some(vec!["outline".to_string()])).expect("the outline check runs");
    assert_eq!(codes(&report), ["drc/outline-missing"]);
    assert_eq!(report["passed"], false);
    close(handle).expect("the board closes");
}

#[test]
fn kicad_import_round_trips_an_exported_board() {
    let directory = TempDir::new();
    let source = directory.join("blinker.kicad_pcb");
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN32".to_string()),
            x: Some(30.0),
            y: Some(20.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the front footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.6,
            h: 0.4,
            net: Some("VCC".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U2".to_string()),
            value: Some("0402".to_string()),
            footprint: Some("R_0402".to_string()),
            x: Some(50.0),
            y: Some(30.0),
            rotation: Some(90.0),
            layer: Some("B.Cu".to_string()),
        },
    )
    .expect("the back footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-2".to_string(),
            number: "1".to_string(),
            x: 0.5,
            y: -0.5,
            w: 0.5,
            h: 0.5,
            net: Some("GND".to_string()),
            drill: None,
        },
    )
    .expect("the back pad is added");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.0, 10.0],
            to: [30.0, 10.0],
        },
    )
    .expect("the front track is routed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("GND".to_string()),
            layer: "B.Cu".to_string(),
            width: 0.3,
            from: [30.0, 10.0],
            to: [50.0, 10.0],
        },
    )
    .expect("the back track is routed");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [20.0, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");
    export(handle, "kicad_pcb", &source).expect("KiCad export writes");

    let imported = import(&source).expect("the exported KiCad board imports");
    let imported_handle = handle_of(&imported);
    assert_eq!(imported["format"], "kicad_pcb");
    assert_eq!(imported["editable"], true);
    assert_eq!(
        imported["losses"].as_array().expect("losses is an array"),
        &Vec::<Value>::new(),
        "the exporter round trips without losses"
    );
    let document = &imported["document"];
    assert_eq!(document["counts"]["nets"], 2);
    assert_eq!(document["counts"]["footprints"], 2);
    assert_eq!(document["counts"]["tracks"], 2);
    assert_eq!(document["counts"]["vias"], 1);
    assert_eq!(document["outline"]["from"][0], 0.0);
    assert_eq!(document["outline"]["to"][1], 80.0);
    let layers: Vec<&str> = document["layers"]
        .as_array()
        .expect("layers is an array")
        .iter()
        .map(|layer| layer["id"].as_str().expect("layer ids are strings"))
        .collect();
    assert_eq!(layers, ["F.Cu", "B.Cu", "F.SilkS", "B.SilkS", "Edge.Cuts"]);

    let saved = directory.join("imported.opcb");
    save(imported_handle, &saved, false).expect("the imported board saves as a native document");
    let text = fs::read_to_string(&saved).expect("the native document is readable");
    let native: Value = serde_json::from_str(&text).expect("the native document is JSON");
    let board = &native["document"];
    assert_eq!(board["nets"][0]["name"], "VCC");
    assert_eq!(board["nets"][1]["name"], "GND");
    assert_eq!(board["footprints"][0]["ref"], "U1");
    assert_eq!(board["footprints"][0]["x"], 30.0);
    assert_eq!(board["footprints"][0]["value"], "MCU");
    assert_eq!(board["footprints"][0]["pads"][0]["net"], "VCC");
    assert_eq!(board["footprints"][1]["rotation"], 90.0);
    assert_eq!(board["footprints"][1]["pads"][0]["x"], 0.5);
    assert_eq!(board["footprints"][1]["pads"][0]["y"], -0.5);
    assert_eq!(board["tracks"][0]["from"][0], 10.0);
    assert_eq!(board["tracks"][0]["to"][1], 10.0);
    assert_eq!(board["tracks"][0]["net"], "VCC");
    assert_eq!(board["tracks"][1]["layer"], "B.Cu");
    assert_eq!(board["vias"][0]["at"][0], 20.0);
    assert_eq!(board["vias"][0]["diameter"], 0.6);
    assert_eq!(board["vias"][0]["drill"], 0.3);
    assert_eq!(board["outline"]["to"][0], 100.0);
    close(imported_handle).expect("the imported board closes");

    let reopened = open(&saved).expect("the saved import reopens");
    let reopened_handle = handle_of(&reopened);
    assert_eq!(reopened["board"]["counts"]["tracks"], 2);
    let inspected = inspect(reopened_handle, None).expect("the imported board passes inspection");
    assert_eq!(inspected["passed"], true);
    close(reopened_handle).expect("the reopened import closes");

    let second = import(&source).expect("a second import of the same file");
    let again = directory.join("imported-again.opcb");
    save(handle_of(&second), &again, false).expect("the second import saves");
    close(handle_of(&second)).expect("the second import closes");
    assert_eq!(
        fs::read(&saved).expect("the first document is readable"),
        fs::read(&again).expect("the second document is readable")
    );
    close(handle).expect("the source board closes");
}

#[test]
fn kicad_import_reports_unrepresentable_constructs() {
    let directory = TempDir::new();
    let path = directory.join("annotated.kicad_pcb");
    let text = "(kicad_pcb (version 20221018) (generator open-pcb)\n\
        (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal))\n\
        (net 0 \"\")\n\
        (gr_rect (start 0 0) (end 20 10) (layer \"Edge.Cuts\"))\n\
        (zone (net 1) (net_name \"GND\") (layer \"F.Cu\") (polygon (pts (xy 0 0) (xy 5 0) (xy 5 5))))\n\
        (gr_text \"hello\" (at 2 2) (layer \"F.SilkS\"))\n\
        (segment (start 1 1) (end 5 1) (width 0.25) (layer \"F.Cu\") (net 0)))\n";
    fs::write(&path, text).expect("the fixture is written");
    let imported = import(&path).expect("the annotated board imports");
    let losses = imported["losses"].as_array().expect("losses is an array");
    assert_eq!(losses.len(), 2, "each unrepresentable construct is reported once");
    for loss in losses {
        assert_eq!(loss["code"], "kicad/construct-not-imported");
    }
    let codes: Vec<&str> =
        imported["lossCodes"].as_array().expect("codes are listed").iter().filter_map(Value::as_str).collect();
    assert_eq!(codes, ["kicad/construct-not-imported"]);
    assert_eq!(imported["document"]["counts"]["tracks"], 1);
    assert_eq!(imported["document"]["outline"]["to"][0], 20.0);
    close(handle_of(&imported)).expect("the imported board closes");
}

fn two_pad_net(handle: u64, first: [f64; 2], second: [f64; 2]) {
    add_net(handle, AddNetRequest { name: "N1".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(0.0), y: Some(0.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    for (number, at) in [("1", first), ("2", second)] {
        add_pad(
            handle,
            AddPadRequest {
                footprint: "fp-1".to_string(),
                number: number.to_string(),
                x: at[0],
                y: at[1],
                w: 0.5,
                h: 0.5,
                net: Some("N1".to_string()),
                drill: None,
            },
        )
        .expect("the pad is added");
    }
}

#[test]
fn router_connects_two_pads_on_a_clean_board() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 15.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");

    let report = route(handle, RouteOptions::default()).expect("routing runs");
    assert_eq!(report["routed"], 1);
    assert_eq!(report["unrouted"], Value::Array(Vec::new()));
    assert!(report["tracks"].as_u64().expect("a track count") >= 1);

    let tracks = store::read(handle, |board| board.tracks.clone()).expect("the board is readable");
    assert_eq!(tracks[0].from, [10.0, 10.0], "routing starts at the first pad centre");
    assert_eq!(tracks[tracks.len() - 1].to, [20.0, 15.0], "routing ends at the second pad centre");
    assert_eq!(summary(handle).expect("the board is open")["counts"]["tracks"], report["tracks"]);
    close(handle).expect("the board closes");
}

#[test]
fn router_avoids_keepout_regions() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    add_keepout(handle, AddKeepoutRequest { layer: None, points: vec![[14.0, 0.0], [16.0, 0.0], [16.0, 30.0], [14.0, 30.0]] })
        .expect("the blocking keepout is added");

    let report = route(handle, RouteOptions::default()).expect("routing runs");
    assert_eq!(report["routed"], 0, "the only direct path crosses a keepout");
    assert_eq!(report["tracks"], 0, "no copper may enter a keepout");
    assert_eq!(report["vias"], 0);
    let unrouted = report["unrouted"].as_array().expect("unrouted is an array");
    assert_eq!(unrouted.len(), 1);
    assert_eq!(unrouted[0]["net"], "N1");
    close(handle).expect("the board closes");
}

#[test]
fn router_reports_an_unroutable_edge_instead_of_fabricating_tracks() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    for layer in ["F.Cu", "B.Cu"] {
        add_track(
            handle,
            AddTrackRequest { net: None, layer: layer.to_string(), width: 0.25, from: [15.0, 0.0], to: [15.0, 30.0] },
        )
        .expect("the blocking track is routed");
    }

    let before = summary(handle).expect("the board is open")["revision"].clone();
    let report = route(handle, RouteOptions::default()).expect("routing runs");
    assert_eq!(report["routed"], 0);
    assert_eq!(report["tracks"], 0);
    assert_eq!(report["vias"], 0);
    let unrouted = report["unrouted"].as_array().expect("unrouted is an array");
    assert_eq!(unrouted.len(), 1);
    assert_eq!(unrouted[0]["net"], "N1");
    assert_eq!(unrouted[0]["from"], "fp-1:1");
    assert_eq!(unrouted[0]["to"], "fp-1:2");
    assert!(unrouted[0]["reason"].as_str().is_some_and(|reason| !reason.is_empty()));
    assert_eq!(summary(handle).expect("the board is open")["revision"], before, "an empty route run leaves the revision");
    close(handle).expect("the board closes");
}

fn crossed_nets(handle: u64) {
    add_net(handle, AddNetRequest { name: "N1".to_string() }).expect("N1 is declared");
    add_net(handle, AddNetRequest { name: "N2".to_string() }).expect("N2 is declared");
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(0.0), y: Some(0.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    for (number, name, at) in
        [("1", "N1", [10.0, 10.5]), ("2", "N1", [30.0, 10.5]), ("3", "N2", [20.0, 10.0]), ("4", "N2", [20.0, 30.0])]
    {
        add_pad(
            handle,
            AddPadRequest {
                footprint: "fp-1".to_string(),
                number: number.to_string(),
                x: at[0],
                y: at[1],
                w: 0.2,
                h: 0.2,
                net: Some(name.to_string()),
                drill: None,
            },
        )
        .expect("the pad is added");
    }
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [40.0, 40.0] }).expect("the outline is set");
}

#[test]
fn router_rip_up_retry_recovers_a_greedy_failure() {
    let handle = created();
    crossed_nets(handle);

    let report = route(handle, RouteOptions::default()).expect("routing runs");
    assert_eq!(report["routed"], 2, "both nets are routed after the retry: {report}");
    assert_eq!(report["unrouted"], Value::Array(Vec::new()));
    assert_eq!(report["retries"], 1, "the first greedy pass fails, so one retry runs");
    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets.len(), 2);
    let n1 = nets.iter().find(|net| net["net"] == "N1").expect("N1 is reported");
    assert_eq!(n1["status"], "routed");
    assert_eq!(n1["edges"], 1);
    assert_eq!(n1["retries"], 0);
    let n2 = nets.iter().find(|net| net["net"] == "N2").expect("N2 is reported");
    assert_eq!(n2["status"], "routed");
    assert_eq!(n2["retries"], 1);
    assert_eq!(n2["unrouted"], 0);

    let counts = summary(handle).expect("the board is open")["counts"].clone();
    assert_eq!(counts["tracks"], report["tracks"]);
    assert_eq!(counts["vias"], report["vias"]);
    close(handle).expect("the board closes");
}

#[test]
fn router_reports_a_failure_after_bounded_rip_up_retries() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    for layer in ["F.Cu", "B.Cu"] {
        add_track(
            handle,
            AddTrackRequest { net: None, layer: layer.to_string(), width: 0.25, from: [15.0, 0.0], to: [15.0, 30.0] },
        )
        .expect("the blocking track is routed");
    }

    let report = route(handle, RouteOptions::default()).expect("routing runs");
    assert_eq!(report["routed"], 0);
    assert_eq!(report["retries"], 2, "the default retry budget is spent on a real wall");
    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets[0]["net"], "N1");
    assert_eq!(nets[0]["status"], "unrouted");
    assert_eq!(nets[0]["retries"], 2);
    assert_eq!(nets[0]["unrouted"], 1);
    assert_eq!(report["unrouted"].as_array().expect("unrouted is an array").len(), 1);
    close(handle).expect("the board closes");
}

fn assign_power_class(handle: u64, width: Option<f64>, clearance: Option<f64>, via_diameter: Option<f64>) {
    add_net_class(
        handle,
        AddNetClassRequest {
            name: "PWR".to_string(),
            width,
            clearance,
            via_diameter,
            via_drill: via_diameter.map(|diameter| diameter / 2.0),
        },
    )
    .expect("the net class is declared");
    assign_net_class(handle, AssignNetClassRequest { net: "N1".to_string(), class: "PWR".to_string() })
        .expect("the net joins the class");
}

#[test]
fn router_uses_net_class_width_and_via() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.25, from: [15.0, 0.0], to: [15.0, 30.0] },
    )
    .expect("the top-layer blocker is routed");
    assign_power_class(handle, Some(0.4), Some(0.3), Some(0.8));

    let report = route(handle, RouteOptions { use_net_classes: true, ..Default::default() }).expect("routing runs");
    assert_eq!(report["routed"], 1);
    assert_eq!(report["vias"], 2, "the blocked top layer forces a via at each endpoint: {report}");
    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets[0]["width"], 0.4);
    assert_eq!(nets[0]["widthSource"], "netclass");

    let (tracks, vias) = store::read(handle, |board| (board.tracks.clone(), board.vias.clone())).expect("the board is open");
    let routed = tracks.iter().filter(|track| track.net.as_deref() == Some("N1")).collect::<Vec<_>>();
    assert!(!routed.is_empty());
    assert!(routed.iter().all(|track| track.width == 0.4), "every routed segment uses the class width");
    assert_eq!(vias.len(), 2);
    assert!(vias.iter().all(|via| via.diameter == 0.8 && via.drill == 0.4), "vias use the class diameter and drill");
    close(handle).expect("the board closes");
}

#[test]
fn router_uses_net_class_clearance_and_falls_back_per_field() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.25, from: [5.0, 10.55], to: [25.0, 10.55] },
    )
    .expect("the parallel blocker is routed");
    add_net_class(handle, AddNetClassRequest { name: "PWR".to_string(), clearance: Some(0.05), ..Default::default() })
        .expect("the clearance-only class is declared");
    assign_net_class(handle, AssignNetClassRequest { net: "N1".to_string(), class: "PWR".to_string() })
        .expect("the net joins the class");

    // The argument clearance (0.8) would block the 0.55 gap; the class clearance
    // (0.05) clears it, and the missing class width falls back to the argument.
    let report = route(
        handle,
        RouteOptions {
            layers: Some(vec!["F.Cu".to_string()]),
            width: Some(0.4),
            clearance: Some(0.8),
            use_net_classes: true,
            ..Default::default()
        },
    )
    .expect("routing runs");
    assert_eq!(report["routed"], 1, "the class clearance governs the gap: {report}");
    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets[0]["width"], 0.4, "a class without a width falls back to the argument");
    assert_eq!(nets[0]["widthSource"], "argument");
    let tracks = store::read(handle, |board| board.tracks.clone()).expect("the board is open");
    assert!(tracks.iter().filter(|track| track.net.as_deref() == Some("N1")).all(|track| track.width == 0.4));
    close(handle).expect("the board closes");

    let fallback = created();
    two_pad_net(fallback, [10.0, 10.0], [20.0, 10.0]);
    set_outline(fallback, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    add_net_class(fallback, AddNetClassRequest { name: "EMPTY".to_string(), ..Default::default() })
        .expect("the empty class is declared");
    assign_net_class(fallback, AssignNetClassRequest { net: "N1".to_string(), class: "EMPTY".to_string() })
        .expect("the net joins the empty class");

    let report = route(fallback, RouteOptions { use_net_classes: true, ..Default::default() }).expect("routing runs");
    assert_eq!(report["routed"], 1);
    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets[0]["width"], 0.25);
    assert_eq!(nets[0]["widthSource"], "default");
    let tracks = store::read(fallback, |board| board.tracks.clone()).expect("the board is open");
    assert!(tracks.iter().all(|track| track.width == 0.25));
    close(fallback).expect("the board closes");

    let unassigned = created();
    two_pad_net(unassigned, [10.0, 10.0], [20.0, 10.0]);
    set_outline(unassigned, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    let report = route(unassigned, RouteOptions { use_net_classes: true, ..Default::default() }).expect("routing runs");
    assert_eq!(report["routed"], 1);
    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets[0]["width"], 0.25, "a net without a class falls back to the default");
    assert_eq!(nets[0]["widthSource"], "default");
    close(unassigned).expect("the board closes");
}

#[test]
fn router_ignores_net_classes_unless_opted_in() {
    let route_with = |options: RouteOptions| {
        let handle = created();
        two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
        set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
        assign_power_class(handle, Some(0.5), Some(0.5), Some(0.9));
        let report = route(handle, options).expect("routing runs");
        let tracks = store::read(handle, |board| board.tracks.clone()).expect("the board is open");
        close(handle).expect("the board closes");
        (report, tracks)
    };

    let (legacy, legacy_tracks) = route_with(RouteOptions::default());
    let (explicit_off, explicit_off_tracks) = route_with(RouteOptions { use_net_classes: false, ..Default::default() });
    assert_eq!(legacy, explicit_off, "the option defaults to off");
    assert_eq!(
        legacy_tracks.iter().map(|track| track.width).collect::<Vec<_>>(),
        explicit_off_tracks.iter().map(|track| track.width).collect::<Vec<_>>()
    );
    assert!(legacy_tracks.iter().all(|track| track.width == 0.25), "the class width stays unused");
    assert!(!serde_json::to_string(&legacy).expect("the report serializes").contains("widthSource"));
}

#[test]
fn router_still_reports_unrouted_edges_with_net_classes() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    for layer in ["F.Cu", "B.Cu"] {
        add_track(
            handle,
            AddTrackRequest { net: None, layer: layer.to_string(), width: 0.25, from: [15.0, 0.0], to: [15.0, 30.0] },
        )
        .expect("the blocking track is routed");
    }
    assign_power_class(handle, Some(0.4), None, None);

    let report = route(handle, RouteOptions { use_net_classes: true, ..Default::default() }).expect("routing runs");
    assert_eq!(report["routed"], 0);
    assert_eq!(report["tracks"], 0);
    assert_eq!(report["vias"], 0);
    let unrouted = report["unrouted"].as_array().expect("unrouted is an array");
    assert_eq!(unrouted.len(), 1);
    assert_eq!(unrouted[0]["net"], "N1");
    assert_eq!(unrouted[0]["widthSource"], "netclass");
    assert!(unrouted[0]["reason"].as_str().is_some_and(|reason| !reason.is_empty()));
    close(handle).expect("the board closes");
}

#[test]
fn router_chamfer45_connects_with_an_exact_diagonal() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [28.0, 22.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");

    let report = route(handle, RouteOptions { style: RouteStyle::Chamfer45, ..Default::default() }).expect("routing runs");
    assert_eq!(report["style"], "chamfer45");
    assert_eq!(report["routed"], 1);
    assert_eq!(report["tracks"], 3, "the edge is an axis segment, a diagonal and an axis segment");

    let edges = report["edges"].as_array().expect("edges is an array");
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["style"], "chamfer45");
    assert!(edges[0]["fallback"].is_null());
    let segments = edges[0]["segments"].as_array().expect("segments is an array");
    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0]["from"][1], segments[0]["to"][1], "the first segment is axis aligned");
    assert_eq!(segments[2]["from"][0], segments[2]["to"][0], "the last segment is axis aligned");
    assert_eq!(segments[0]["to"], segments[1]["from"], "the segments form one chain");
    assert_eq!(segments[1]["to"], segments[2]["from"]);
    let dx = segments[1]["to"][0].as_f64().expect("a diagonal x") - segments[1]["from"][0].as_f64().expect("a diagonal x");
    let dy = segments[1]["to"][1].as_f64().expect("a diagonal y") - segments[1]["from"][1].as_f64().expect("a diagonal y");
    assert!(dx.abs() > 0.0, "the diagonal has length");
    assert!((dx.abs() - dy.abs()).abs() < 1e-9, "the diagonal is exactly 45 degrees: dx={dx} dy={dy}");

    let tracks = store::read(handle, |board| board.tracks.clone()).expect("the board is open");
    let routed = tracks.iter().filter(|track| track.net.as_deref() == Some("N1")).collect::<Vec<_>>();
    assert_eq!(routed.len(), 3);
    assert_eq!(routed[0].from, [10.0, 10.0]);
    assert_eq!(routed[routed.len() - 1].to, [28.0, 22.0]);

    let inspection = inspect(handle, Some(vec!["drc".to_string(), "clearance".to_string()])).expect("the board is inspected");
    assert_eq!(inspection["passed"], true, "a chamfered edge still passes DRC and clearance: {inspection}");
    close(handle).expect("the board closes");
}

#[test]
fn router_chamfer45_falls_back_when_an_edge_has_no_diagonal_room() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");

    let report = route(handle, RouteOptions { style: RouteStyle::Chamfer45, ..Default::default() }).expect("routing runs");
    assert_eq!(report["routed"], 1);
    assert_eq!(report["tracks"], 1);
    let edges = report["edges"].as_array().expect("edges is an array");
    assert_eq!(edges[0]["style"], "orthogonal");
    assert_eq!(edges[0]["fallback"], "diagonal-length-zero");
    let segments = edges[0]["segments"].as_array().expect("segments is an array");
    assert_eq!(segments[0]["style"], "orthogonal");
    assert_eq!(segments[0]["from"][1], segments[0]["to"][1], "the fallback stays an axis-aligned L path");
    close(handle).expect("the board closes");
}

#[test]
fn router_chamfer45_falls_back_when_no_diagonal_is_clear() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [30.0, 20.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    add_keepout(
        handle,
        AddKeepoutRequest { layer: None, points: vec![[11.0, 10.5], [29.0, 10.5], [29.0, 19.5], [11.0, 19.5]] },
    )
    .expect("the central keepout is added");

    let report = route(handle, RouteOptions { style: RouteStyle::Chamfer45, ..Default::default() }).expect("routing runs");
    assert_eq!(report["routed"], 1, "the orthogonal L path avoids the keepout");
    let edges = report["edges"].as_array().expect("edges is an array");
    assert_eq!(edges[0]["style"], "orthogonal");
    assert_eq!(edges[0]["fallback"], "chamfer-clearance", "every diagonal variant crosses the keepout");

    let tracks = store::read(handle, |board| board.tracks.clone()).expect("the board is open");
    let routed = tracks.iter().filter(|track| track.net.as_deref() == Some("N1")).collect::<Vec<_>>();
    assert_eq!(routed.len(), 2, "the fallback is the historical two-segment L path");
    let inspection = inspect(handle, Some(vec!["drc".to_string(), "clearance".to_string(), "keepout".to_string()]))
        .expect("the board is inspected");
    assert_eq!(inspection["passed"], true, "the fallback keeps copper out of the keepout: {inspection}");
    close(handle).expect("the board closes");
}

#[test]
fn router_default_style_keeps_the_historical_report() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 10.0]);
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");

    let report = route(handle, RouteOptions::default()).expect("routing runs");
    assert_eq!(
        report,
        json!({
            "routed": 1,
            "unrouted": [],
            "tracks": 1,
            "vias": 0,
            "retries": 0,
            "nets": [{
                "net": "N1",
                "status": "routed",
                "edges": 1,
                "routed": 1,
                "unrouted": 0,
                "retries": 0,
            }],
        }),
        "orthogonal routing keeps every field and adds none"
    );
    assert!(report.get("style").is_none());
    assert!(report.get("edges").is_none());
    close(handle).expect("the board closes");
}

#[test]
fn router_chamfer45_is_deterministic() {
    let run = || {
        let handle = created();
        two_pad_net(handle, [10.0, 10.0], [28.0, 22.0]);
        set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
        let report = route(handle, RouteOptions { style: RouteStyle::Chamfer45, ..Default::default() }).expect("routing runs");
        let tracks = store::read(handle, |board| {
            board.tracks.iter().map(|track| (track.layer.clone(), track.from, track.to)).collect::<Vec<_>>()
        })
        .expect("the board is open");
        close(handle).expect("the board closes");
        (report, tracks)
    };

    let first = run();
    let second = run();
    assert_eq!(first.0, second.0, "the same board and options produce the same report");
    assert_eq!(first.1, second.1, "the same board and options produce the same copper");
}

#[test]
fn new_checks_flag_annular_ring_track_width_and_unrouted_nets() {
    let handle = created();
    two_pad_net(handle, [10.0, 10.0], [20.0, 20.0]);
    add_via(
        handle,
        AddViaRequest { net: Some("N1".to_string()), at: [2.0, 2.0], diameter: 0.6, drill: 0.55, ..Default::default() },
    )
    .expect("the thin-ring via is placed");
    add_track(handle, AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.05, from: [0.0, 0.0], to: [5.0, 0.0] })
        .expect("the narrow track is routed");

    let report = inspect(handle, Some(vec!["annular-ring".to_string(), "track-width".to_string(), "unrouted".to_string()]))
        .expect("the new checks run");
    let found = codes(&report);
    assert!(found.contains(&"drc/annular-ring-too-small"), "got {found:?}");
    assert!(found.contains(&"drc/track-width-too-small"), "got {found:?}");
    assert!(found.contains(&"drc/unrouted"), "got {found:?}");
    assert_eq!(report["passed"], false, "the annular ring and width findings are errors");

    let issues = report["issues"].as_array().expect("issues is an array");
    let ring = issues.iter().find(|issue| issue["code"] == "drc/annular-ring-too-small").expect("the ring issue");
    assert_eq!(ring["objectId"], "via-1");
    assert!(ring["message"].as_str().expect("a message").contains("0.025"));
    let unrouted = issues.iter().find(|issue| issue["code"] == "drc/unrouted").expect("the unrouted issue");
    assert_eq!(unrouted["objectId"], "net-1");
    assert!(unrouted["message"].as_str().expect("a message").contains("2 disconnected pad groups"));
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_hole_flags_close_drills_of_different_nets_only() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the first via is placed");
    add_via(
        handle,
        AddViaRequest { net: Some("B".to_string()), at: [10.4, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the second via is placed");

    let report = inspect(handle, Some(vec!["hole-to-hole".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/hole-to-hole"]);
    assert_eq!(report["passed"], false);
    let issue = &report["issues"][0];
    assert_eq!(issue["objectId"], "via-1+via-2");
    assert!(issue["message"].as_str().expect("a message").contains("0.1 mm"));
    close(handle).expect("the board closes");

    let same_net = created();
    add_net(same_net, AddNetRequest { name: "A".to_string() }).expect("the net is declared");
    add_via(
        same_net,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the first via is placed");
    add_via(
        same_net,
        AddViaRequest { net: Some("A".to_string()), at: [10.4, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the second via is placed");
    let report = inspect(same_net, Some(vec!["hole-to-hole".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "holes on the same net are not a hole-to-hole violation");
    close(same_net).expect("the board closes");
}

#[test]
fn hole_to_hole_flags_drilled_pads_of_different_nets() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(10.0), y: Some(10.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    for (number, x, net) in [("1", 0.0, "A"), ("2", 0.4, "B")] {
        add_pad(
            handle,
            AddPadRequest {
                footprint: "fp-1".to_string(),
                number: number.to_string(),
                x,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                net: Some(net.to_string()),
                drill: Some(0.3),
            },
        )
        .expect("the drilled pad is added");
    }

    let report = inspect(handle, Some(vec!["hole-to-hole".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/hole-to-hole"]);
    assert_eq!(report["passed"], false);
    let issue = &report["issues"][0];
    assert_eq!(issue["objectId"], "fp-1.1+fp-1.2", "a pad hole id is the footprint id and pad number");
    assert!(issue["message"].as_str().expect("a message").contains("0.1 mm"));
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_hole_compares_drilled_pads_with_vias() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [20.0, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(20.4), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("B".to_string()),
            drill: Some(0.3),
        },
    )
    .expect("the drilled pad is added");

    let report = inspect(handle, Some(vec!["hole-to-hole".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/hole-to-hole"]);
    let issue = &report["issues"][0];
    assert_eq!(issue["objectId"], "via-1+fp-1.1", "vias come before pads in the report");
    assert!(issue["message"].as_str().expect("a message").contains("0.1 mm"));
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_hole_ignores_same_net_unassigned_plain_and_exact_gap_pads() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    for (reference, id, x, first, second) in [
        ("J1", "fp-1", 10.0, (Some("A"), 0.0, 0.3), (Some("A"), 0.4, 0.3)),
        ("J2", "fp-2", 30.0, (None, 0.0, 0.3), (None, 0.4, 0.3)),
        ("J3", "fp-3", 50.0, (Some("A"), 0.0, 0.75), (Some("B"), 1.0, 0.75)),
    ] {
        add_footprint(
            handle,
            AddFootprintRequest { reference: Some(reference.to_string()), x: Some(x), y: Some(x), ..Default::default() },
        )
        .expect("the footprint is placed");
        for (number, (net, pad_x, drill)) in [("1", first), ("2", second)] {
            add_pad(
                handle,
                AddPadRequest {
                    footprint: id.to_string(),
                    number: number.to_string(),
                    x: pad_x,
                    y: 0.0,
                    w: 1.0,
                    h: 1.0,
                    net: net.map(str::to_string),
                    drill: Some(drill),
                },
            )
            .expect("the drilled pad is added");
        }
    }
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J4".to_string()), x: Some(70.0), y: Some(70.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    for (number, net) in [("1", "A"), ("2", "B")] {
        add_pad(
            handle,
            AddPadRequest {
                footprint: "fp-4".to_string(),
                number: number.to_string(),
                x: if number == "1" { 0.0 } else { 0.1 },
                y: 0.0,
                w: 1.0,
                h: 1.0,
                net: Some(net.to_string()),
                drill: None,
            },
        )
        .expect("the plain pad is added");
    }

    let report = inspect(handle, Some(vec!["hole-to-hole".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "same-net, unassigned, plain and exactly-at-rule pads are not violations");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_hole_orders_vias_before_pads() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    for (at, net) in [([10.0, 10.0], "A"), ([10.4, 10.0], "B")] {
        add_via(handle, AddViaRequest { net: Some(net.to_string()), at, diameter: 0.6, drill: 0.3, ..Default::default() })
            .expect("the via is placed");
    }
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(20.0), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    for (number, x, net) in [("1", 0.0, "A"), ("2", 0.4, "B")] {
        add_pad(
            handle,
            AddPadRequest {
                footprint: "fp-1".to_string(),
                number: number.to_string(),
                x,
                y: 0.0,
                w: 1.0,
                h: 1.0,
                net: Some(net.to_string()),
                drill: Some(0.3),
            },
        )
        .expect("the drilled pad is added");
    }

    let report = inspect(handle, Some(vec!["hole-to-hole".to_string()])).expect("the check runs");
    let objects: Vec<&str> = report["issues"]
        .as_array()
        .expect("issues is an array")
        .iter()
        .filter_map(|issue| issue["objectId"].as_str())
        .collect();
    assert_eq!(objects, ["via-1+via-2", "fp-1.1+fp-1.2"], "holes keep via and pad document order");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_track_flags_a_via_near_a_track_of_another_net() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("B".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.25, 8.0],
            to: [10.25, 12.0],
        },
    )
    .expect("the track is routed");

    let report = inspect(handle, Some(vec!["hole-to-track".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/hole-to-track"]);
    assert_eq!(report["passed"], false);
    let issue = &report["issues"][0];
    assert_eq!(issue["objectId"], "via-1+track-1");
    assert_eq!(issue["layerId"], "F.Cu");
    let message = issue["message"].as_str().expect("a message");
    assert!(message.contains("drill hole `via-1`") && message.contains("track `track-1`"), "{message}");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_track_flags_a_rotated_drilled_pad_near_a_track() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("J1".to_string()),
            x: Some(10.0),
            y: Some(10.0),
            rotation: Some(90.0),
            ..Default::default()
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 1.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("A".to_string()),
            drill: Some(0.3),
        },
    )
    .expect("the drilled pad is added");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("B".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [8.0, 11.25],
            to: [12.0, 11.25],
        },
    )
    .expect("the track is routed");

    let report = inspect(handle, Some(vec!["hole-to-track".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/hole-to-track"]);
    assert_eq!(
        report["issues"][0]["objectId"], "fp-1.1+track-1",
        "a rotated pad uses its world position and its hole id is the footprint and pad number"
    );
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_track_skips_same_net_and_unassigned_pairs() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("the net is declared");
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the same-net via is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("A".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.25, 8.0],
            to: [10.25, 12.0],
        },
    )
    .expect("the same-net track is routed");
    add_via(handle, AddViaRequest { net: None, at: [20.0, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the unassigned via is placed");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "B.Cu".to_string(), width: 0.25, from: [20.25, 18.0], to: [20.25, 22.0] },
    )
    .expect("the unassigned track is routed");

    let report = inspect(handle, Some(vec!["hole-to-track".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "same-net and unassigned holes and tracks are not compared");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_track_accepts_a_gap_exactly_at_the_rule() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_net_class(handle, AddNetClassRequest { name: "PWR".to_string(), clearance: Some(0.5), ..Default::default() })
        .expect("the clearance class is declared");
    for net in ["A", "B"] {
        assign_net_class(handle, AssignNetClassRequest { net: net.to_string(), class: "PWR".to_string() })
            .expect("the net joins the class");
    }
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.8, drill: 0.5, ..Default::default() },
    )
    .expect("the via is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("B".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.5,
            from: [11.0, 8.0],
            to: [11.0, 12.0],
        },
    )
    .expect("the track is routed");

    let report = inspect(handle, Some(vec!["hole-to-track".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a gap exactly at the rule passes");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_track_ignores_pads_without_a_drill() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(10.0), y: Some(10.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("A".to_string()),
            drill: None,
        },
    )
    .expect("the plain pad is added");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("B".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.0, 8.0],
            to: [10.0, 12.0],
        },
    )
    .expect("the track crosses the pad centre");

    let report = inspect(handle, Some(vec!["hole-to-track".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a pad without a drill is not a hole");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_track_measures_the_segment_not_its_infinite_line() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("B".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [20.0, 10.1],
            to: [30.0, 10.1],
        },
    )
    .expect("the track is routed");

    let report = inspect(handle, Some(vec!["hole-to-track".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "the nearest point of the segment is far from the hole");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_track_orders_holes_before_tracks() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    for at in [[10.0, 10.0], [30.0, 30.0]] {
        add_via(handle, AddViaRequest { net: Some("A".to_string()), at, diameter: 0.6, drill: 0.3, ..Default::default() })
            .expect("the via is placed");
    }
    for (from, to) in [([10.25, 8.0], [10.25, 12.0]), ([9.75, 8.0], [9.75, 12.0]), ([30.25, 28.0], [30.25, 32.0])] {
        add_track(handle, AddTrackRequest { net: Some("B".to_string()), layer: "F.Cu".to_string(), width: 0.25, from, to })
            .expect("the track is routed");
    }

    let report = inspect(handle, Some(vec!["hole-to-track".to_string()])).expect("the check runs");
    let objects: Vec<&str> = report["issues"]
        .as_array()
        .expect("issues is an array")
        .iter()
        .filter_map(|issue| issue["objectId"].as_str())
        .collect();
    assert_eq!(
        objects,
        ["via-1+track-1", "via-1+track-2", "via-2+track-3"],
        "holes follow document order and each hole visits tracks in document order"
    );
    close(handle).expect("the board closes");
}

#[test]
fn inspect_runs_hole_to_track_without_an_explicit_list() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("B".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.25, 8.0],
            to: [10.25, 12.0],
        },
    )
    .expect("the track is routed");

    let report = inspect(handle, None).expect("the checks run");
    assert!(codes(&report).contains(&"drc/hole-to-track"), "an explicit-free run includes hole-to-track: {report}");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_pad_flags_a_via_on_another_nets_pad() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(10.0), y: Some(10.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("A".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    add_via(
        handle,
        AddViaRequest { net: Some("B".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");

    let report = inspect(handle, Some(vec!["hole-to-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/hole-to-pad"]);
    assert_eq!(report["passed"], false);
    let issue = &report["issues"][0];
    assert_eq!(issue["objectId"], "via-1+fp-1:1");
    assert_eq!(issue["layerId"], "F.Cu");
    let message = issue["message"].as_str().expect("a message");
    assert!(message.contains("drill hole `via-1`") && message.contains("pad `fp-1:1`"), "{message}");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_pad_skips_same_net_and_unassigned_pairs() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(10.0), y: Some(10.0), ..Default::default() },
    )
    .expect("the same-net footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("A".to_string()),
            drill: None,
        },
    )
    .expect("the same-net pad is added");
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the same-net via is placed");
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J2".to_string()), x: Some(30.0), y: Some(30.0), ..Default::default() },
    )
    .expect("the unassigned footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-2".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: None,
            drill: None,
        },
    )
    .expect("the unassigned pad is added");
    add_via(handle, AddViaRequest { net: None, at: [30.0, 30.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the unassigned via is placed");

    let report = inspect(handle, Some(vec!["hole-to-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "same-net and unassigned holes and pads are not compared");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_pad_accepts_a_gap_exactly_at_the_rule() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_net_class(handle, AddNetClassRequest { name: "PWR".to_string(), clearance: Some(0.5), ..Default::default() })
        .expect("the clearance class is declared");
    for net in ["A", "B"] {
        assign_net_class(handle, AssignNetClassRequest { net: net.to_string(), class: "PWR".to_string() })
            .expect("the net joins the class");
    }
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(10.0), y: Some(10.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("A".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    add_via(
        handle,
        AddViaRequest { net: Some("B".to_string()), at: [11.25, 10.0], diameter: 0.8, drill: 0.5, ..Default::default() },
    )
    .expect("the via is placed");

    let report = inspect(handle, Some(vec!["hole-to-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a gap exactly at the rule passes");
    close(handle).expect("the board closes");
}

#[test]
fn hole_to_pad_ignores_a_drilled_pads_own_hole() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(10.0), y: Some(10.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("A".to_string()),
            drill: Some(0.3),
        },
    )
    .expect("the drilled pad is added");

    let report = inspect(handle, Some(vec!["hole-to-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a drilled pad is not measured against its own drill");
    close(handle).expect("the board closes");
}

#[test]
fn inspect_runs_hole_to_pad_without_an_explicit_list() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("J1".to_string()), x: Some(10.0), y: Some(10.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("A".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    add_via(
        handle,
        AddViaRequest { net: Some("B".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");

    let report = inspect(handle, None).expect("the checks run");
    assert!(codes(&report).contains(&"drc/hole-to-pad"), "an explicit-free run includes hole-to-pad: {report}");
    close(handle).expect("the board closes");
}

#[test]
fn explicit_check_lists_still_filter_hole_to_track() {
    let handle = created();
    for name in ["A", "B"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("B".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.25, 8.0],
            to: [10.25, 12.0],
        },
    )
    .expect("the track is routed");

    let only_holes = inspect(handle, Some(vec!["hole-to-hole".to_string()])).expect("the check runs");
    assert_eq!(codes(&only_holes), Vec::<&str>::new(), "a check outside the list does not run");
    let only_track = inspect(handle, Some(vec!["hole-to-track".to_string()])).expect("the check runs");
    assert_eq!(codes(&only_track), ["drc/hole-to-track"]);
    close(handle).expect("the board closes");
}

#[test]
fn edge_clearance_flags_components_and_drills_near_the_border() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN32".to_string()),
            x: Some(0.2),
            y: Some(10.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the near footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [0.4, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the near via is placed");
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [20.0, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the far via is placed");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");

    let report = inspect(handle, Some(vec!["edge-clearance".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/component-to-edge", "drc/hole-to-edge"]);
    assert_eq!(report["passed"], false);
    assert_eq!(report["issues"][0]["objectId"], "fp-1");
    assert_eq!(report["issues"][1]["objectId"], "via-1");
    close(handle).expect("the board closes");

    let no_outline = created();
    add_via(no_outline, AddViaRequest { net: None, at: [0.4, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the via is placed");
    let report = inspect(no_outline, Some(vec!["edge-clearance".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a missing outline is not measured against invented geometry");
    close(no_outline).expect("the board closes");
}

#[test]
fn copper_to_edge_flags_copper_near_the_border_and_stays_quiet_without_an_outline() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("the net is declared");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.25, from: [0.2, 10.0], to: [10.0, 10.0] },
    )
    .expect("the track is routed");
    add_via(
        handle,
        AddViaRequest { net: Some("A".to_string()), at: [0.5, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");

    let report = inspect(handle, Some(vec!["copper-to-edge".to_string()])).expect("the check runs");
    let found = codes(&report);
    assert!(found.iter().all(|code| *code == "drc/copper-to-edge"), "got {found:?}");
    assert_eq!(found.len(), 2, "the near track and the near via are both reported");
    assert_eq!(report["passed"], false);
    close(handle).expect("the board closes");

    let no_outline = created();
    add_track(
        no_outline,
        AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.25, from: [0.2, 10.0], to: [10.0, 10.0] },
    )
    .expect("the track is routed");
    let report = inspect(no_outline, Some(vec!["copper-to-edge".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a missing outline is not measured against invented geometry");
    close(no_outline).expect("the board closes");

    let clear = created();
    add_track(
        clear,
        AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.25, from: [10.0, 10.0], to: [20.0, 10.0] },
    )
    .expect("the track is routed");
    set_outline(clear, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    let report = inspect(clear, Some(vec!["copper-to-edge".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "copper away from the edge passes");
    close(clear).expect("the board closes");
}

#[test]
fn silkscreen_over_pad_flags_a_silk_track_crossing_a_pad_on_the_same_side() {
    let handle = created();
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(20.0), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "F.SilkS".to_string(), width: 0.2, from: [18.0, 20.0], to: [22.0, 20.0] },
    )
    .expect("the silkscreen track is placed");

    let report = inspect(handle, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/silkscreen-over-pad"]);
    assert_eq!(report["issues"][0]["objectId"], "track-1>fp-1:1");
    assert_eq!(report["issues"][0]["layerId"], "F.SilkS");
    assert_eq!(report["passed"], false);
    close(handle).expect("the board closes");

    let other_side = created();
    add_footprint(
        other_side,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(20.0), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        other_side,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    add_track(
        other_side,
        AddTrackRequest { net: None, layer: "B.SilkS".to_string(), width: 0.2, from: [18.0, 20.0], to: [22.0, 20.0] },
    )
    .expect("the silkscreen track is placed");
    let report = inspect(other_side, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "bottom silkscreen does not overlap a top pad");
    close(other_side).expect("the board closes");

    let clear = created();
    add_footprint(
        clear,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(20.0), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        clear,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    add_track(clear, AddTrackRequest { net: None, layer: "F.SilkS".to_string(), width: 0.2, from: [0.0, 0.0], to: [5.0, 0.0] })
        .expect("the silkscreen track is placed");
    let report = inspect(clear, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "silkscreen away from pads passes");
    close(clear).expect("the board closes");
}

#[test]
fn silkscreen_over_pad_flags_silk_text_covering_a_pad_and_stays_deterministic() {
    let handle = created();
    add_footprint(
        handle,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(20.0), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    add_text(
        handle,
        AddTextRequest {
            text: "AB".to_string(),
            layer: "F.SilkS".to_string(),
            at: [20.0, 20.0],
            rotation: Some(0.0),
            size: None,
            thickness: None,
        },
    )
    .expect("the legend is placed");
    let revision = summary(handle).expect("the board is listed")["revision"].clone();

    let report = inspect(handle, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/silkscreen-over-pad"]);
    assert_eq!(report["issues"][0]["objectId"], "text-1>fp-1:1");
    assert_eq!(report["issues"][0]["layerId"], "F.SilkS");
    assert_eq!(report["passed"], false);
    assert_eq!(report["revision"], revision, "inspection never bumps the revision");
    assert_eq!(summary(handle).expect("the board is listed")["revision"], revision, "inspection leaves the revision alone");

    let repeat = inspect(handle, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs again");
    assert_eq!(codes(&repeat), codes(&report), "a repeat run reports the same findings in the same order");
    assert_eq!(repeat["issues"][0]["objectId"], report["issues"][0]["objectId"]);
    close(handle).expect("the board closes");
}

#[test]
fn silkscreen_over_pad_ignores_tangent_legend_and_the_other_side() {
    let tangent_text = created();
    add_footprint(
        tangent_text,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(20.0), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        tangent_text,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    add_text(
        tangent_text,
        AddTextRequest {
            text: "AB".to_string(),
            layer: "F.SilkS".to_string(),
            at: [20.0, 21.25],
            rotation: Some(0.0),
            size: None,
            thickness: None,
        },
    )
    .expect("the legend is placed");
    let report = inspect(tangent_text, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a legend box that touches the pad edge is contact, not overlap");
    close(tangent_text).expect("the board closes");

    let tangent_track = created();
    add_footprint(
        tangent_track,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(20.0), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        tangent_track,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 4.0,
            h: 4.0,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    add_track(
        tangent_track,
        AddTrackRequest { net: None, layer: "F.SilkS".to_string(), width: 2.0, from: [16.0, 23.0], to: [24.0, 23.0] },
    )
    .expect("the silkscreen track is placed");
    let report = inspect(tangent_track, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a stroke whose edge touches the pad is contact, not overlap");
    close(tangent_track).expect("the board closes");

    let other_side = created();
    add_footprint(
        other_side,
        AddFootprintRequest { reference: Some("U1".to_string()), x: Some(20.0), y: Some(20.0), ..Default::default() },
    )
    .expect("the footprint is placed");
    add_pad(
        other_side,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: None,
            drill: None,
        },
    )
    .expect("the pad is added");
    add_text(
        other_side,
        AddTextRequest {
            text: "AB".to_string(),
            layer: "B.SilkS".to_string(),
            at: [20.0, 20.0],
            rotation: Some(0.0),
            size: None,
            thickness: None,
        },
    )
    .expect("the legend is placed");
    let report = inspect(other_side, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "bottom silkscreen text does not overlap a top pad");
    close(other_side).expect("the board closes");
}

#[test]
fn png_export_writes_a_valid_deterministic_raster() {
    let directory = TempDir::new();
    let handle = created();
    add_full_board(handle);

    let first = directory.join("board.png");
    let report = export(handle, "png", &first).expect("PNG export writes");
    assert_eq!(report["format"], "png");
    assert_eq!(report["artifact"]["kind"], "png");
    assert_eq!(report["artifact"]["width"], 1024);
    assert_eq!(report["artifact"]["height"], 768);
    assert_eq!(report["artifact"]["tracks"], 1);

    let bytes = fs::read(&first).expect("the PNG is readable");
    assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a], "PNG signature");
    assert!(bytes.len() < 300_000, "the PNG is deflate-compressed, not stored: {} bytes", bytes.len());
    let mut offset = 8;
    let mut saw_ihdr = false;
    while offset < bytes.len() {
        let length = u32::from_be_bytes(bytes[offset..offset + 4].try_into().expect("a chunk length")) as usize;
        let kind = &bytes[offset + 4..offset + 8];
        let data = &bytes[offset + 8..offset + 8 + length];
        let stored = u32::from_be_bytes(bytes[offset + 8 + length..offset + 12 + length].try_into().expect("a chunk CRC"));
        assert_eq!(test_crc32(&bytes[offset + 4..offset + 8 + length]), stored, "CRC of each chunk validates");
        if kind == b"IHDR" {
            saw_ihdr = true;
            assert_eq!(u32::from_be_bytes(data[..4].try_into().expect("IHDR width")), 1024);
            assert_eq!(&data[4..8], &[0, 0, 3, 0]);
        }
        offset += 12 + length;
    }
    assert!(saw_ihdr, "the stream contains an IHDR chunk");
    assert_eq!(offset, bytes.len(), "the chunk stream is complete");

    let second = directory.join("board-again.png");
    export(handle, "png", &second).expect("PNG export writes again");
    assert_eq!(bytes, fs::read(&second).expect("the second PNG is readable"), "PNG bytes are deterministic");

    let empty = created();
    let refused = export(empty, "png", &directory.join("empty.png")).expect_err("an empty board is rejected");
    assert_eq!(refused.code, "format/invalid");
    close(empty).expect("the empty board closes");
    close(handle).expect("the board closes");
}

fn test_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn inner_layers(handle: u64, names: &[&str]) {
    for name in names {
        add_layer(handle, AddLayerRequest { name: (*name).to_string(), kind: Some("copper".to_string()), color: None })
            .expect("the inner copper layer is added");
    }
}

#[test]
fn blind_and_buried_vias_record_their_span() {
    let handle = created();
    inner_layers(handle, &["In1.Cu", "In2.Cu"]);

    let through = add_via(handle, AddViaRequest { at: [1.0, 1.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the through via is placed");
    assert_eq!(through["object"]["kind"], "through");
    assert_eq!(through["object"]["fromLayer"], "F.Cu");
    assert_eq!(through["object"]["toLayer"], "B.Cu");

    let blind = add_via(
        handle,
        AddViaRequest {
            at: [2.0, 2.0],
            diameter: 0.6,
            drill: 0.3,
            from_layer: Some("F.Cu".to_string()),
            to_layer: Some("In1.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the blind via is placed");
    assert_eq!(blind["object"]["kind"], "blind");
    assert_eq!(blind["object"]["fromLayer"], "F.Cu");
    assert_eq!(blind["object"]["toLayer"], "In1.Cu");

    let buried = add_via(
        handle,
        AddViaRequest {
            at: [3.0, 3.0],
            diameter: 0.6,
            drill: 0.3,
            from_layer: Some("In1.Cu".to_string()),
            to_layer: Some("In2.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the buried via is placed");
    assert_eq!(buried["object"]["kind"], "buried");
    assert_eq!(buried["object"]["fromLayer"], "In1.Cu");
    assert_eq!(buried["object"]["toLayer"], "In2.Cu");

    let summary = summary(handle).expect("the board is open");
    let count = |id: &str| {
        summary["layers"]
            .as_array()
            .expect("layers is an array")
            .iter()
            .find(|layer| layer["id"] == id)
            .expect("the layer is listed")["objects"]
            .as_u64()
            .expect("objects is a count")
    };
    assert_eq!(count("F.Cu"), 2, "the through and blind vias reach F.Cu");
    assert_eq!(count("In1.Cu"), 3, "every via reaches In1");
    assert_eq!(count("In2.Cu"), 2, "the through and buried vias reach In2");
    assert_eq!(count("B.Cu"), 1, "only the through via reaches B.Cu");

    let directory = TempDir::new();
    let path = directory.join("vias.opcb");
    save(handle, &path, false).expect("the board saves");
    let value: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("the document is readable")).expect("the document is JSON");
    let vias = value["document"]["vias"].as_array().expect("vias is an array");
    assert!(vias[0].get("from_layer").is_none(), "a through via omits its span");
    assert!(vias[0].get("to_layer").is_none());
    assert_eq!(vias[1]["from_layer"], "F.Cu");
    assert_eq!(vias[1]["to_layer"], "In1.Cu");
    assert_eq!(vias[2]["from_layer"], "In1.Cu");
    assert_eq!(vias[2]["to_layer"], "In2.Cu");
    close(handle).expect("the board closes");

    let reopened = open(&path).expect("the document reopens");
    let reopened_handle = handle_of(&reopened);
    let kinds =
        store::read(reopened_handle, |board| board.vias.iter().map(|via| board.via_kind(via).to_string()).collect::<Vec<_>>())
            .expect("the board is readable");
    assert_eq!(kinds, ["through", "blind", "buried"]);
    close(reopened_handle).expect("the reopened board closes");
}

#[test]
fn via_spans_reject_invalid_arguments_and_locked_layers() {
    let handle = created();
    inner_layers(handle, &["In1.Cu"]);

    let request = |from: Option<&str>, to: Option<&str>| AddViaRequest {
        at: [0.0, 0.0],
        diameter: 0.6,
        drill: 0.3,
        from_layer: from.map(str::to_string),
        to_layer: to.map(str::to_string),
        ..Default::default()
    };
    for (from, to) in [
        (None, Some("In1.Cu")),
        (Some("In1.Cu"), None),
        (Some("F.Cu"), Some("F.Cu")),
        (Some("F.Cu"), Some("X.Cu")),
        (Some("F.SilkS"), Some("B.Cu")),
        (Some("B.Cu"), Some("F.Cu")),
    ] {
        assert_eq!(
            add_via(handle, request(from, to)).expect_err("the span is rejected").code,
            "session/argument-invalid",
            "{from:?} -> {to:?}"
        );
    }

    update_layer(handle, UpdateLayerRequest { id: "In1.Cu".to_string(), locked: Some(true), ..Default::default() })
        .expect("the inner layer is locked");
    assert_eq!(
        add_via(handle, request(Some("F.Cu"), Some("In1.Cu"))).expect_err("a locked span layer is rejected").code,
        "board/layer-locked"
    );
    assert_eq!(
        add_via(handle, request(Some("In1.Cu"), Some("B.Cu"))).expect_err("a locked span layer is rejected").code,
        "board/layer-locked"
    );
    add_via(handle, request(None, None)).expect("a through via keeps checking only the outer layers");
    update_layer(handle, UpdateLayerRequest { id: "In1.Cu".to_string(), locked: Some(false), ..Default::default() })
        .expect("the inner layer is unlocked");

    let blind = add_via(handle, request(Some("F.Cu"), Some("In1.Cu"))).expect("the blind via is placed after unlocking");
    let blind_id = blind["object"]["id"].as_str().expect("the via id").to_string();
    update_layer(handle, UpdateLayerRequest { id: "In1.Cu".to_string(), locked: Some(true), ..Default::default() })
        .expect("the inner layer is locked");
    assert_eq!(delete_via(handle, &blind_id).expect_err("a locked span rejects deletes").code, "board/layer-locked");
    close(handle).expect("the board closes");

    let mut board = crafted();
    board.vias.push(Via {
        id: "via-1".to_string(),
        net: None,
        at: [0.0, 0.0],
        diameter: 0.6,
        drill: 0.3,
        from_layer: Some("F.Cu".to_string()),
        to_layer: None,
        micro: false,
        back_drill: None,
    });
    let directory = TempDir::new();
    let path = directory.join("half-span.opcb");
    fs::write(&path, serde_json::to_string(&Document::new(board)).expect("the document serializes"))
        .expect("the document is written");
    assert_eq!(open(&path).expect_err("a half span is rejected").code, "format/invalid");

    let mut reversed = crafted();
    reversed.vias.push(Via {
        id: "via-1".to_string(),
        net: None,
        at: [0.0, 0.0],
        diameter: 0.6,
        drill: 0.3,
        from_layer: Some("B.Cu".to_string()),
        to_layer: Some("F.Cu".to_string()),
        micro: false,
        back_drill: None,
    });
    let reversed_path = directory.join("reversed-span.opcb");
    fs::write(&reversed_path, serde_json::to_string(&Document::new(reversed)).expect("the document serializes"))
        .expect("the document is written");
    assert_eq!(open(&reversed_path).expect_err("a reversed span is rejected").code, "format/invalid");
}

#[test]
fn micro_vias_round_trip_and_reject_illegal_spans() {
    let handle = created();
    inner_layers(handle, &["In1.Cu", "In2.Cu"]);

    let micro = add_via(
        handle,
        AddViaRequest {
            at: [1.0, 1.0],
            diameter: 0.3,
            drill: 0.15,
            from_layer: Some("F.Cu".to_string()),
            to_layer: Some("In1.Cu".to_string()),
            kind: Some("micro".to_string()),
            ..Default::default()
        },
    )
    .expect("the micro via is placed");
    assert_eq!(micro["object"]["kind"], "micro");
    assert_eq!(micro["object"]["fromLayer"], "F.Cu");
    assert_eq!(micro["object"]["toLayer"], "In1.Cu");

    add_via(
        handle,
        AddViaRequest {
            at: [2.0, 2.0],
            diameter: 0.3,
            drill: 0.15,
            from_layer: Some("In2.Cu".to_string()),
            to_layer: Some("B.Cu".to_string()),
            kind: Some("micro".to_string()),
            ..Default::default()
        },
    )
    .expect("the micro via on the back outer layer is placed");

    for (from, to) in [(Some("F.Cu"), Some("In2.Cu")), (Some("In1.Cu"), Some("In2.Cu")), (None, None)] {
        let error = add_via(
            handle,
            AddViaRequest {
                at: [3.0, 3.0],
                diameter: 0.3,
                drill: 0.15,
                from_layer: from.map(str::to_string),
                to_layer: to.map(str::to_string),
                kind: Some("micro".to_string()),
                ..Default::default()
            },
        )
        .expect_err("an illegal micro span is rejected");
        assert_eq!(error.code, "via/micro-span", "{from:?} -> {to:?}");
    }

    assert_eq!(
        add_via(
            handle,
            AddViaRequest {
                at: [3.0, 3.0],
                diameter: 0.3,
                drill: 0.15,
                from_layer: Some("F.Cu".to_string()),
                to_layer: Some("In1.Cu".to_string()),
                kind: Some("blind".to_string()),
                ..Default::default()
            },
        )
        .expect_err("only micro is accepted as an explicit type")
        .code,
        "session/argument-invalid"
    );

    let blind = add_via(
        handle,
        AddViaRequest {
            at: [4.0, 4.0],
            diameter: 0.3,
            drill: 0.15,
            from_layer: Some("F.Cu".to_string()),
            to_layer: Some("In1.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the blind via is placed");
    let blind_id = blind["object"]["id"].as_str().expect("the via id").to_string();
    let updated =
        update_via(handle, UpdateViaRequest { id: blind_id.clone(), kind: Some("micro".to_string()), ..Default::default() })
            .expect("the blind via becomes micro");
    assert_eq!(updated["object"]["kind"], "micro");

    let through = add_via(handle, AddViaRequest { at: [5.0, 5.0], diameter: 0.3, drill: 0.15, ..Default::default() })
        .expect("the through via is placed");
    let through_id = through["object"]["id"].as_str().expect("the via id").to_string();
    assert_eq!(
        update_via(handle, UpdateViaRequest { id: through_id, kind: Some("micro".to_string()), ..Default::default() })
            .expect_err("a through span cannot become micro")
            .code,
        "via/micro-span"
    );
    assert_eq!(
        update_via(handle, UpdateViaRequest { id: blind_id, kind: Some("buried".to_string()), ..Default::default() })
            .expect_err("only micro is accepted as an explicit type")
            .code,
        "session/argument-invalid"
    );

    let directory = TempDir::new();
    let path = directory.join("micro.opcb");
    save(handle, &path, false).expect("the board saves");
    let value: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("the document is readable")).expect("the document is JSON");
    let vias = value["document"]["vias"].as_array().expect("vias is an array");
    let flags: Vec<bool> = vias.iter().map(|via| via["micro"].as_bool().unwrap_or(false)).collect();
    assert_eq!(flags, [true, true, true, false], "micro flags persist and a through via omits the field");
    assert!(vias[3].get("micro").is_none(), "a through via keeps the historical document shape");
    close(handle).expect("the board closes");

    let reopened = open(&path).expect("the document reopens");
    let reopened_handle = handle_of(&reopened);
    let kinds =
        store::read(reopened_handle, |board| board.vias.iter().map(|via| board.via_kind(via).to_string()).collect::<Vec<_>>())
            .expect("the board is readable");
    assert_eq!(kinds, ["micro", "micro", "micro", "through"]);
    close(reopened_handle).expect("the reopened board closes");
}

#[test]
fn drill_report_lists_micro_vias_as_holes() {
    let handle = created();
    inner_layers(handle, &["In1.Cu"]);
    add_via(
        handle,
        AddViaRequest {
            at: [10.0, 10.0],
            diameter: 0.3,
            drill: 0.15,
            from_layer: Some("F.Cu".to_string()),
            to_layer: Some("In1.Cu".to_string()),
            kind: Some("micro".to_string()),
            ..Default::default()
        },
    )
    .expect("the micro via is placed");
    add_via(handle, AddViaRequest { at: [20.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the through via is placed");

    let report = drill_report(handle).expect("the drill report is built");
    assert_eq!(report["holeCount"], 2, "the micro via is a hole like any other via");
    assert_eq!(report["vias"]["micro"], 1);
    assert_eq!(report["vias"]["through"], 1);
    let holes = report["holes"].as_array().expect("holes is an array");
    let micro = holes.iter().find(|hole| hole["at"][0].as_f64() == Some(10.0)).expect("the micro hole is listed");
    assert_eq!(micro["kind"], "micro");
    assert_eq!(micro["diameter"], 0.15);
    assert!(micro["tool"].as_u64().is_some_and(|tool| tool > 0), "the micro hole owns a tool");
    close(handle).expect("the board closes");
}

#[test]
fn kicad_import_keeps_legal_micro_spans_and_reports_illegal_ones() {
    let directory = TempDir::new();
    let path = directory.join("micro.kicad_pcb");
    let text = "(kicad_pcb (version 20221018) (generator open-pcb)\n\
        (layers (0 \"F.Cu\" signal) (1 \"In1.Cu\" signal) (2 \"In2.Cu\" signal) (31 \"B.Cu\" signal))\n\
        (net 0 \"\")\n\
        (via (at 1 1) (size 0.3) (drill 0.15) (layers \"F.Cu\" \"In1.Cu\") (type micro))\n\
        (via (at 2 2) (size 0.3) (drill 0.15) (layers \"In2.Cu\" \"B.Cu\") (type micro))\n\
        (via (at 3 3) (size 0.3) (drill 0.15) (layers \"F.Cu\" \"In2.Cu\") (type micro))\n\
        (via (at 4 4) (size 0.3) (drill 0.15) (layers \"In1.Cu\" \"In2.Cu\") (type micro)))\n";
    fs::write(&path, text).expect("the fixture is written");
    let imported = import(&path).expect("the annotated board imports");
    assert_eq!(imported["document"]["counts"]["vias"], 2, "only legal micro spans are imported");
    let codes: Vec<&str> =
        imported["lossCodes"].as_array().expect("codes are listed").iter().filter_map(Value::as_str).collect();
    assert_eq!(codes, ["kicad/via-layer-span"]);
    let imported_handle = handle_of(&imported);
    let (spans, kinds) = store::read(imported_handle, |board| {
        let spans = board.vias.iter().map(|via| (via.from_layer.clone(), via.to_layer.clone())).collect::<Vec<_>>();
        (spans, board.vias.iter().map(|via| board.via_kind(via).to_string()).collect::<Vec<_>>())
    })
    .expect("the imported board is readable");
    assert_eq!(
        spans,
        [(Some("F.Cu".to_string()), Some("In1.Cu".to_string())), (Some("In2.Cu".to_string()), Some("B.Cu".to_string()))]
    );
    assert_eq!(kinds, ["micro", "micro"]);
    close(imported_handle).expect("the imported board closes");
}

#[test]
fn kicad_export_writes_the_micro_token_and_reimports_it() {
    let directory = TempDir::new();
    let handle = created();
    inner_layers(handle, &["In1.Cu"]);
    add_via(
        handle,
        AddViaRequest {
            at: [1.0, 1.0],
            diameter: 0.3,
            drill: 0.15,
            from_layer: Some("F.Cu".to_string()),
            to_layer: Some("In1.Cu".to_string()),
            kind: Some("micro".to_string()),
            ..Default::default()
        },
    )
    .expect("the micro via is placed");
    add_via(handle, AddViaRequest { at: [2.0, 2.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the through via is placed");

    let target = directory.join("micro.kicad_pcb");
    export(handle, "kicad_pcb", &target).expect("KiCad export writes");
    let text = fs::read_to_string(&target).expect("the KiCad board is readable");
    assert_eq!(text.matches("(type micro)").count(), 1, "only the micro via carries the token: {text}");
    assert!(text.contains("(layers \"F.Cu\" \"In1.Cu\")"), "the micro span is written: {text}");
    close(handle).expect("the board closes");

    let imported = import(&target).expect("the exported board imports");
    assert!(imported["losses"].as_array().expect("losses is an array").is_empty(), "the round trip loses nothing");
    let imported_handle = handle_of(&imported);
    let kinds =
        store::read(imported_handle, |board| board.vias.iter().map(|via| board.via_kind(via).to_string()).collect::<Vec<_>>())
            .expect("the imported board is readable");
    assert_eq!(kinds, ["micro", "through"]);
    close(imported_handle).expect("the imported board closes");
}

#[test]
fn unrouted_respects_the_via_layer_span() {
    let handle = created();
    inner_layers(handle, &["In1.Cu"]);
    add_net(handle, AddNetRequest { name: "N1".to_string() }).expect("the net is declared");
    for (index, (reference, layer)) in [("U1", "F.Cu"), ("U2", "B.Cu")].iter().enumerate() {
        add_footprint(
            handle,
            AddFootprintRequest {
                reference: Some((*reference).to_string()),
                x: Some(10.0 + 20.0 * index as f64),
                y: Some(10.0),
                layer: Some((*layer).to_string()),
                ..Default::default()
            },
        )
        .expect("the footprint is placed");
        add_pad(
            handle,
            AddPadRequest {
                footprint: format!("fp-{}", index + 1),
                number: "1".to_string(),
                x: 0.0,
                y: 0.0,
                w: 0.5,
                h: 0.5,
                net: Some("N1".to_string()),
                drill: None,
            },
        )
        .expect("the pad is added");
    }
    add_track(
        handle,
        AddTrackRequest {
            net: Some("N1".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.0, 10.0],
            to: [20.0, 10.0],
        },
    )
    .expect("the front track is routed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("N1".to_string()),
            layer: "B.Cu".to_string(),
            width: 0.25,
            from: [30.0, 10.0],
            to: [20.0, 10.0],
        },
    )
    .expect("the back track is routed");
    add_via(
        handle,
        AddViaRequest {
            net: Some("N1".to_string()),
            at: [20.0, 10.0],
            diameter: 0.6,
            drill: 0.3,
            from_layer: Some("F.Cu".to_string()),
            to_layer: Some("In1.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the blind via is placed");

    let report = inspect(handle, Some(vec!["unrouted".to_string()])).expect("the check runs");
    assert!(codes(&report).contains(&"drc/unrouted"), "a via that does not reach B.Cu does not join the B.Cu track: {report}");

    add_via(
        handle,
        AddViaRequest { net: Some("N1".to_string()), at: [20.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the through via is placed");
    let report = inspect(handle, Some(vec!["unrouted".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "a through via joins both copper layers");
    close(handle).expect("the board closes");
}

#[test]
fn exports_write_one_gerber_per_copper_layer_and_keep_every_hole() {
    let directory = TempDir::new();
    let handle = created();
    inner_layers(handle, &["In1.Cu", "In2.Cu"]);
    add_via(
        handle,
        AddViaRequest {
            at: [5.0, 5.0],
            diameter: 0.6,
            drill: 0.3,
            from_layer: Some("F.Cu".to_string()),
            to_layer: Some("In1.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the blind via is placed");
    add_via(
        handle,
        AddViaRequest {
            at: [6.0, 6.0],
            diameter: 0.6,
            drill: 0.3,
            from_layer: Some("In1.Cu".to_string()),
            to_layer: Some("In2.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the buried via is placed");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [40.0, 30.0] }).expect("the outline is set");

    let gbr = directory.join("gerber");
    let report = export(handle, "gbr", &gbr).expect("Gerber export writes");
    let artifacts = report["artifacts"].as_array().expect("artifacts is an array");
    let names: Vec<&str> = artifacts.iter().map(|item| item["name"].as_str().expect("a name")).collect();
    assert_eq!(
        names,
        [
            "F_Cu.gbr",
            "In1_Cu.gbr",
            "In2_Cu.gbr",
            "B_Cu.gbr",
            "F_SilkS.gbr",
            "B_SilkS.gbr",
            "F_Mask.gbr",
            "B_Mask.gbr",
            "F_Paste.gbr",
            "B_Paste.gbr",
            "Edge_Cuts.gbr"
        ]
    );

    let read = |name: &str| fs::read_to_string(gbr.join(name)).expect("the layer is readable");
    let front = read("F_Cu.gbr");
    assert!(front.contains("X5000000Y5000000D03*"), "the blind via is flashed on F.Cu");
    assert!(!front.contains("X6000000Y6000000D03*"), "the buried via is not flashed on F.Cu");
    let first_inner = read("In1_Cu.gbr");
    assert!(first_inner.contains("X5000000Y5000000D03*"), "the blind via reaches In1");
    assert!(first_inner.contains("X6000000Y6000000D03*"), "the buried via starts on In1");
    let second_inner = read("In2_Cu.gbr");
    assert!(!second_inner.contains("X5000000Y5000000D03*"), "the blind via does not reach In2");
    assert!(second_inner.contains("X6000000Y6000000D03*"), "the buried via reaches In2");
    let back = read("B_Cu.gbr");
    assert!(!back.contains("D03*"), "neither via reaches B.Cu");

    let drill = directory.join("board.drl");
    export(handle, "drl", &drill).expect("drill export writes");
    let drill = fs::read_to_string(&drill).expect("the drill file is readable");
    assert!(drill.contains("X5.000Y5.000"), "the blind hole is drilled");
    assert!(drill.contains("X6.000Y6.000"), "the buried hole is drilled");

    let svg = directory.join("board.svg");
    export(handle, "svg", &svg).expect("SVG export writes");
    let svg = fs::read_to_string(&svg).expect("the SVG is readable");
    let circles = svg.matches("<ellipse").count() + svg.matches("<circle").count();
    assert!(circles >= 2, "both vias are drawn; got {circles}");

    let kicad = directory.join("board.kicad_pcb");
    export(handle, "kicad_pcb", &kicad).expect("KiCad export writes");
    let kicad = fs::read_to_string(&kicad).expect("the KiCad board is readable");
    assert!(kicad.contains("(layers \"F.Cu\" \"In1.Cu\")"), "the blind span is written: {kicad}");
    assert!(kicad.contains("(layers \"In1.Cu\" \"In2.Cu\")"), "the buried span is written: {kicad}");
    close(handle).expect("the board closes");
}

#[test]
fn kicad_import_round_trips_blind_and_buried_vias() {
    let directory = TempDir::new();
    let source = directory.join("spans.kicad_pcb");
    let handle = created();
    inner_layers(handle, &["In1.Cu", "In2.Cu"]);
    for (at, from, to) in
        [([1.0, 1.0], None, None), ([2.0, 2.0], Some("F.Cu"), Some("In1.Cu")), ([3.0, 3.0], Some("In1.Cu"), Some("In2.Cu"))]
    {
        add_via(
            handle,
            AddViaRequest {
                at,
                diameter: 0.6,
                drill: 0.3,
                from_layer: from.map(str::to_string),
                to_layer: to.map(str::to_string),
                ..Default::default()
            },
        )
        .expect("the via is placed");
    }
    export(handle, "kicad_pcb", &source).expect("KiCad export writes");
    let text = fs::read_to_string(&source).expect("the KiCad board is readable");
    assert!(text.contains("(layers \"F.Cu\" \"In1.Cu\")"), "the blind span is written: {text}");
    assert!(text.contains("(layers \"In1.Cu\" \"In2.Cu\")"), "the buried span is written: {text}");

    let imported = import(&source).expect("the exported board imports");
    assert!(imported["losses"].as_array().expect("losses is an array").is_empty(), "the round trip loses nothing");
    assert_eq!(imported["document"]["counts"]["vias"], 3);
    let imported_handle = handle_of(&imported);
    let spans = store::read(imported_handle, |board| {
        board.vias.iter().map(|via| (via.from_layer.clone(), via.to_layer.clone())).collect::<Vec<_>>()
    })
    .expect("the imported board is readable");
    assert_eq!(
        spans,
        [
            (None, None),
            (Some("F.Cu".to_string()), Some("In1.Cu".to_string())),
            (Some("In1.Cu".to_string()), Some("In2.Cu".to_string())),
        ]
    );
    let kinds =
        store::read(imported_handle, |board| board.vias.iter().map(|via| board.via_kind(via).to_string()).collect::<Vec<_>>())
            .expect("the imported board is readable");
    assert_eq!(kinds, ["through", "blind", "buried"]);
    close(imported_handle).expect("the imported board closes");
    close(handle).expect("the source board closes");
}

#[test]
fn kicad_import_reports_micro_spans_and_unknown_via_types() {
    let directory = TempDir::new();
    let path = directory.join("via-types.kicad_pcb");
    let text = "(kicad_pcb (version 20221018) (generator open-pcb)\n\
        (layers (0 \"F.Cu\" signal) (1 \"In1.Cu\" signal) (31 \"B.Cu\" signal))\n\
        (net 0 \"\")\n\
        (via (at 1 1) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\") (type micro))\n\
        (via (at 2 2) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\") (type foo))\n\
        (via (at 3 3) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"B.Cu\") (type through))\n\
        (via (at 4 4) (size 0.6) (drill 0.3) (layers \"F.Cu\" \"In1.Cu\") (type blind))\n\
        (via (at 5 5) (size 0.6) (drill 0.3) (layers \"In1.Cu\" \"B.Cu\") (type blind))\n\
        (via (at 6 6) (size 0.6) (drill 0.3) (layers \"F.Cu\")))\n";
    fs::write(&path, text).expect("the fixture is written");
    let imported = import(&path).expect("the annotated board imports");
    assert_eq!(imported["document"]["counts"]["vias"], 3, "only the valid spans are imported");
    let codes: Vec<&str> =
        imported["lossCodes"].as_array().expect("codes are listed").iter().filter_map(Value::as_str).collect();
    assert_eq!(codes, ["kicad/via-layer-span", "kicad/via-type"]);
    let imported_handle = handle_of(&imported);
    let spans = store::read(imported_handle, |board| {
        board.vias.iter().map(|via| (via.from_layer.clone(), via.to_layer.clone())).collect::<Vec<_>>()
    })
    .expect("the imported board is readable");
    assert_eq!(
        spans,
        [
            (None, None),
            (Some("F.Cu".to_string()), Some("In1.Cu".to_string())),
            (Some("In1.Cu".to_string()), Some("B.Cu".to_string())),
        ]
    );
    close(imported_handle).expect("the imported board closes");
}

#[test]
fn kicad_import_refuses_a_non_rectangular_outline() {
    let directory = TempDir::new();
    let path = directory.join("triangle.kicad_pcb");
    let text = "(kicad_pcb (version 20221018)\n\
        (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal))\n\
        (net 0 \"\")\n\
        (gr_line (start 0 0) (end 10 0) (layer \"Edge.Cuts\"))\n\
        (gr_line (start 10 0) (end 0 10) (layer \"Edge.Cuts\"))\n\
        (gr_line (start 0 10) (end 0 0) (layer \"Edge.Cuts\")))\n";
    fs::write(&path, text).expect("the fixture is written");
    let imported = import(&path).expect("the triangle imports without an outline");
    assert_eq!(imported["document"]["outline"], Value::Null, "a non-rectangular outline is not approximated");
    let codes: Vec<&str> =
        imported["lossCodes"].as_array().expect("codes are listed").iter().filter_map(Value::as_str).collect();
    assert!(codes.contains(&"kicad/outline-not-rectangular"), "got {codes:?}");
    close(handle_of(&imported)).expect("the imported board closes");
}

#[test]
fn ipc2581_import_round_trips_an_exported_board() {
    let directory = TempDir::new();
    let source = directory.join("blinker.ipc2581");
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN32".to_string()),
            x: Some(30.0),
            y: Some(20.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the front footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.6,
            h: 0.4,
            net: Some("VCC".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("R1".to_string()),
            value: Some("10k".to_string()),
            footprint: Some("0402".to_string()),
            x: Some(50.0),
            y: Some(30.0),
            rotation: Some(90.0),
            layer: Some("B.Cu".to_string()),
        },
    )
    .expect("the back footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-2".to_string(),
            number: "1".to_string(),
            x: 0.5,
            y: -0.5,
            w: 0.5,
            h: 0.5,
            net: Some("GND".to_string()),
            drill: None,
        },
    )
    .expect("the back pad is added");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.0, 10.0],
            to: [30.0, 10.0],
        },
    )
    .expect("the front track is routed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("GND".to_string()),
            layer: "B.Cu".to_string(),
            width: 0.3,
            from: [30.0, 10.0],
            to: [50.0, 10.0],
        },
    )
    .expect("the back track is routed");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "F.SilkS".to_string(), width: 0.2, from: [2.0, 2.0], to: [6.0, 2.0] },
    )
    .expect("the silkscreen legend is drawn");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [20.0, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [100.0, 80.0] }).expect("the outline is set");
    export(handle, "ipc2581", &source).expect("IPC-2581 export writes");

    let imported = import(&source).expect("the exported IPC-2581 board imports");
    let imported_handle = handle_of(&imported);
    assert_eq!(imported["format"], "ipc2581");
    assert_eq!(imported["editable"], true);
    assert_eq!(
        imported["losses"].as_array().expect("losses is an array"),
        &Vec::<Value>::new(),
        "the exporter round trips without losses"
    );
    let document = &imported["document"];
    assert_eq!(document["counts"]["nets"], 2);
    assert_eq!(document["counts"]["footprints"], 2);
    assert_eq!(document["counts"]["tracks"], 3);
    assert_eq!(document["counts"]["vias"], 1);
    assert_eq!(document["outline"]["from"][0], 0.0);
    assert_eq!(document["outline"]["to"][1], 80.0);
    let layers: Vec<&str> = document["layers"]
        .as_array()
        .expect("layers is an array")
        .iter()
        .map(|layer| layer["id"].as_str().expect("layer ids are strings"))
        .collect();
    assert_eq!(layers, ["F.Cu", "B.Cu", "F.SilkS", "B.SilkS", "Edge.Cuts"]);

    let saved = directory.join("imported.opcb");
    save(imported_handle, &saved, false).expect("the imported board saves as a native document");
    let text = fs::read_to_string(&saved).expect("the native document is readable");
    let native: Value = serde_json::from_str(&text).expect("the native document is JSON");
    let board = &native["document"];
    assert_eq!(board["nets"][0]["name"], "VCC");
    assert_eq!(board["nets"][1]["name"], "GND");
    assert_eq!(board["footprints"][0]["ref"], "U1");
    assert_eq!(board["footprints"][0]["value"], "MCU");
    assert_eq!(board["footprints"][0]["x"], 30.0);
    assert_eq!(board["footprints"][0]["pads"][0]["net"], "VCC");
    assert_eq!(board["footprints"][1]["rotation"], 90.0);
    assert_eq!(board["footprints"][1]["pads"][0]["x"], 0.5);
    assert_eq!(board["footprints"][1]["pads"][0]["y"], -0.5);
    let tracks = board["tracks"].as_array().expect("tracks is an array");
    assert_eq!(tracks.len(), 3);
    let track_layers: Vec<&str> = tracks.iter().map(|track| track["layer"].as_str().expect("a layer")).collect();
    assert_eq!(track_layers, ["F.Cu", "B.Cu", "F.SilkS"]);
    assert_eq!(tracks[0]["net"], "VCC");
    assert_eq!(tracks[1]["net"], "GND");
    assert_eq!(tracks[2]["net"], Value::Null);
    assert_eq!(board["vias"][0]["at"][0], 20.0);
    assert_eq!(board["vias"][0]["diameter"], 0.6);
    assert_eq!(board["vias"][0]["drill"], 0.3);
    close(imported_handle).expect("the imported board closes");

    let reopened = open(&saved).expect("the saved import reopens");
    let reopened_handle = handle_of(&reopened);
    let inspected = inspect(reopened_handle, None).expect("the imported board passes inspection");
    assert_eq!(inspected["passed"], true, "{}", inspected["issues"]);
    close(reopened_handle).expect("the reopened import closes");
    close(handle).expect("the source board closes");
}

#[test]
fn ipc2581_import_reads_namespaced_documents_and_reports_losses() {
    let directory = TempDir::new();
    let path = directory.join("panel.ipc2581");
    let text = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
        <ipc:IPC-2581 xmlns:ipc=\"http://webstds.ipc.org/2581\" revision=\"B\" units=\"inch\">\n\
          <!-- the reader ignores comments -->\n\
          <ipc:Ecad><ipc:Content><ipc:Step name=\"Panel\">\n\
            <ipc:Profile><ipc:Polyline>\n\
              <ipc:Line startX=\"0\" startY=\"0\" endX=\"1\" endY=\"0\"/>\n\
              <ipc:Line startX=\"1\" startY=\"0\" endX=\"0\" endY=\"1\"/>\n\
              <ipc:Line startX=\"0\" startY=\"1\" endX=\"0\" endY=\"0\"/>\n\
            </ipc:Polyline></ipc:Profile>\n\
            <ipc:Package name=\"R0402\">\n\
              <ipc:Pin number=\"1\"><ipc:Location x=\"0.1\" y=\"0\"/><ipc:StandardPrimitive><ipc:Rect width=\"0.05\" height=\"0.06\"/></ipc:StandardPrimitive></ipc:Pin>\n\
              <ipc:Pin number=\"2\"><ipc:StandardPrimitive><ipc:Circle diameter=\"0.05\"/></ipc:StandardPrimitive></ipc:Pin>\n\
            </ipc:Package>\n\
            <ipc:Component refDes=\"R1\" part=\"R0402\" value=\"10k\" layerRef=\"TOP\" x=\"1\" y=\"1\"/>\n\
            <ipc:Component refDes=\"R2\" part=\"MISSING\" layerRef=\"BOTTOM\" x=\"2\" y=\"1\"/>\n\
            <ipc:LogicalNet name=\"GND\"/>\n\
            <ipc:LayerFeature layerRef=\"TOP\"><ipc:Set>\n\
              <ipc:Line startX=\"0\" startY=\"0\" endX=\"0.5\" endY=\"0\" width=\"0.01\" net=\"VCC\"/>\n\
            </ipc:Set></ipc:LayerFeature>\n\
            <ipc:LayerFeature layerRef=\"BOTTOM\">\n\
              <ipc:Hole x=\"0.2\" y=\"0.2\" diameter=\"0.02\" plating=\"non-plated\"/>\n\
            </ipc:LayerFeature>\n\
          </ipc:Step></ipc:Content></ipc:Ecad>\n\
        </ipc:IPC-2581>\n";
    fs::write(&path, text).expect("the fixture is written");
    let imported = import(&path).expect("the namespaced document imports");
    assert_eq!(imported["format"], "ipc2581");
    assert_eq!(imported["document"]["outline"], Value::Null, "a triangle is not approximated");
    assert_eq!(imported["document"]["counts"]["nets"], 1);
    assert_eq!(imported["document"]["counts"]["footprints"], 2, "a component with a missing package keeps its placement");
    assert_eq!(imported["document"]["counts"]["tracks"], 1);
    assert_eq!(imported["document"]["counts"]["vias"], 0);

    let saved = directory.join("panel.opcb");
    save(handle_of(&imported), &saved, false).expect("the imported board saves");
    let native: Value =
        serde_json::from_str(&fs::read_to_string(&saved).expect("the document is readable")).expect("the document is JSON");
    let board = &native["document"];
    assert_eq!(board["footprints"][0]["ref"], "R1");
    assert_eq!(board["footprints"][0]["layer"], "F.Cu", "TOP maps to the front copper layer");
    assert_eq!(board["footprints"][0]["x"], 25.4, "inches are scaled to millimetres");
    assert_eq!(board["footprints"][0]["pads"][0]["w"], 1.27);
    assert_eq!(board["footprints"][0]["pads"][1]["w"], 1.27, "a circular pin becomes a square pad");
    assert_eq!(board["footprints"][1]["ref"], "R2");
    assert_eq!(board["footprints"][1]["layer"], "B.Cu", "BOTTOM maps to the back copper layer");
    assert_eq!(board["tracks"][0]["layer"], "F.Cu");
    assert_eq!(board["tracks"][0]["net"], Value::Null, "an undeclared net is not invented");
    assert_eq!(board["title"], "Panel");
    close(handle_of(&imported)).expect("the imported board closes");

    let codes: Vec<&str> =
        imported["lossCodes"].as_array().expect("codes are listed").iter().filter_map(Value::as_str).collect();
    for code in ["ipc2581/outline-not-rectangular", "ipc2581/package-missing", "ipc2581/net-unknown", "ipc2581/hole-not-plated"]
    {
        assert!(codes.contains(&code), "loss codes include {code}; got {codes:?}");
    }
}

#[test]
fn ipc2581_import_rejects_unknown_content() {
    let directory = TempDir::new();
    let path = directory.join("board.txt");
    fs::write(&path, "not a board").expect("the fixture is written");
    let error = import(&path).expect_err("unknown content is rejected");
    assert_eq!(error.code, "format/invalid");
    assert!(error.message.contains("neither a KiCad PCB nor an IPC-2581 document"), "{}", error.message);
}

#[test]
fn gerber_writes_silkscreen_tracks_on_both_sides() {
    let directory = TempDir::new();
    let handle = created();
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "F.SilkS".to_string(), width: 0.15, from: [2.0, 2.0], to: [6.0, 2.0] },
    )
    .expect("the front legend is drawn");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "B.SilkS".to_string(), width: 0.2, from: [3.0, 3.0], to: [7.0, 3.0] },
    )
    .expect("the back legend is drawn");

    let target = directory.join("gerber");
    export(handle, "gbr", &target).expect("Gerber export writes");
    let front = fs::read_to_string(target.join("F_SilkS.gbr")).expect("the front legend is readable");
    assert!(front.contains("%ADD10C,0.150000*%"), "the aperture follows the track width: {front}");
    assert!(front.contains("X2000000Y2000000D02*"), "the legend starts at its first point");
    assert_eq!(front.matches("D01*").count(), 1, "one front legend draw");
    let back = fs::read_to_string(target.join("B_SilkS.gbr")).expect("the back legend is readable");
    assert!(back.contains("%ADD10C,0.200000*%"), "the aperture follows the track width: {back}");
    assert_eq!(back.matches("D01*").count(), 1, "one back legend draw");
    let copper = fs::read_to_string(target.join("F_Cu.gbr")).expect("the copper layer is readable");
    assert_eq!(copper.matches("D01*").count(), 0, "silkscreen geometry does not leak into copper");
    close(handle).expect("the board closes");
}

#[test]
fn differential_pairs_validate_and_persist() {
    let directory = TempDir::new();
    let path = directory.join("pairs.opcb");
    let handle = created();
    add_net(handle, AddNetRequest { name: "D+".to_string() }).expect("the positive net is declared");
    add_net(handle, AddNetRequest { name: "D-".to_string() }).expect("the negative net is declared");

    let pair = add_differential_pair(
        handle,
        AddDifferentialPairRequest {
            positive: "D+".to_string(),
            negative: "D-".to_string(),
            width: 0.2,
            gap: 0.15,
            max_skew: 0.5,
        },
    )
    .expect("the pair is declared");
    assert_eq!(pair["object"]["id"], "dp-1");
    assert_eq!(pair["object"]["maxSkew"], 0.5);
    assert_eq!(pair["board"]["counts"]["differentialPairs"], 1);

    let duplicate = add_differential_pair(
        handle,
        AddDifferentialPairRequest {
            positive: "D+".to_string(),
            negative: "D-".to_string(),
            width: 0.2,
            gap: 0.15,
            max_skew: 0.5,
        },
    )
    .expect_err("a repeated pair is rejected");
    assert_eq!(duplicate.code, "object/duplicate");

    let same = add_differential_pair(
        handle,
        AddDifferentialPairRequest {
            positive: "D+".to_string(),
            negative: "D+".to_string(),
            width: 0.2,
            gap: 0.15,
            max_skew: 0.5,
        },
    )
    .expect_err("a pair needs two nets");
    assert_eq!(same.code, "session/argument-invalid");

    let unknown = add_differential_pair(
        handle,
        AddDifferentialPairRequest {
            positive: "D+".to_string(),
            negative: "GND".to_string(),
            width: 0.2,
            gap: 0.15,
            max_skew: 0.5,
        },
    )
    .expect_err("an undeclared net is rejected");
    assert_eq!(unknown.code, "session/argument-invalid");

    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    assert_eq!(summary(reopened).expect("the board is readable")["counts"]["differentialPairs"], 1);
    let deleted = delete_differential_pair(reopened, "dp-1").expect("the pair is deleted");
    assert_eq!(deleted["deleted"], "dp-1");
    assert_eq!(deleted["board"]["counts"]["differentialPairs"], 0);
    close(reopened).expect("the reopened board closes");
}

#[test]
fn diffpair_check_reports_length_and_spacing_deviation() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "D+".to_string() }).expect("the positive net is declared");
    add_net(handle, AddNetRequest { name: "D-".to_string() }).expect("the negative net is declared");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("D+".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [0.0, 0.0],
            to: [10.0, 0.0],
        },
    )
    .expect("the positive half is routed");
    let negative = add_track(
        handle,
        AddTrackRequest {
            net: Some("D-".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [0.0, 1.0],
            to: [10.0, 1.0],
        },
    )
    .expect("the negative half is routed");
    add_differential_pair(
        handle,
        AddDifferentialPairRequest {
            positive: "D+".to_string(),
            negative: "D-".to_string(),
            width: 0.2,
            gap: 0.1,
            max_skew: 0.1,
        },
    )
    .expect("the pair is declared");

    let clean = inspect(handle, Some(vec!["diffpair".to_string()])).expect("the check runs");
    assert_eq!(codes(&clean), Vec::<&str>::new(), "the matched pair passes");

    let extension = add_track(
        handle,
        AddTrackRequest {
            net: Some("D+".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [10.0, 0.0],
            to: [14.0, 0.0],
        },
    )
    .expect("the positive half is extended");
    let skewed = inspect(handle, Some(vec!["diffpair".to_string()])).expect("the check runs");
    assert_eq!(codes(&skewed), ["drc/diffpair-length-mismatch"]);
    let id = extension["object"]["id"].as_str().expect("the track id is a string");
    delete_track(handle, id).expect("the extra segment is removed");
    let negative_id = negative["object"]["id"].as_str().expect("the negative track id is a string");
    delete_track(handle, negative_id).expect("the original negative segment is removed");

    add_track(
        handle,
        AddTrackRequest {
            net: Some("D-".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [0.0, 0.25],
            to: [10.0, 0.25],
        },
    )
    .expect("the negative half is moved closer");
    let tight = inspect(handle, Some(vec!["diffpair".to_string()])).expect("the check runs");
    assert_eq!(codes(&tight), ["drc/diffpair-spacing"]);
    close(handle).expect("the board closes");
}

#[test]
fn diffpair_check_infers_pairs_from_net_names() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "CLK_P".to_string() }).expect("the positive net is declared");
    add_net(handle, AddNetRequest { name: "CLK_N".to_string() }).expect("the negative net is declared");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("CLK_P".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [0.0, 0.0],
            to: [10.0, 0.0],
        },
    )
    .expect("the positive half is routed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("CLK_N".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [0.0, 1.0],
            to: [10.0, 1.0],
        },
    )
    .expect("the negative half is routed");

    let matched = inspect(handle, Some(vec!["diffpair".to_string()])).expect("the check runs");
    assert_eq!(codes(&matched), Vec::<&str>::new(), "equal halves pass without a declared pair");

    add_track(
        handle,
        AddTrackRequest {
            net: Some("CLK_P".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [10.0, 0.0],
            to: [12.0, 0.0],
        },
    )
    .expect("the positive half is extended");
    let skewed = inspect(handle, Some(vec!["diffpair".to_string()])).expect("the check runs");
    assert_eq!(codes(&skewed), ["drc/diffpair-skew"]);
    let issue = &skewed["issues"][0];
    assert_eq!(issue["severity"], "warning");
    let message = issue["message"].as_str().expect("the finding has a message");
    assert!(message.contains("CLK"), "the message names the base pair: {message}");
    assert!(message.contains("12.000") && message.contains("10.000"), "the message carries both lengths: {message}");
    assert!(message.contains("2.000"), "the message carries the skew: {message}");
    assert_eq!(issue["objectId"], "CLK", "the finding names the base pair");
    close(handle).expect("the board closes");

    let unpaired_handle = created();
    for (net, x) in [("X_P", 0.0), ("Y_P", 2.0)] {
        add_net(unpaired_handle, AddNetRequest { name: net.to_string() }).expect("the lone net is declared");
        add_track(
            unpaired_handle,
            AddTrackRequest { net: Some(net.to_string()), layer: "F.Cu".to_string(), width: 0.2, from: [0.0, x], to: [5.0, x] },
        )
        .expect("the lone net is routed");
    }
    let unpaired = inspect(unpaired_handle, Some(vec!["diffpair".to_string()])).expect("the check runs");
    assert_eq!(codes(&unpaired), ["drc/diffpair-unpaired", "drc/diffpair-unpaired"]);
    let messages: Vec<&str> = unpaired["issues"]
        .as_array()
        .expect("issues is an array")
        .iter()
        .filter_map(|issue| issue["message"].as_str())
        .collect();
    assert!(messages.iter().any(|message| message.contains("X_P")), "the message names the unpaired net: {messages:?}");
    assert!(messages.iter().any(|message| message.contains("Y_P")), "one finding per unpaired net: {messages:?}");
    close(unpaired_handle).expect("the board closes");

    let plus_handle = created();
    add_net(plus_handle, AddNetRequest { name: "RX+".to_string() }).expect("the positive net is declared");
    add_net(plus_handle, AddNetRequest { name: "RX-".to_string() }).expect("the negative net is declared");
    for (net, y) in [("RX+", 0.0), ("RX-", 1.0)] {
        add_track(
            plus_handle,
            AddTrackRequest { net: Some(net.to_string()), layer: "F.Cu".to_string(), width: 0.2, from: [0.0, y], to: [8.0, y] },
        )
        .expect("the suffixed net is routed");
    }
    let plus = inspect(plus_handle, Some(vec!["diffpair".to_string()])).expect("the check runs");
    assert_eq!(codes(&plus), Vec::<&str>::new(), "`+` and `-` suffixes pair too");
    close(plus_handle).expect("the board closes");

    let plain_handle = created();
    add_net(plain_handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    add_track(
        plain_handle,
        AddTrackRequest {
            net: Some("GND".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [0.0, 0.0],
            to: [5.0, 0.0],
        },
    )
    .expect("the net is routed");
    let plain = inspect(plain_handle, Some(vec!["diffpair".to_string()])).expect("the check runs");
    assert_eq!(codes(&plain), Vec::<&str>::new(), "no differential networks means no diagnostics");
    close(plain_handle).expect("the board closes");
}

#[test]
fn dangling_copper_reports_loose_track_ends_only() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "SIG".to_string() }).expect("the net is declared");
    add_via(
        handle,
        AddViaRequest { net: Some("SIG".to_string()), at: [0.0, 0.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the start via is placed");
    add_via(
        handle,
        AddViaRequest { net: Some("SIG".to_string()), at: [20.0, 0.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the end via is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("SIG".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 0.0],
            to: [10.0, 0.0],
        },
    )
    .expect("the first segment is routed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("SIG".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [10.0, 0.0],
            to: [20.0, 0.0],
        },
    )
    .expect("the second segment is routed");
    let loose = add_track(
        handle,
        AddTrackRequest {
            net: Some("SIG".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [5.0, 5.0],
            to: [15.0, 5.0],
        },
    )
    .expect("the isolated segment is routed");

    let report = inspect(handle, Some(vec!["dangling-copper".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/dangling-copper"]);
    assert_eq!(report["issues"][0]["objectId"], loose["object"]["id"]);
    close(handle).expect("the board closes");
}

#[test]
fn back_drill_is_validated_exported_and_round_tripped() {
    let directory = TempDir::new();
    let handle = created();
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("the net is declared");
    add_via(
        handle,
        AddViaRequest { net: Some("GND".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the through via is placed");
    add_layer(handle, AddLayerRequest { name: "In1.Cu".to_string(), kind: Some("copper".to_string()), color: None })
        .expect("the inner layer is declared");
    add_via(
        handle,
        AddViaRequest {
            net: Some("GND".to_string()),
            at: [20.0, 10.0],
            diameter: 0.6,
            drill: 0.3,
            from_layer: Some("F.Cu".to_string()),
            to_layer: Some("In1.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the blind via is placed");

    let zero =
        set_via_back_drill(handle, SetViaBackDrillRequest { id: "via-1".to_string(), depth: Some(0.0), stub: Some(0.1) })
            .expect_err("a zero depth is rejected");
    assert_eq!(zero.code, "session/argument-invalid");

    let set = set_via_back_drill(handle, SetViaBackDrillRequest { id: "via-1".to_string(), depth: Some(0.8), stub: Some(0.2) })
        .expect("the through via is back drilled");
    assert_eq!(set["object"]["backDrill"]["depth"], 0.8);
    assert_eq!(set["object"]["backDrill"]["stub"], 0.2);

    let blind =
        set_via_back_drill(handle, SetViaBackDrillRequest { id: "via-2".to_string(), depth: Some(0.5), stub: Some(0.1) })
            .expect_err("a blind via cannot be back drilled");
    assert_eq!(blind.code, "session/argument-invalid");

    let drill = directory.join("back.drl");
    export(handle, "drl", &drill).expect("the drill file writes");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    assert!(text.contains(";Back drill depth=0.800 stub=0.200"), "{text}");

    let ipc = directory.join("back.ipc2581");
    export(handle, "ipc2581", &ipc).expect("IPC-2581 writes");
    let xml = fs::read_to_string(&ipc).expect("the IPC-2581 document is readable");
    assert!(xml.contains("backDrillDepth=\"0.8\""), "{xml}");
    assert!(xml.contains("backDrillStub=\"0.2\""), "{xml}");

    let reopened = handle_of(&import(&ipc).expect("the IPC-2581 document imports"));
    let back = store::read(reopened, |board| {
        board.vias.iter().find(|via| via.id == "via-1").and_then(|via| via.back_drill).map(|back| (back.depth, back.stub))
    })
    .expect("the board is readable");
    assert_eq!(back, Some((0.8, 0.2)));
    close(reopened).expect("the reopened board closes");

    let path = directory.join("back.opcb");
    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let native = handle_of(&open(&path).expect("the native document reopens"));
    let back = store::read(native, |board| {
        board.vias.iter().find(|via| via.id == "via-1").and_then(|via| via.back_drill).map(|back| (back.depth, back.stub))
    })
    .expect("the reopened board is readable");
    assert_eq!(back, Some((0.8, 0.2)));
    close(native).expect("the native board closes");
}

#[test]
fn linear_teardrops_materialise_without_drc_false_positives() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "SIG".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("J1".to_string()),
            x: Some(10.0),
            y: Some(10.0),
            layer: Some("F.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the first footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("SIG".to_string()),
            drill: None,
        },
    )
    .expect("the first pad is placed");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("J2".to_string()),
            x: Some(30.0),
            y: Some(10.0),
            layer: Some("F.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the second footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-2".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("SIG".to_string()),
            drill: None,
        },
    )
    .expect("the second pad is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("SIG".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.3,
            from: [10.0, 10.0],
            to: [30.0, 10.0],
        },
    )
    .expect("the pads are routed");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [40.0, 20.0] }).expect("the outline is set");

    let first = add_teardrops(handle, AddTeardropsRequest::default()).expect("teardrops are generated");
    assert_eq!(first["object"]["tracks"], 4);
    assert_eq!(first["object"]["skipped"], 0);
    assert_eq!(first["board"]["counts"]["tracks"], 5);

    let second = add_teardrops(handle, AddTeardropsRequest::default()).expect("a repeat run is harmless");
    assert_eq!(second["object"]["tracks"], 0);
    assert_eq!(second["object"]["skipped"], 4);

    let report = inspect(
        handle,
        Some(vec!["drc".to_string(), "clearance".to_string(), "connectivity".to_string(), "dangling-copper".to_string()]),
    )
    .expect("the checks run");
    assert!(report["passed"].as_bool().expect("passed is a boolean"), "{report}");
    assert_eq!(codes(&report), Vec::<&str>::new());

    let arc = add_teardrops(handle, AddTeardropsRequest { shape: Some("arc".to_string()), length: None })
        .expect_err("the arc shape stays unimplemented");
    assert_eq!(arc.code, "capability/not-implemented");

    let directory = TempDir::new();
    let path = directory.join("teardrops.opcb");
    save(handle, &path, false).expect("the board saves");
    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    let third = add_teardrops(reopened, AddTeardropsRequest::default()).expect("the saved teardrops are recognised");
    assert_eq!(third["object"]["tracks"], 0);
    assert_eq!(third["object"]["skipped"], 4);
    close(reopened).expect("the reopened board closes");
}

#[test]
fn net_lengths_sum_track_segments_and_count_vias() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("VCC is declared");
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("GND is declared");
    add_net(handle, AddNetRequest { name: "SPARE".to_string() }).expect("SPARE is declared");

    // A 3-4-5 right triangle followed by a 2 mm continuation: 5 + 2 = 7 mm.
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 0.0],
            to: [3.0, 4.0],
        },
    )
    .expect("the 3-4-5 segment is routed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [3.0, 4.0],
            to: [5.0, 4.0],
        },
    )
    .expect("the second segment is routed");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [3.0, 4.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    add_track(
        handle,
        AddTrackRequest { net: None, layer: "F.Cu".to_string(), width: 0.25, from: [0.0, 10.0], to: [10.0, 10.0] },
    )
    .expect("copper without a net is routed");

    let report = net_lengths(handle).expect("the report is produced");
    assert_eq!(report["report"], "open-pcb-net-lengths");
    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets.len(), 3, "only declared nets are listed");
    assert_eq!(nets[0]["name"], "VCC");
    assert_eq!(nets[0]["tracks"], 2);
    assert_eq!(nets[0]["length"], 7.0);
    assert_eq!(nets[0]["vias"], 1);
    assert_eq!(nets[1]["name"], "GND");
    assert_eq!(nets[1]["tracks"], 0);
    assert_eq!(nets[1]["length"], 0.0);
    assert_eq!(nets[1]["vias"], 0);
    assert_eq!(nets[2]["name"], "SPARE");
    assert_eq!(nets[2]["length"], 0.0);
    let ranking = report["ranking"].as_array().expect("ranking is an array");
    assert_eq!(ranking.len(), 3);
    assert_eq!(ranking[0]["name"], "VCC", "the routed net ranks first");
    assert_eq!(ranking[0]["length"], 7.0);
    assert_eq!(ranking[1]["name"], "GND", "equal zero lengths fall back to the net name");
    assert_eq!(ranking[2]["name"], "SPARE");
    assert_eq!(report["totals"]["tracks"], 2);
    assert_eq!(report["totals"]["length"], 7.0);
    assert_eq!(report["totals"]["vias"], 1);
    close(handle).expect("the board closes");
}

#[test]
fn net_lengths_rank_nets_longest_first_read_only_and_deterministically() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "A".to_string() }).expect("A is declared");
    add_net(handle, AddNetRequest { name: "B".to_string() }).expect("B is declared");
    add_net(handle, AddNetRequest { name: "C".to_string() }).expect("C is declared");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("A".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 0.0],
            to: [3.0, 0.0],
        },
    )
    .expect("A is routed by 3 mm");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("B".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 1.0],
            to: [10.0, 1.0],
        },
    )
    .expect("B is routed by 10 mm");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("C".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [0.0, 2.0],
            to: [3.0, 2.0],
        },
    )
    .expect("C is routed by 3 mm");

    let first = net_lengths(handle).expect("the report is produced");
    let ranking = first["ranking"].as_array().expect("ranking is an array");
    assert_eq!(ranking.len(), 3, "every declared net is ranked");
    assert_eq!(ranking[0]["name"], "B", "the longest net ranks first");
    assert_eq!(ranking[0]["length"], 10.0);
    assert_eq!(ranking[1]["name"], "A", "equal lengths fall back to the net name");
    assert_eq!(ranking[1]["length"], 3.0);
    assert_eq!(ranking[2]["name"], "C");
    assert_eq!(ranking[2]["length"], 3.0);
    assert_eq!(first["nets"][0]["name"], "A", "the nets array keeps declaration order");

    let second = net_lengths(handle).expect("the report is produced again");
    assert_eq!(
        serde_json::to_string(&first).expect("the first report serializes"),
        serde_json::to_string(&second).expect("the second report serializes"),
        "repeated reports are byte-identical"
    );

    let revision = summary(handle).expect("the board is listed")["revision"].clone();
    assert_eq!(first["revision"], revision, "the report leaves the revision alone");
    assert_eq!(summary(handle).expect("the board is still listed")["revision"], revision, "a read never bumps the revision");

    close(handle).expect("the board closes");
    let missing = net_lengths(handle).expect_err("a closed handle is rejected");
    assert_eq!(missing.code, "board/not-found");
}

#[test]
fn pad_drills_validate_and_survive_save_and_reopen() {
    let directory = TempDir::new();
    let matrix = capabilities();
    assert_eq!(matrix["padDrills"], "supported", "pad drills are a claimed capability");

    let handle = created();
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("J1".to_string()),
            x: Some(10.0),
            y: Some(20.0),
            layer: Some("F.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the footprint is placed");
    let added = add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 0.8,
            net: None,
            drill: Some(0.4),
        },
    )
    .expect("the drilled pad is added");
    assert_eq!(added["object"]["drill"], 0.4);

    for drill in [0.8, 0.0, -0.2, f64::INFINITY] {
        let refused = add_pad(
            handle,
            AddPadRequest {
                footprint: "fp-1".to_string(),
                number: "2".to_string(),
                x: 2.0,
                y: 0.0,
                w: 1.0,
                h: 0.8,
                net: None,
                drill: Some(drill),
            },
        )
        .expect_err("an invalid drill is rejected");
        assert_eq!(refused.code, "pad/invalid-drill", "drill {drill} is rejected");
    }

    let updated = update_pad(
        handle,
        UpdatePadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), drill: Some(0.5), ..Default::default() },
    )
    .expect("the drill is updated");
    assert_eq!(updated["object"]["drill"], 0.5);

    let too_large = update_pad(
        handle,
        UpdatePadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), drill: Some(0.9), ..Default::default() },
    )
    .expect_err("a drill must be smaller than the short side");
    assert_eq!(too_large.code, "pad/invalid-drill");
    let thin = update_pad(
        handle,
        UpdatePadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), w: Some(0.3), ..Default::default() },
    )
    .expect_err("shrinking the pad below the stored drill is rejected");
    assert_eq!(thin.code, "pad/invalid-drill");

    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "2".to_string(),
            x: 2.0,
            y: 0.0,
            w: 0.5,
            h: 0.5,
            net: None,
            drill: None,
        },
    )
    .expect("a pad without a drill is added");

    let path = directory.join("drills.opcb");
    save(handle, &path, false).expect("the board saves");
    let saved: Value =
        serde_json::from_str(&fs::read_to_string(&path).expect("the document is readable")).expect("the document is JSON");
    let pads = saved["document"]["footprints"][0]["pads"].as_array().expect("pads is an array");
    assert_eq!(pads[0]["drill"], 0.5, "the stored drill is written");
    assert!(pads[1].get("drill").is_none(), "a pad without a drill keeps the old document shape");

    close(handle).expect("the board closes");
    let reopened = handle_of(&open(&path).expect("the document reopens"));
    let drill = directory.join("drills.drl");
    export(reopened, "drl", &drill).expect("the drill file writes");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    assert!(text.contains("T1C0.500"), "the reopened pad keeps its drill; got {text}");
    assert!(text.contains("X10.000Y20.000"), "the pad hole uses absolute coordinates; got {text}");
    close(reopened).expect("the reopened board closes");
}

#[test]
fn kicad_import_keeps_circular_pad_drills_and_reports_other_drills() {
    let directory = TempDir::new();
    let path = directory.join("drills.kicad_pcb");
    let text = "(kicad_pcb (version 20221018) (generator open-pcb)\n\
        (layers (0 \"F.Cu\" signal) (31 \"B.Cu\" signal))\n\
        (net 0 \"\")\n\
        (net 1 \"VCC\")\n\
        (footprint \"THT\" (layer \"F.Cu\") (at 10 20)\n\
            (property \"Reference\" \"J1\" (layer \"F.SilkS\"))\n\
            (property \"Value\" \"Conn\" (layer \"F.Fab\"))\n\
            (pad \"1\" thru_hole circle (at 0 0) (size 1.6 1.6) (drill 0.8) (layers \"*.Cu\" \"*.Mask\") (net 1 \"VCC\")))\n\
        (footprint \"OVAL\" (layer \"F.Cu\") (at 30 20)\n\
            (property \"Reference\" \"J2\" (layer \"F.SilkS\"))\n\
            (pad \"1\" thru_hole oval (at 0 0) (size 2 1) (drill oval 0.5 1.0) (layers \"*.Cu\" \"*.Mask\"))))\n";
    fs::write(&path, text).expect("the fixture is written");
    let imported = import(&path).expect("the board imports");
    let codes: Vec<&str> =
        imported["lossCodes"].as_array().expect("codes are listed").iter().filter_map(Value::as_str).collect();
    assert!(codes.contains(&"kicad/pad-drill"), "an oval drill stays in the ledger; got {codes:?}");

    let handle = handle_of(&imported);
    let drills = store::read(handle, |board| {
        board.footprints.iter().flat_map(|footprint| footprint.pads.iter().map(|pad| pad.drill)).collect::<Vec<_>>()
    })
    .expect("the imported board is readable");
    assert_eq!(drills, [Some(0.8), None], "only the circular drill is stored");

    let target = directory.join("drills-out.kicad_pcb");
    export(handle, "kicad_pcb", &target).expect("KiCad export writes");
    let exported = fs::read_to_string(&target).expect("the exported board is readable");
    assert!(exported.contains("(drill 0.8)"), "the circular drill is written back; got {exported}");
    let again = import(&target).expect("the exported board imports");
    let again_codes: Vec<&str> =
        again["lossCodes"].as_array().expect("codes are listed").iter().filter_map(Value::as_str).collect();
    assert!(!again_codes.contains(&"kicad/pad-drill"), "a round trip keeps the drill; got {again_codes:?}");
    close(handle).expect("the imported board closes");
    close(handle_of(&again)).expect("the reimported board closes");
}

#[test]
fn excellon_and_drill_map_include_pad_holes() {
    let directory = TempDir::new();
    let handle = created();
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("J1".to_string()),
            x: Some(10.0),
            y: Some(20.0),
            layer: Some("F.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: None,
            drill: Some(0.4),
        },
    )
    .expect("the drilled pad is added");
    add_via(handle, AddViaRequest { at: [30.0, 40.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the via is placed");

    let drill = directory.join("holes.drl");
    export(handle, "drl", &drill).expect("the drill file writes");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    assert!(text.contains("T1C0.300"), "the via tool comes first; got {text}");
    assert!(text.contains("T2C0.400"), "the pad drill is its own tool; got {text}");
    assert!(text.contains("T1\nX30.000Y40.000"), "the via hit is written; got {text}");
    assert!(text.contains("T2\nX10.000Y20.000"), "the pad hit is written; got {text}");

    let map = directory.join("holes.drill-map.svg");
    export(handle, "drill_map", &map).expect("the drill map writes");
    let map = fs::read_to_string(&map).expect("the drill map is readable");
    assert_eq!(map.matches("<circle").count(), 2, "the via and the pad each draw one hole");
    assert!(map.contains("T1 dia 0.300 mm plated x1"), "the via tool is in the legend; got {map}");
    assert!(map.contains("T2 dia 0.400 mm plated x1"), "the pad tool is in the legend; got {map}");
    close(handle).expect("the board closes");
}

#[test]
fn drill_report_counts_pad_holes_additively() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("J1".to_string()),
            x: Some(10.0),
            y: Some(20.0),
            layer: Some("F.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("VCC".to_string()),
            drill: Some(0.3),
        },
    )
    .expect("the drilled pad is added");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [40.0, 20.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");

    let report = drill_report(handle).expect("the drill report is built");
    assert_eq!(report["holeCount"], 1, "the historical hole count still counts vias only");
    assert_eq!(report["padHoleCount"], 1);
    let tools = report["tools"].as_array().expect("tools is an array");
    assert_eq!(tools.len(), 1, "a shared diameter is one tool");
    assert_eq!(tools[0]["holes"], 1);
    assert_eq!(tools[0]["slots"], 0);
    assert_eq!(tools[0]["padHoles"], 1);
    let pads = report["padHoles"].as_array().expect("padHoles is an array");
    assert_eq!(pads.len(), 1);
    assert_eq!(pads[0]["id"], "fp-1:1");
    assert_eq!(pads[0]["net"], "VCC");
    assert_eq!(pads[0]["tool"], 1);
    assert_eq!(pads[0]["at"], json!([10.0, 20.0]));
    assert_eq!(pads[0]["diameter"], 0.3);
    assert_eq!(pads[0]["plated"], true);
    close(handle).expect("the board closes");
}

#[test]
fn annular_ring_checks_drilled_pads_with_the_via_rule() {
    let handle = created();
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("J1".to_string()),
            x: Some(10.0),
            y: Some(20.0),
            layer: Some("F.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 0.4,
            h: 0.4,
            net: None,
            drill: Some(0.35),
        },
    )
    .expect("the thin-ring pad is added");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "2".to_string(),
            x: 2.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: None,
            drill: Some(0.4),
        },
    )
    .expect("the healthy pad is added");

    let report = inspect(handle, Some(vec!["annular-ring".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), ["drc/annular-ring-too-small"]);
    assert_eq!(report["issues"][0]["objectId"], "fp-1");
    assert_eq!(report["issues"][0]["layerId"], "F.Cu");
    assert!(report["issues"][0]["message"].as_str().is_some_and(|message| message.contains("pad `1`")));
    assert_eq!(report["passed"], false);
    close(handle).expect("the board closes");
}

#[test]
fn pads_without_drills_add_no_fabrication_output() {
    let directory = TempDir::new();
    let handle = created();
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("J1".to_string()),
            x: Some(10.0),
            y: Some(20.0),
            layer: Some("F.Cu".to_string()),
            ..Default::default()
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: None,
            drill: None,
        },
    )
    .expect("the plain pad is added");
    add_via(handle, AddViaRequest { at: [30.0, 40.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the via is placed");

    let drill = directory.join("plain.drl");
    export(handle, "drl", &drill).expect("the drill file writes");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    assert_eq!(
        text, "M48\n;OpenPCB drill file\n;Units: mm\nMETRIC,TZ\nT1C0.300\n%\nG90\nG05\nT1\nX30.000Y40.000\nM30\n",
        "a pad without a drill leaves the drill file byte-for-byte unchanged"
    );

    let map = directory.join("plain.drill-map.svg");
    export(handle, "drill_map", &map).expect("the drill map writes");
    let map = fs::read_to_string(&map).expect("the drill map is readable");
    assert_eq!(map.matches("<circle").count(), 1, "only the via draws a hole");
    assert!(!map.contains("X10.000"), "the pad contributes no hit");

    let kicad = directory.join("plain.kicad_pcb");
    export(handle, "kicad_pcb", &kicad).expect("KiCad export writes");
    let kicad = fs::read_to_string(&kicad).expect("the KiCad board is readable");
    assert!(
        kicad.contains("(pad \"1\" smd rect (at 0 0) (size 1 1) (layers \"F.Cu\" \"F.Paste\" \"F.Mask\"))"),
        "the pad block keeps its historical shape; got {kicad}"
    );
    assert!(!kicad.contains("(pad \"1\" smd rect (at 0 0) (size 1 1) (drill"), "a pad without a drill writes no drill token");
    assert_eq!(kicad.matches("(drill").count(), 1, "only the via declares a drill");

    let report = drill_report(handle).expect("the drill report is built");
    assert_eq!(report["padHoleCount"], 0);
    assert_eq!(report["padHoles"], json!([]));
    close(handle).expect("the board closes");
}

#[test]
fn svg_preview_draws_footprints_and_keeps_board_y_down() {
    let directory = TempDir::new();
    let handle = created();
    set_outline(handle, SetOutlineRequest { from: [2.0, 2.0], to: [60.0, 40.0] }).expect("the outline is set");
    for (reference, y) in [("A1", 10.0), ("A2", 30.0)] {
        let footprint = add_footprint(
            handle,
            AddFootprintRequest {
                reference: Some(reference.to_string()),
                value: Some("X".to_string()),
                footprint: Some("0603".to_string()),
                x: Some(20.0),
                y: Some(y),
                rotation: Some(0.0),
                layer: None,
            },
        )
        .expect("the footprint is placed");
        let id = footprint["object"]["id"]
            .as_str()
            .or_else(|| footprint["id"].as_str())
            .expect("the footprint id is present")
            .to_string();
        for (index, offset) in [-1.0, 1.0].into_iter().enumerate() {
            add_pad(
                handle,
                AddPadRequest {
                    footprint: id.clone(),
                    number: (index + 1).to_string(),
                    x: offset,
                    y: 0.0,
                    w: 1.2,
                    h: 1.4,
                    net: None,
                    drill: None,
                },
            )
            .expect("the pad is added");
        }
    }

    let path = directory.join("preview.svg");
    export(handle, "svg", &path).expect("the SVG preview writes");
    let svg = fs::read_to_string(&path).expect("the SVG is readable");
    assert!(svg.contains(">A1</text>"), "the reference designator is drawn; got {svg}");
    assert!(svg.contains(">A2</text>"), "the second reference is drawn; got {svg}");
    assert!(svg.contains("stroke=\"#e6e6e6\""), "a silkscreen outline is drawn; got {svg}");
    assert!(!svg.contains("fill=\"#c87533\" stroke=\"#000000\""), "pads are solid copper instead of black-ringed; got {svg}");

    let y_of = |reference: &str| -> f64 {
        let marker = format!(">{reference}</text>");
        let end = svg.find(&marker).expect("the reference text exists");
        let start = svg[..end].rfind("<text").expect("the text start exists");
        let value = svg[start..end].split("y=\"").nth(1).expect("a y attribute exists");
        value.split('"').next().expect("the y value exists").parse().expect("y is a number")
    };
    assert!(y_of("A1") < y_of("A2"), "a smaller board Y must stay higher in the preview");
    close(handle).expect("the board closes");
}

#[test]
fn svg_export_carries_the_title_layer_legend_and_board_size() {
    let directory = TempDir::new();
    let handle = created();
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [80.0, 50.0] }).expect("the outline is set");
    let path = directory.join("title.svg");
    export(handle, "svg", &path).expect("the SVG export writes");
    let svg = fs::read_to_string(&path).expect("the SVG is readable");
    assert!(svg.contains(">Blinker</text>"), "the title is drawn above the board");
    for marker in ["Front copper", "Back copper", "Silkscreen", "Drill", "Edge cuts", "Board: 80 x 50 mm"] {
        assert!(svg.contains(marker), "the legend carries {marker}");
    }
    assert!(svg.contains("class=\"title-block\""), "the title block is grouped");
    close(handle).expect("the board closes");
}

#[test]
fn fab_manifest_mirrors_the_gerber_files_and_a_clean_board_passes() {
    let directory = TempDir::new();
    let handle = created();
    add_full_board(handle);

    let report = fab_manifest(handle).expect("the manifest is built");
    assert_eq!(report["report"], "open-pcb-fab-manifest");
    assert_eq!(report["units"], "mm");
    assert_eq!(report["outline"], true);
    assert_eq!(report["status"], "pass", "a routable board has no manifest issue: {}", report["issues"]);
    assert_eq!(report["copperLayers"], json!(["F.Cu", "B.Cu"]));
    assert_eq!(report["fileCount"], 9);

    let files = report["files"].as_array().expect("files is an array");
    assert_eq!(files[0]["name"], "F_Cu.gbr");
    assert_eq!(files[0]["function"], "Copper,L1,Top");
    assert_eq!(files[0]["kind"], "copper");
    assert_eq!(files[0]["layer"], "F.Cu");
    assert_eq!(files[0]["objects"], 2, "one track and one through via sit on F.Cu");
    assert_eq!(files[1]["function"], "Copper,L2,Bot");
    assert_eq!(files[1]["objects"], 1, "the through via reaches B.Cu as well");
    assert_eq!(files[4]["name"], "F_Mask.gbr");
    assert_eq!(files[4]["function"], "Soldermask,Top");
    assert_eq!(files[6]["function"], "Paste,Top");
    assert_eq!(files[8]["name"], "Edge_Cuts.gbr");
    assert_eq!(files[8]["function"], "Profile,NP");
    assert_eq!(files[8]["objects"], 4);

    assert_eq!(report["drill"]["tools"], 1);
    assert_eq!(report["drill"]["holes"], 1);
    assert_eq!(report["drill"]["slots"], 0);
    assert_eq!(report["drill"]["padHoles"], 0);
    assert_eq!(report["assembly"]["placements"], 1);
    assert_eq!(report["assembly"]["bomGroups"], 1);

    let gbr = directory.join("gerber");
    let exported = export(handle, "gbr", &gbr).expect("Gerber export writes");
    let artifacts = exported["artifacts"].as_array().expect("artifacts is an array");
    let exported_names: Vec<&str> = artifacts.iter().map(|item| item["name"].as_str().expect("a name")).collect();
    let planned_names: Vec<&str> = files.iter().map(|file| file["name"].as_str().expect("a name")).collect();
    assert_eq!(planned_names, exported_names, "the manifest lists the files the writer emits, in order");
    for file in files {
        let name = file["name"].as_str().expect("a name");
        let function = file["function"].as_str().expect("a function");
        let text = fs::read_to_string(gbr.join(name)).expect("the Gerber file is readable");
        assert!(text.contains(&format!("%TF.FileFunction,{function}*%")), "{name} declares {function}; got {text}");
    }
    close(handle).expect("the board closes");
}

#[test]
fn fab_manifest_reports_a_missing_outline_and_a_file_name_collision() {
    let handle = created();
    add_layer(handle, AddLayerRequest { name: "F_Mask".to_string(), kind: None, color: None })
        .expect("a copper layer named after the mask file is added");

    let report = fab_manifest(handle).expect("the manifest is built");
    assert_eq!(report["status"], "error");
    let codes: Vec<&str> =
        report["issues"].as_array().expect("issues is an array").iter().filter_map(|issue| issue["code"].as_str()).collect();
    assert!(codes.contains(&"fab/no-outline"), "a missing outline stops Gerber export; got {codes:?}");
    assert!(codes.contains(&"fab/file-name-collision"), "two files plan one name; got {codes:?}");
    assert!(codes.contains(&"fab/no-drill-data"), "a board without drills warns; got {codes:?}");
    assert!(codes.contains(&"fab/no-placements"), "a board without footprints warns; got {codes:?}");
    assert_eq!(report["fileCount"], 9, "the extra copper layer adds one planned file");
    let files = report["files"].as_array().expect("files is an array");
    assert_eq!(files.iter().filter(|file| file["name"] == "F_Mask.gbr").count(), 2);
    assert_eq!(files.iter().filter(|file| file["kind"] == "profile").count(), 0, "no outline plans no profile");

    close(handle).expect("the board closes");
    let missing = fab_manifest(handle).expect_err("a closed handle is rejected");
    assert_eq!(missing.code, "board/not-found");
}

#[test]
fn zone_audit_separates_connected_isolated_and_unassigned_pours() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "GND".to_string() }).expect("GND is declared");
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("VCC is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN".to_string()),
            x: Some(5.0),
            y: Some(5.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
            net: Some("GND".to_string()),
            drill: None,
        },
    )
    .expect("the ground pad is added");
    add_zone(
        handle,
        AddZoneRequest {
            layer: "F.Cu".to_string(),
            net: Some("GND".to_string()),
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]],
        },
    )
    .expect("the connected pour is added");
    add_zone(
        handle,
        AddZoneRequest {
            layer: "F.Cu".to_string(),
            net: Some("VCC".to_string()),
            points: vec![[20.0, 20.0], [30.0, 20.0], [30.0, 30.0], [20.0, 30.0]],
        },
    )
    .expect("the isolated pour is added");
    add_zone(
        handle,
        AddZoneRequest {
            layer: "F.Cu".to_string(),
            net: None,
            points: vec![[40.0, 40.0], [50.0, 40.0], [50.0, 50.0], [40.0, 50.0]],
        },
    )
    .expect("the unassigned pour is added");

    let report = zone_audit(handle).expect("the zone audit is built");
    assert_eq!(report["report"], "open-pcb-zone-audit");
    assert_eq!(report["units"], "mm2");
    assert_eq!(report["zoneCount"], 3);
    assert_eq!(report["connectedCount"], 1);
    assert_eq!(report["isolatedCount"], 1);
    assert_eq!(report["unassignedCount"], 1);
    assert_eq!(report["totalArea"], 300.0);
    assert_eq!(report["isolatedArea"], 100.0);
    assert_eq!(report["status"], "warn");
    let codes: Vec<&str> =
        report["issues"].as_array().expect("issues is an array").iter().filter_map(|issue| issue["code"].as_str()).collect();
    assert_eq!(codes, ["zone/isolated", "zone/unassigned-net"]);

    let zones = report["zones"].as_array().expect("zones is an array");
    assert_eq!(zones[0]["id"], "zone-1");
    assert_eq!(zones[0]["status"], "connected");
    assert_eq!(zones[0]["anchors"], json!({ "pads": 1, "vias": 0, "tracks": 0 }));
    assert_eq!(zones[0]["area"], 100.0);
    assert_eq!(zones[0]["perimeter"], 40.0);
    assert_eq!(zones[1]["status"], "isolated");
    assert_eq!(zones[1]["anchors"], json!({ "pads": 0, "vias": 0, "tracks": 0 }));
    assert_eq!(zones[2]["status"], "unassigned");
    assert_eq!(report["layers"][0]["layer"], "F.Cu");
    assert_eq!(report["layers"][0]["zones"], 3);
    assert_eq!(report["layers"][0]["connected"], 1);
    assert_eq!(report["layers"][0]["isolated"], 1);
    assert_eq!(report["layers"][0]["unassigned"], 1);
    assert_eq!(report["layers"][0]["area"], 300.0);
    assert_eq!(report["layers"][0]["isolatedArea"], 100.0);

    add_track(
        handle,
        AddTrackRequest {
            net: Some("GND".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.25,
            from: [5.0, 5.0],
            to: [15.0, 5.0],
        },
    )
    .expect("a track leaves the pour");
    let report = zone_audit(handle).expect("the zone audit is rebuilt");
    assert_eq!(report["zones"][0]["anchors"], json!({ "pads": 1, "vias": 0, "tracks": 1 }));
    let revision = summary(handle).expect("the board is listed")["revision"].as_u64().expect("a revision");
    let again = zone_audit(handle).expect("the audit is deterministic");
    assert_eq!(again["zones"], report["zones"]);
    assert_eq!(summary(handle).expect("the board is listed")["revision"].as_u64(), Some(revision), "the audit edits nothing");

    close(handle).expect("the board closes");
    let missing = zone_audit(handle).expect_err("a closed handle is rejected");
    assert_eq!(missing.code, "board/not-found");
}

#[test]
fn net_audit_reports_class_targets_widths_and_clearance_pairs() {
    let handle = created();
    for name in ["N1", "N2", "N3"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_net_class(
        handle,
        AddNetClassRequest {
            name: "Power".to_string(),
            width: Some(0.5),
            clearance: Some(0.5),
            via_diameter: Some(0.8),
            via_drill: Some(0.4),
        },
    )
    .expect("the class is declared");
    for net in ["N1", "N2"] {
        assign_net_class(handle, AssignNetClassRequest { net: net.to_string(), class: "Power".to_string() })
            .expect("the net joins the class");
    }
    add_track(
        handle,
        AddTrackRequest {
            net: Some("N1".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [0.0, 0.0],
            to: [10.0, 0.0],
        },
    )
    .expect("the narrow track is placed");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("N2".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.3,
            from: [0.0, 0.5],
            to: [10.0, 0.5],
        },
    )
    .expect("the close track is placed");

    let report = net_audit(handle).expect("the net audit is built");
    assert_eq!(report["report"], "open-pcb-net-audit");
    assert_eq!(report["units"], "mm");
    assert_eq!(report["status"], "error");
    assert_eq!(report["netCount"], 3);
    assert_eq!(report["classCount"], 1);
    assert_eq!(report["pairCount"], 1);
    let codes: Vec<&str> =
        report["issues"].as_array().expect("issues is an array").iter().filter_map(|issue| issue["code"].as_str()).collect();
    assert_eq!(
        codes,
        ["net/clearance-below-class", "net/width-below-class", "net/width-below-class"],
        "both class nets route below the target width"
    );

    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets[0]["name"], "N1");
    assert_eq!(nets[0]["class"], "Power");
    assert_eq!(nets[0]["targets"]["width"], 0.5);
    assert_eq!(nets[0]["targets"]["clearance"], 0.5);
    assert_eq!(nets[0]["targets"]["viaDiameter"], 0.8);
    assert_eq!(nets[0]["targets"]["viaDrill"], 0.4);
    assert_eq!(nets[0]["tracks"], 1);
    assert_eq!(nets[0]["width"], json!({ "min": 0.2, "max": 0.2 }));
    assert_eq!(nets[0]["widthStatus"], "below");
    assert_eq!(nets[0]["clearance"], json!({ "to": "N2", "actual": 0.25, "required": 0.5, "ok": false }));
    assert_eq!(nets[2]["name"], "N3");
    assert_eq!(nets[2]["class"], Value::Null);
    assert_eq!(nets[2]["tracks"], 0);
    assert_eq!(nets[2]["widthStatus"], "none");
    assert_eq!(nets[2]["clearance"], Value::Null);

    let pairs = report["pairs"].as_array().expect("pairs is an array");
    assert_eq!(pairs[0]["a"], "N1");
    assert_eq!(pairs[0]["b"], "N2");
    assert_eq!(pairs[0]["actual"], 0.25);
    assert_eq!(pairs[0]["required"], 0.5);
    assert_eq!(pairs[0]["ok"], false);
    assert_eq!(pairs[0]["layer"], "F.Cu");

    let revision = summary(handle).expect("the board is listed")["revision"].as_u64().expect("a revision");
    let again = net_audit(handle).expect("the audit is deterministic");
    assert_eq!(again["nets"], report["nets"]);
    assert_eq!(summary(handle).expect("the board is listed")["revision"].as_u64(), Some(revision), "the audit edits nothing");

    close(handle).expect("the board closes");
    let missing = net_audit(handle).expect_err("a closed handle is rejected");
    assert_eq!(missing.code, "board/not-found");
}

#[test]
fn testpoint_coverage_classifies_covered_uncovered_and_unconnected_nets() {
    let handle = created();
    for name in ["GND", "VCC", "SIG", "FLOAT"] {
        add_net(handle, AddNetRequest { name: name.to_string() }).expect("the net is declared");
    }
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("U1".to_string()),
            value: Some("MCU".to_string()),
            footprint: Some("QFN".to_string()),
            x: Some(20.0),
            y: Some(20.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the footprint is placed");
    for (number, net) in [("1", "GND"), ("2", "VCC"), ("3", "SIG")] {
        add_pad(
            handle,
            AddPadRequest {
                footprint: "fp-1".to_string(),
                number: number.to_string(),
                x: number.parse::<f64>().expect("a column") - 1.0,
                y: 0.0,
                w: 0.8,
                h: 0.8,
                net: Some(net.to_string()),
                drill: None,
            },
        )
        .expect("the pad is added");
    }
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [10.0, 10.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed");
    add_test_point(handle, AddTestPointRequest { target: "fp-1:1".to_string() }).expect("the pad is a probe");
    add_test_point(handle, AddTestPointRequest { target: "via-1".to_string() }).expect("the via is a probe");

    let report = testpoint_coverage(handle).expect("the coverage report is built");
    assert_eq!(report["report"], "open-pcb-testpoint-coverage");
    assert_eq!(report["units"], "mm");
    assert_eq!(report["status"], "error");
    assert_eq!(report["netCount"], 4);
    assert_eq!(report["coveredCount"], 2);
    assert_eq!(report["uncoveredCount"], 1);
    assert_eq!(report["unconnectedCount"], 1);
    assert_eq!(report["probeCount"], 2);
    assert_eq!(report["unassignedProbes"], 0);
    assert_eq!(report["coveragePercent"], 66.666667);
    let codes: Vec<&str> =
        report["issues"].as_array().expect("issues is an array").iter().filter_map(|issue| issue["code"].as_str()).collect();
    assert_eq!(codes, ["testpoints/net-not-covered", "testpoints/net-unconnected"]);

    let nets = report["nets"].as_array().expect("nets is an array");
    assert_eq!(nets[0]["name"], "GND");
    assert_eq!(nets[0]["pads"], 1);
    assert_eq!(nets[0]["probes"], 1);
    assert_eq!(nets[0]["covered"], true);
    assert_eq!(nets[1]["name"], "VCC");
    assert_eq!(nets[1]["vias"], 1);
    assert_eq!(nets[1]["probes"], 1);
    assert_eq!(nets[1]["covered"], true);
    assert_eq!(nets[2]["name"], "SIG");
    assert_eq!(nets[2]["pads"], 1);
    assert_eq!(nets[2]["probes"], 0);
    assert_eq!(nets[2]["covered"], false);
    assert_eq!(nets[3]["name"], "FLOAT");
    assert_eq!(nets[3]["pads"], 0);
    assert_eq!(nets[3]["covered"], false);

    let probes = report["probes"].as_array().expect("probes is an array");
    assert_eq!(probes[0]["target"], "fp-1:1");
    assert_eq!(probes[0]["designator"], "U1");
    assert_eq!(probes[0]["pad"], "1");
    assert_eq!(probes[1]["target"], "via-1");

    let revision = summary(handle).expect("the board is listed")["revision"].as_u64().expect("a revision");
    let again = testpoint_coverage(handle).expect("the report is deterministic");
    assert_eq!(again["nets"], report["nets"]);
    assert_eq!(summary(handle).expect("the board is listed")["revision"].as_u64(), Some(revision), "the report edits nothing");

    close(handle).expect("the board closes");
    let missing = testpoint_coverage(handle).expect_err("a closed handle is rejected");
    assert_eq!(missing.code, "board/not-found");
}

#[test]
fn empty_board_exports_every_supported_format_without_panic() {
    let dir = TempDir::new();
    let handle = store::insert(crafted());
    for format in [
        "gbr",
        "drl",
        "drill_map",
        "bom",
        "pos",
        "placement",
        "testpoints",
        "ipc_d_356",
        "svg",
        "pdf",
        "kicad_pcb",
        "ipc2581",
        "png",
    ] {
        let target = dir.join(&format!("empty.{format}"));
        let result = export(handle, format, &target);
        if matches!(format, "gbr" | "png") {
            let error = result.expect_err("`{format}` should need an outline");
            assert_eq!(error.code, "format/invalid", "{format} error code");
        }
        else {
            let report = result.unwrap_or_else(|error| panic!("`{format}` failed: {}", error.message));
            let bytes = report["bytes"].as_u64().expect("the report counts bytes");
            assert!(bytes > 0, "`{format}` wrote an empty file for an empty board");
        }
    }
    close(handle).expect("the board closes");
}

#[test]
fn gerber_pads_follow_footprint_rotation() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("R1".to_string()),
            value: Some("10k".to_string()),
            footprint: Some("0603".to_string()),
            x: Some(10.0),
            y: Some(10.0),
            rotation: Some(90.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the quarter-turn footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 2.0,
            y: 0.0,
            w: 1.0,
            h: 0.5,
            net: Some("VCC".to_string()),
            drill: Some(0.3),
        },
    )
    .expect("the pad is added");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("R2".to_string()),
            value: Some("22k".to_string()),
            footprint: Some("0805".to_string()),
            x: Some(20.0),
            y: Some(20.0),
            rotation: Some(45.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the angled footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-2".to_string(),
            number: "1".to_string(),
            x: 2.0,
            y: 0.0,
            w: 2.0,
            h: 1.0,
            net: Some("VCC".to_string()),
            drill: None,
        },
    )
    .expect("the pad is added");
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");

    let directory = TempDir::new();
    let gbr = directory.join("gerber");
    export(handle, "gbr", &gbr).expect("the gerber directory writes");
    let copper = fs::read_to_string(gbr.join("F_Cu.gbr")).expect("the copper layer is readable");
    assert!(copper.contains("%ADD10R,0.500000X1.000000*%"), "a quarter turn swaps the aperture sides; got {copper}");
    assert!(copper.contains("X10000000Y12000000D03*"), "the rotated pad lands at its folded centre; got {copper}");
    assert!(copper.contains("G36*"), "the angled pad is drawn as a region; got {copper}");
    assert!(copper.contains("X21060660Y20353553D02*"), "the region starts at the rotated corner; got {copper}");
    let mask = fs::read_to_string(gbr.join("F_Mask.gbr")).expect("the mask layer is readable");
    assert!(mask.contains("%ADD10R,0.600000X1.100000*%"), "the mask opening keeps the swap and the expansion; got {mask}");
    assert!(mask.contains("X10000000Y12000000D03*"), "the mask opening follows the rotated centre; got {mask}");
    let paste = fs::read_to_string(gbr.join("F_Paste.gbr")).expect("the paste layer is readable");
    assert!(paste.contains("X10000000Y12000000D03*"), "the stencil window follows the rotated centre; got {paste}");
    assert!(paste.contains("G36*"), "the angled pad opens a rotated stencil window; got {paste}");

    let drill = directory.join("rotated.drl");
    export(handle, "drl", &drill).expect("the drill file writes");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    assert!(text.contains("X10.000Y12.000"), "the drilled pad hits the same centre as the copper; got {text}");
    close(handle).expect("the board closes");
}

#[test]
fn four_layer_gerber_and_drill_carry_the_via_spans() {
    let handle = created();
    inner_layers(handle, &["In1.Cu", "In2.Cu"]);
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "In1.Cu".to_string(),
            width: 0.2,
            from: [10.0, 10.0],
            to: [40.0, 10.0],
        },
    )
    .expect("the inner track is routed");
    for (at, from, to) in
        [([5.0, 5.0], None, None), ([6.0, 6.0], Some("F.Cu"), Some("In1.Cu")), ([7.0, 7.0], Some("In1.Cu"), Some("In2.Cu"))]
    {
        add_via(
            handle,
            AddViaRequest {
                net: Some("VCC".to_string()),
                at,
                diameter: 0.6,
                drill: 0.3,
                from_layer: from.map(str::to_string),
                to_layer: to.map(str::to_string),
                ..Default::default()
            },
        )
        .expect("the via is placed");
    }
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [50.0, 40.0] }).expect("the outline is set");

    let directory = TempDir::new();
    let gbr = directory.join("gerber");
    let report = export(handle, "gbr", &gbr).expect("the gerber directory writes");
    let names = report["artifacts"]
        .as_array()
        .expect("artifacts is an array")
        .iter()
        .map(|item| item["name"].as_str().expect("a name").to_string())
        .collect::<Vec<_>>();
    assert!(
        names.iter().any(|name| name == "In1_Cu.gbr") && names.iter().any(|name| name == "In2_Cu.gbr"),
        "inner layers are written"
    );
    let read = |name: &str| fs::read_to_string(gbr.join(name)).expect("the gerber file is readable");
    let top = read("F_Cu.gbr");
    assert!(top.contains("%TF.FileFunction,Copper,L1,Top*%"), "the top layer stays L1 in a four-layer stack");
    assert!(
        top.contains("X5000000Y5000000D03*") && top.contains("X6000000Y6000000D03*"),
        "through and blind vias reach the top"
    );
    assert!(!top.contains("X7000000Y7000000"), "a buried via never reaches an outer layer");
    let in1 = read("In1_Cu.gbr");
    assert!(in1.contains("%TF.FileFunction,Copper,L2,Inr*%"), "the first inner layer is L2,Inr");
    assert!(
        in1.contains("X5000000Y5000000D03*") && in1.contains("X6000000Y6000000D03*") && in1.contains("X7000000Y7000000D03*"),
        "every via span covers the first inner layer"
    );
    assert!(in1.contains("D01*"), "the inner track is drawn on its own layer");
    let in2 = read("In2_Cu.gbr");
    assert!(in2.contains("%TF.FileFunction,Copper,L3,Inr*%"), "the second inner layer is L3,Inr");
    assert!(in2.contains("X7000000Y7000000D03*") && !in2.contains("X6000000Y6000000"), "only the buried span reaches In2");
    let bottom = read("B_Cu.gbr");
    assert!(bottom.contains("%TF.FileFunction,Copper,L4,Bot*%"), "the bottom layer becomes L4");
    assert!(
        bottom.contains("X5000000Y5000000D03*") && !bottom.contains("X6000000Y6000000"),
        "only the through via reaches the bottom"
    );

    let drill = directory.join("stack.drl");
    let report = export(handle, "drl", &drill).expect("the drill file writes");
    assert_eq!(report["losses"][0]["code"], "drl/partial-span-via", "partial spans are a reported loss");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    assert!(
        text.contains("X5.000Y5.000") && text.contains("X6.000Y6.000") && text.contains("X7.000Y7.000"),
        "every via still contributes a hit; got {text}"
    );
    close(handle).expect("the board closes");

    let plain = created();
    add_via(plain, AddViaRequest { at: [5.0, 5.0], diameter: 0.6, drill: 0.3, ..Default::default() })
        .expect("the through via is placed");
    let through_drill = directory.join("through.drl");
    let report = export(plain, "drl", &through_drill).expect("the through drill file writes");
    assert!(report.get("losses").is_none(), "a through-only board loses nothing");
    close(plain).expect("the board closes");
}

#[test]
fn negative_coordinates_reach_every_fabrication_writer() {
    let handle = created();
    add_net(handle, AddNetRequest { name: "VCC".to_string() }).expect("the net is declared");
    set_outline(handle, SetOutlineRequest { from: [-30.0, -20.0], to: [30.0, 20.0] }).expect("the outline is set");
    add_track(
        handle,
        AddTrackRequest {
            net: Some("VCC".to_string()),
            layer: "F.Cu".to_string(),
            width: 0.2,
            from: [-10.0, -5.0],
            to: [10.0, 5.0],
        },
    )
    .expect("the track crosses the origin");
    add_via(
        handle,
        AddViaRequest { net: Some("VCC".to_string()), at: [-3.0, -4.0], diameter: 0.6, drill: 0.3, ..Default::default() },
    )
    .expect("the via is placed in a negative quadrant");
    add_footprint(
        handle,
        AddFootprintRequest {
            reference: Some("C1".to_string()),
            value: Some("100n".to_string()),
            footprint: Some("0402".to_string()),
            x: Some(-15.0),
            y: Some(-10.0),
            rotation: Some(0.0),
            layer: Some("F.Cu".to_string()),
        },
    )
    .expect("the footprint is placed");
    add_pad(
        handle,
        AddPadRequest {
            footprint: "fp-1".to_string(),
            number: "1".to_string(),
            x: 2.0,
            y: 1.0,
            w: 1.0,
            h: 1.0,
            net: Some("VCC".to_string()),
            drill: Some(0.4),
        },
    )
    .expect("the pad is added");

    let directory = TempDir::new();
    let gbr = directory.join("gerber");
    export(handle, "gbr", &gbr).expect("the gerber directory writes");
    let copper = fs::read_to_string(gbr.join("F_Cu.gbr")).expect("the copper layer is readable");
    assert!(copper.contains("X-10000000Y-5000000D02*"), "the track keeps its negative start; got {copper}");
    assert!(copper.contains("X-3000000Y-4000000D03*"), "the via keeps its negative centre; got {copper}");
    assert!(copper.contains("X-13000000Y-9000000D03*"), "the pad keeps its negative centre; got {copper}");
    let edges = fs::read_to_string(gbr.join("Edge_Cuts.gbr")).expect("edge cuts are readable");
    assert!(edges.contains("X-30000000Y-20000000D02*"), "the outline keeps its negative corner; got {edges}");

    let drill = directory.join("negative.drl");
    export(handle, "drl", &drill).expect("the drill file writes");
    let text = fs::read_to_string(&drill).expect("the drill file is readable");
    assert!(text.contains("X-3.000Y-4.000"), "the via hit keeps its sign; got {text}");
    assert!(text.contains("X-13.000Y-9.000"), "the pad hit keeps its sign; got {text}");

    let netlist = directory.join("negative.ipc");
    export(handle, "ipc_d_356", &netlist).expect("the netlist writes");
    let text = fs::read_to_string(&netlist).expect("the netlist is readable");
    assert!(text.contains("X-003000Y-004000"), "the probe coordinate carries its sign; got {text}");

    for (format, name) in
        [("svg", "negative.svg"), ("pdf", "negative.pdf"), ("kicad_pcb", "negative.kicad_pcb"), ("ipc2581", "negative.xml")]
    {
        let path = directory.join(name);
        export(handle, format, &path).unwrap_or_else(|error| panic!("`{format}` failed: {}", error.message));
        assert!(fs::metadata(&path).expect("the file exists").len() > 0, "`{format}` wrote an empty file");
    }
    close(handle).expect("the board closes");
}

#[test]
fn gerber_rejects_coordinates_beyond_its_declared_format() {
    let handle = created();
    set_outline(handle, SetOutlineRequest { from: [0.0, 0.0], to: [12_000.0, 80.0] }).expect("the oversized outline is set");
    let directory = TempDir::new();
    let gbr = directory.join("gerber");
    let error = export(handle, "gbr", &gbr).expect_err("coordinates beyond 4.6 must fail instead of misparsing");
    assert_eq!(error.code, "format/invalid", "the error names the format");
    assert!(error.message.contains("4.6"), "the error explains the declared format; got {}", error.message);
    assert!(!gbr.join("F_Cu.gbr").exists(), "no layer file is written for a rejected export");
    close(handle).expect("the board closes");
}

#[test]
fn mil_board_writes_millimetre_gerber_geometry() {
    let mut board = crafted();
    board.units = crate::board::Units::Mil;
    board.outline = Some(crate::board::Outline { from: [0.0, 0.0], to: [4000.0, 3000.0] });
    board.footprints.push(Footprint {
        id: "fp-mil".to_string(),
        ref_: "U1".to_string(),
        value: String::new(),
        footprint: String::new(),
        x: 1000.0,
        y: 2000.0,
        rotation: 0.0,
        layer: "F.Cu".to_string(),
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 400.0, h: 200.0, net: None, drill: None }],
    });
    let handle = store::insert(board);
    let directory = TempDir::new();
    let gbr = directory.join("gerber");
    export(handle, "gbr", &gbr).expect("the mil board exports");
    let copper = fs::read_to_string(gbr.join("F_Cu.gbr")).expect("the copper layer is readable");
    assert!(copper.contains("%ADD10R,10.160000X5.080000*%"), "pad sizes are converted to millimetres; got {copper}");
    assert!(copper.contains("X25400000Y50800000D03*"), "placement is converted to millimetres; got {copper}");
    let mask = fs::read_to_string(gbr.join("F_Mask.gbr")).expect("the mask layer is readable");
    assert!(mask.contains("%ADD10R,10.260000X5.180000*%"), "the millimetre rule expands after the unit scale; got {mask}");
    close(handle).expect("the board closes");
}
