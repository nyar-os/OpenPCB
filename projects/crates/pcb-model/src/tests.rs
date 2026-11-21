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
    AddDifferentialPairRequest, AddFootprintRequest, AddKeepoutRequest, AddLayerRequest, AddNetClassRequest, AddNetRequest,
    AddPadRequest, AddSlotRequest, AddStitchingRequest, AddTeardropsRequest, AddTestPointRequest, AddTrackRequest,
    AddViaRequest, AddWaiverRequest, AddZoneRequest, AssignNetClassRequest, CreateRequest, ImpedanceRequest,
    MoveFootprintRequest, PanelizeRequest, RouteOptions, SetOutlineRequest, SetRulesRequest, SetSeverityRequest,
    SetStackupRequest, SetViaBackDrillRequest, UpdateFootprintRequest, UpdateLayerRequest, WidthRequest, add_differential_pair,
    add_footprint, add_keepout, add_layer, add_net, add_net_class, add_pad, add_slot, add_teardrops, add_test_point, add_track,
    add_via, add_waiver, add_zone, assign_net_class,
    board::{Board, Document, Footprint, Net, Pad, Track, Via},
    capabilities, close, copper_balance, create, delete_differential_pair, delete_footprint, delete_keepout, delete_net_class,
    delete_severity, delete_slot, delete_test_point, delete_track, delete_via, delete_waiver, delete_zone, export, import,
    inspect, move_footprint, open, panelize, redo, route, save, set_outline, set_rules, set_severity, set_stackup,
    set_via_back_drill, stackup_impedance, stackup_width, stitch_vias, store, summary, undo, update_footprint, update_layer,
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
        "copper-to-edge",
        "edge-clearance",
        "silkscreen-over-pad",
        "diffpair",
        "dangling-copper",
        "netclass",
        "keepout",
        "via-in-pad",
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
    assert_eq!(matrix["vias"]["micro"], "unsupported");
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
    assert_eq!(matrix["netClasses"], "supported");
    assert_eq!(matrix["testPoints"], "supported");
    assert_eq!(matrix["stackup"], "supported");
    assert_eq!(matrix["impedance"], "supported");
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
    let utilization = report["utilizationPercent"].as_f64().expect("utilization is a number");
    assert!((90.0..91.0).contains(&utilization), "utilization is plausible: {utilization}");
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
        AddPadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0, net: None },
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
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 0.5, h: 0.5, net: Some("GND".to_string()) }],
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
        AddPadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), x: 0.0, y: 0.0, w: 0.5, h: 0.5, net: None },
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
            .contains("%TF.FileFunction,Legend,Top*%")
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
    assert!(text.contains("/MediaBox [0 0 841.89 595.28]"), "the page is A4 landscape at 1 pt per unit");
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
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 0.5, h: 0.25, net: None }],
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
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0, net: None }],
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
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0, net: None }],
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
            pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 2.0, h: 2.0, net: None }],
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
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 2.0, h: 2.0, net: None }],
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
        pads: vec![Pad { number: "1".to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0, net: None }],
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
        AddPadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), x: 0.0, y: 0.0, w: 1.0, h: 1.0, net: None },
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
        AddPadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), x: 0.0, y: 0.0, w: 0.5, h: 0.5, net: None },
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
        AddPadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), x: 0.0, y: 0.0, w: 0.5, h: 0.5, net: None },
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
        AddPadRequest { footprint: "fp-1".to_string(), number: "1".to_string(), x: 0.0, y: 0.0, w: 0.5, h: 0.5, net: None },
    )
    .expect("the pad is added");
    add_track(clear, AddTrackRequest { net: None, layer: "F.SilkS".to_string(), width: 0.2, from: [0.0, 0.0], to: [5.0, 0.0] })
        .expect("the silkscreen track is placed");
    let report = inspect(clear, Some(vec!["silkscreen-over-pad".to_string()])).expect("the check runs");
    assert_eq!(codes(&report), Vec::<&str>::new(), "silkscreen away from pads passes");
    close(clear).expect("the board closes");
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
        back_drill: None,
    });
    let reversed_path = directory.join("reversed-span.opcb");
    fs::write(&reversed_path, serde_json::to_string(&Document::new(reversed)).expect("the document serializes"))
        .expect("the document is written");
    assert_eq!(open(&reversed_path).expect_err("a reversed span is rejected").code, "format/invalid");
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
fn kicad_import_refuses_micro_and_malformed_via_spans() {
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
    assert_eq!(codes, ["kicad/via-type", "kicad/via-layer-span"]);
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
