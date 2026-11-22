//! IPC-2581B XML reader.
//!
//! The reader parses the subset of IPC-2581 the native document can hold: a
//! rectangular profile, package definitions with pins, component placements,
//! logical nets, line features and plated holes. It never approximates
//! geometry: anything the document cannot carry is skipped and reported in the
//! loss ledger with a stable code from [`LOSS_CODES`].
//!
//! The writer in [`crate::ipc2581`] emits copper and drill attributes
//! (`net`, `padDiameter`, `fromLayer`, `toLayer`) so a board written and read
//! back keeps its nets and via spans. Files from other generators are read the
//! same way, with those attributes simply absent.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ServiceError,
    board::{BackDrill, Board, Footprint, Layer, LayerKind, Net, Outline, Pad, Track, Via},
    kicad_import::Loss,
};

/// Stable codes the reader may report in its loss ledger.
pub(crate) const LOSS_CODES: &[&str] = &[
    "ipc2581/construct-not-imported",
    "ipc2581/outline-not-rectangular",
    "ipc2581/net-duplicate",
    "ipc2581/net-unknown",
    "ipc2581/package-without-name",
    "ipc2581/package-duplicate",
    "ipc2581/package-missing",
    "ipc2581/component-without-reference",
    "ipc2581/component-duplicate-reference",
    "ipc2581/component-layer-unmapped",
    "ipc2581/pin-invalid",
    "ipc2581/pin-without-size",
    "ipc2581/pin-number-duplicate",
    "ipc2581/track-invalid",
    "ipc2581/track-layer-missing",
    "ipc2581/track-without-width",
    "ipc2581/hole-invalid",
    "ipc2581/hole-not-plated",
    "ipc2581/back-drill-invalid",
    "ipc2581/units-unknown",
];

/// A parsed IPC-2581 document and everything the board model did not keep.
pub(crate) struct Imported {
    /// The native board the file maps to.
    pub(crate) board: Board,
    /// Per-item losses in document order.
    pub(crate) losses: Vec<Loss>,
}

/// One XML element with its local name, attributes and children.
///
/// Namespace prefixes are dropped while reading, so `<ipc:Step>` and `<Step>`
/// resolve to the same name. Text content is ignored: IPC-2581 carries board
/// geometry in attributes.
#[derive(Debug, Default)]
struct Element {
    name: String,
    attrs: BTreeMap<String, String>,
    children: Vec<Element>,
}

impl Element {
    /// First child element with one local name.
    fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|child| child.name == name)
    }

    /// Every child element with one local name, in document order.
    fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |child| child.name == name)
    }

    /// First present attribute among several accepted spellings.
    fn attr(&self, names: &[&str]) -> Option<&str> {
        names.iter().find_map(|name| self.attrs.get(*name).map(String::as_str))
    }

    /// First present numeric attribute among several accepted spellings.
    fn number(&self, names: &[&str]) -> Option<f64> {
        self.attr(names).and_then(parse_number)
    }
}

/// Parses one IPC-2581 document into a board the model can validate.
pub(crate) fn parse(source: &str, id: String) -> Result<Imported, ServiceError> {
    let root = parse_document(source)?;
    if root.name != "IPC-2581" {
        return Err(crate::invalid(format!("the document starts with `<{}>`, expected `<IPC-2581>`", root.name)));
    }

    let mut losses = Vec::new();
    let scale = scale_of(&root, &mut losses);
    let mut board = Board::new(id, None, None)?;

    let mut steps = Vec::new();
    descendants(&root, "Step", &mut steps);
    let Some(step) = steps.first().copied()
    else {
        return Err(crate::invalid("the IPC-2581 document has no `<Step>` element"));
    };
    for extra in steps.iter().skip(1) {
        let name = extra.attr(&["name"]).unwrap_or("<unnamed>");
        loss(
            &mut losses,
            "ipc2581/construct-not-imported",
            format!("step `{name}` is not imported; only the first `<Step>` is read"),
        );
    }
    if let Some(title) = step.attr(&["name"]).map(str::trim).filter(|title| !title.is_empty()) {
        board.title = Some(title.to_string());
    }

    board.outline = outline_of(step, scale, &mut losses);
    let feature_layers = layer_features(step, &mut board, &mut losses);
    declare_nets(step, &mut board, &mut losses);
    let pad_nets = pad_nets_of(&feature_layers);
    let packages = packages_of(step, &mut losses);
    import_components(step, &packages, &pad_nets, &mut board, scale, &mut losses);
    import_tracks(&feature_layers, &mut board, scale, &mut losses);
    import_holes(&feature_layers, &mut board, scale, &mut losses);

    Ok(Imported { board, losses })
}

/// Scales source units to millimetres, reporting an unknown unit.
fn scale_of(root: &Element, losses: &mut Vec<Loss>) -> f64 {
    let raw = root.attr(&["units"]).map(str::trim).map(str::to_ascii_lowercase);
    match raw.as_deref() {
        None | Some("") | Some("mm") | Some("millimeter") | Some("millimeters") | Some("millimetre") | Some("millimetres") => {
            1.0
        }
        Some("inch") | Some("in") | Some("inches") => 25.4,
        Some("mil") | Some("mils") | Some("thou") => 0.0254,
        Some(other) => {
            loss(
                losses,
                "ipc2581/units-unknown",
                format!("unit `{other}` is not recognised; coordinates are read as millimetres"),
            );
            1.0
        }
    }
}

/// Reads a rectangular `Profile` and reports any other outline.
fn outline_of(step: &Element, scale: f64, losses: &mut Vec<Loss>) -> Option<Outline> {
    let profile = step.child("Profile")?;
    let mut lines = Vec::new();
    descendants(profile, "Line", &mut lines);
    if lines.is_empty() {
        return None;
    }
    let mut corners: Vec<[f64; 2]> = Vec::new();
    for line in &lines {
        let start = [line.number(&["startX"])?, line.number(&["startY"])?];
        let start = [start[0] * scale, start[1] * scale];
        if corners.last() != Some(&start) {
            corners.push(start);
        }
    }
    if corners.first() == corners.last() && corners.len() > 1 {
        corners.pop();
    }
    if corners.len() != 4 {
        loss(
            losses,
            "ipc2581/outline-not-rectangular",
            format!("the profile has {} corner(s); only an axis-aligned rectangle is imported", corners.len()),
        );
        return None;
    }
    let xs: BTreeSet<String> = corners.iter().map(|corner| format!("{:.6}", corner[0])).collect();
    let ys: BTreeSet<String> = corners.iter().map(|corner| format!("{:.6}", corner[1])).collect();
    let rectangle = xs.len() == 2
        && ys.len() == 2
        && (0..4).all(|index| {
            let current = corners[index];
            let next = corners[(index + 1) % 4];
            (current[0] == next[0]) != (current[1] == next[1])
        });
    if !rectangle {
        loss(
            losses,
            "ipc2581/outline-not-rectangular",
            "the profile is not an axis-aligned rectangle and is not imported".to_string(),
        );
        return None;
    }
    let from = [
        corners.iter().map(|corner| corner[0]).fold(f64::INFINITY, f64::min),
        corners.iter().map(|corner| corner[1]).fold(f64::INFINITY, f64::min),
    ];
    let to = [
        corners.iter().map(|corner| corner[0]).fold(f64::NEG_INFINITY, f64::max),
        corners.iter().map(|corner| corner[1]).fold(f64::NEG_INFINITY, f64::max),
    ];
    Some(Outline { from, to })
}

/// Collects the layer references of every `LayerFeature`, adding unknown layers.
fn layer_features<'a>(step: &'a Element, board: &mut Board, losses: &mut Vec<Loss>) -> Vec<(String, &'a Element)> {
    let mut features = Vec::new();
    for feature in step.children_named("LayerFeature") {
        let raw = feature.attr(&["layerRef", "layer", "name"]).unwrap_or_default();
        let Some(layer) = canonical_layer(raw, board)
        else {
            loss(losses, "ipc2581/track-layer-missing", "a layer feature names no layer; its geometry is skipped".to_string());
            continue;
        };
        ensure_layer(board, &layer, layer_kind_of(feature.attr(&["kind", "layerKind"])));
        features.push((layer, feature));
    }
    features
}

/// Declares one net per `LogicalNet` name, keeping the first.
fn declare_nets(step: &Element, board: &mut Board, losses: &mut Vec<Loss>) {
    let mut nets = Vec::new();
    descendants(step, "LogicalNet", &mut nets);
    let mut declared = BTreeSet::new();
    for element in nets {
        let name = element.attr(&["name"]).unwrap_or_default().trim();
        if name.is_empty() {
            continue;
        }
        if !declared.insert(name.to_string()) {
            loss(losses, "ipc2581/net-duplicate", format!("net `{name}` is declared more than once"));
            continue;
        }
        let id = format!("net-{}", board.next_net);
        board.next_net += 1;
        board.nets.push(Net { id, name: name.to_string() });
    }
}

/// Maps `pinRef` (`REF.PAD`) to the net a `Pad` feature carries.
fn pad_nets_of(features: &[(String, &Element)]) -> BTreeMap<(String, String), String> {
    let mut nets = BTreeMap::new();
    for (_, feature) in features {
        for set in feature.children_named("Set") {
            for pad in set.children_named("Pad") {
                let Some((reference, number)) = pad.attr(&["pinRef"]).and_then(|pin_ref| pin_ref.rsplit_once('.'))
                else {
                    continue;
                };
                if let Some(net) = pad.attr(&["net", "netRef"]).map(str::trim).filter(|net| !net.is_empty()) {
                    nets.entry((reference.to_string(), number.to_string())).or_insert_with(|| net.to_string());
                }
            }
        }
    }
    nets
}

/// Indexes package definitions by name, keeping the first definition.
fn packages_of<'a>(step: &'a Element, losses: &mut Vec<Loss>) -> BTreeMap<String, &'a Element> {
    let mut elements = Vec::new();
    descendants(step, "Package", &mut elements);
    let mut packages: BTreeMap<String, &Element> = BTreeMap::new();
    for package in elements {
        let name = package.attr(&["name", "packageRef"]).unwrap_or_default().trim().to_string();
        if name.is_empty() {
            loss(losses, "ipc2581/package-without-name", "a package definition has no name and is skipped".to_string());
            continue;
        }
        if packages.contains_key(&name) {
            loss(
                losses,
                "ipc2581/package-duplicate",
                format!("package `{name}` is defined more than once; the first definition is used"),
            );
            continue;
        }
        packages.insert(name, package);
    }
    packages
}

/// Imports every `Component` placement and its package pins.
fn import_components(
    step: &Element,
    packages: &BTreeMap<String, &Element>,
    pad_nets: &BTreeMap<(String, String), String>,
    board: &mut Board,
    scale: f64,
    losses: &mut Vec<Loss>,
) {
    let declared: BTreeSet<String> = board.nets.iter().map(|net| net.name.clone()).collect();
    let mut elements = Vec::new();
    descendants(step, "Component", &mut elements);
    let mut references = BTreeSet::new();
    for component in elements {
        let reference = component.attr(&["ref", "refDes", "name"]).unwrap_or_default().trim().to_string();
        if reference.is_empty() {
            loss(losses, "ipc2581/component-without-reference", "a component has no reference and is skipped".to_string());
            continue;
        }
        if !references.insert(reference.clone()) {
            loss(
                losses,
                "ipc2581/component-duplicate-reference",
                format!("component `{reference}` is placed more than once; the extra placement is skipped"),
            );
            continue;
        }
        let raw_layer = component.attr(&["layer", "layerRef", "side"]).unwrap_or("F.Cu");
        let Some(layer) = canonical_layer(raw_layer, board)
        else {
            loss(losses, "ipc2581/component-layer-unmapped", format!("component `{reference}` names no layer and is skipped"));
            continue;
        };
        ensure_layer(board, &layer, None);
        if board.layer(&layer).is_none_or(|entry| entry.kind != LayerKind::Copper) {
            loss(
                losses,
                "ipc2581/component-layer-unmapped",
                format!("component `{reference}` is placed on non-copper layer `{layer}` and is skipped"),
            );
            continue;
        }
        let x = component.number(&["x"]).unwrap_or(0.0);
        let y = component.number(&["y"]).unwrap_or(0.0);
        let rotation = component
            .number(&["rotation", "rot"])
            .or_else(|| component.child("Xform").and_then(|xform| xform.number(&["rotation"])))
            .unwrap_or(0.0);
        let part = component.attr(&["part", "packageRef"]).unwrap_or_default().trim().to_string();
        let value = component.attr(&["value"]).unwrap_or_default().trim().to_string();
        let pads = match packages.get(&part) {
            Some(package) if !part.is_empty() => pads_of(package, &reference, pad_nets, &declared, scale, losses),
            _ => {
                if part.is_empty() {
                    loss(
                        losses,
                        "ipc2581/package-missing",
                        format!("component `{reference}` names no package; the footprint is imported without pads"),
                    );
                }
                else {
                    loss(
                        losses,
                        "ipc2581/package-missing",
                        format!(
                            "component `{reference}` references undefined package `{part}`; the footprint is imported without pads"
                        ),
                    );
                }
                Vec::new()
            }
        };
        let id = format!("fp-{}", board.next_footprint);
        board.next_footprint += 1;
        board.footprints.push(Footprint {
            id,
            ref_: reference,
            value,
            footprint: part,
            x: x * scale,
            y: y * scale,
            rotation,
            layer,
            pads,
        });
    }
}

/// Reads the pins of one package into pads of one footprint.
fn pads_of(
    package: &Element,
    reference: &str,
    pad_nets: &BTreeMap<(String, String), String>,
    declared: &BTreeSet<String>,
    scale: f64,
    losses: &mut Vec<Loss>,
) -> Vec<Pad> {
    let mut pads = Vec::new();
    let mut numbers = BTreeSet::new();
    for pin in package.children_named("Pin") {
        let number = pin.attr(&["number", "name"]).unwrap_or_default().trim().to_string();
        if number.is_empty() {
            loss(losses, "ipc2581/pin-invalid", format!("a pin of `{reference}` has no number and is skipped"));
            continue;
        }
        if !numbers.insert(number.clone()) {
            loss(losses, "ipc2581/pin-number-duplicate", format!("pin `{number}` of `{reference}` is defined more than once"));
            continue;
        }
        let location = pin.child("Location");
        let x = pin.number(&["x"]).or_else(|| location.and_then(|node| node.number(&["x"]))).unwrap_or(0.0);
        let y = pin.number(&["y"]).or_else(|| location.and_then(|node| node.number(&["y"]))).unwrap_or(0.0);
        let Some((width, height)) = pin_size(pin)
        else {
            loss(
                losses,
                "ipc2581/pin-without-size",
                format!("pin `{number}` of `{reference}` has no rectangular or circular size and is skipped"),
            );
            continue;
        };
        if !(width > 0.0 && height > 0.0) {
            loss(losses, "ipc2581/pin-invalid", format!("pin `{number}` of `{reference}` has a non-positive size"));
            continue;
        }
        let raw_net = pin
            .attr(&["net", "netRef"])
            .map(str::trim)
            .filter(|net| !net.is_empty())
            .map(str::to_string)
            .or_else(|| pad_nets.get(&(reference.to_string(), number.clone())).cloned());
        let net = match raw_net {
            Some(net) if declared.contains(&net) => Some(net),
            Some(net) => {
                loss(losses, "ipc2581/net-unknown", format!("pin `{number}` of `{reference}` names undeclared net `{net}`"));
                None
            }
            None => None,
        };
        pads.push(Pad { number, x: x * scale, y: y * scale, w: width * scale, h: height * scale, net, drill: None });
    }
    pads
}

/// Reads the rectangular or circular size of one pin.
fn pin_size(pin: &Element) -> Option<(f64, f64)> {
    let primitive = pin.child("StandardPrimitive").unwrap_or(pin);
    if let Some(rect) = primitive.child("Rect").or_else(|| pin.child("Rect")) {
        return Some((rect.number(&["width"])?, rect.number(&["height"])?));
    }
    if let Some(circle) = primitive.child("Circle").or_else(|| pin.child("Circle")) {
        let diameter = circle.number(&["diameter"])?;
        return Some((diameter, diameter));
    }
    if let Some(oval) = primitive.child("Oval").or_else(|| pin.child("Oval")) {
        return Some((oval.number(&["width"])?, oval.number(&["height"])?));
    }
    None
}

/// Imports every `Set/Line` feature as one track.
fn import_tracks(features: &[(String, &Element)], board: &mut Board, scale: f64, losses: &mut Vec<Loss>) {
    let declared: BTreeSet<String> = board.nets.iter().map(|net| net.name.clone()).collect();
    let mut track_nets = Vec::new();
    for (layer, feature) in features {
        for set in feature.children_named("Set") {
            for line in set.children_named("Line") {
                let Some(width) = line.number(&["width"]).map(|width| width * scale)
                else {
                    loss(losses, "ipc2581/track-without-width", "a track has no width and is skipped".to_string());
                    continue;
                };
                let (Some(start_x), Some(start_y), Some(end_x), Some(end_y)) =
                    (line.number(&["startX"]), line.number(&["startY"]), line.number(&["endX"]), line.number(&["endY"]))
                else {
                    loss(losses, "ipc2581/track-invalid", "a track has an incomplete endpoint and is skipped".to_string());
                    continue;
                };
                if width <= 0.0 {
                    loss(losses, "ipc2581/track-invalid", "a track has a non-positive width and is skipped".to_string());
                    continue;
                }
                let raw_net = line.attr(&["net", "netRef"]).map(str::trim).filter(|net| !net.is_empty()).map(str::to_string);
                let net = match raw_net {
                    Some(net) if declared.contains(&net) => Some(net),
                    Some(net) => {
                        loss(losses, "ipc2581/net-unknown", format!("a track names undeclared net `{net}`"));
                        None
                    }
                    None => None,
                };
                let id = format!("track-{}", board.next_track);
                board.next_track += 1;
                track_nets.push(Track {
                    id,
                    net,
                    layer: layer.clone(),
                    width,
                    from: [start_x * scale, start_y * scale],
                    to: [end_x * scale, end_y * scale],
                    teardrop: false,
                });
            }
        }
    }
    board.tracks.extend(track_nets);
}

/// Imports every plated `Hole` as a through via.
fn import_holes(features: &[(String, &Element)], board: &mut Board, scale: f64, losses: &mut Vec<Loss>) {
    let declared: BTreeSet<String> = board.nets.iter().map(|net| net.name.clone()).collect();
    let mut seen = BTreeSet::new();
    let mut vias = Vec::new();
    for (_, feature) in features {
        for hole in feature.children_named("Hole") {
            let plating = hole.attr(&["plating"]).unwrap_or_default().trim().to_ascii_lowercase();
            if matches!(plating.as_str(), "non-plated" | "nonplated" | "npth" | "false") {
                loss(
                    losses,
                    "ipc2581/hole-not-plated",
                    "a non-plated hole cannot be represented as a via and is skipped".to_string(),
                );
                continue;
            }
            let (Some(x), Some(y), Some(drill)) = (hole.number(&["x"]), hole.number(&["y"]), hole.number(&["diameter"]))
            else {
                loss(losses, "ipc2581/hole-invalid", "a hole has incomplete geometry and is skipped".to_string());
                continue;
            };
            let drill = drill * scale;
            let diameter = hole.number(&["padDiameter"]).map(|value| value * scale).unwrap_or(drill);
            if !(drill > 0.0 && diameter > 0.0) {
                loss(losses, "ipc2581/hole-invalid", "a hole has a non-positive diameter and is skipped".to_string());
                continue;
            }
            let at = [x * scale, y * scale];
            let key = format!("{:.6},{:.6},{:.6}", at[0], at[1], drill);
            if !seen.insert(key) {
                continue;
            }
            let raw_net = hole.attr(&["net", "netRef"]).map(str::trim).filter(|net| !net.is_empty()).map(str::to_string);
            let net = match raw_net {
                Some(net) if declared.contains(&net) => Some(net),
                Some(net) => {
                    loss(losses, "ipc2581/net-unknown", format!("a hole names undeclared net `{net}`"));
                    None
                }
                None => None,
            };
            let from_layer = hole.attr(&["fromLayer"]).map(str::trim).filter(|layer| !layer.is_empty()).map(str::to_string);
            let to_layer = hole.attr(&["toLayer"]).map(str::trim).filter(|layer| !layer.is_empty()).map(str::to_string);
            let (from_layer, to_layer) = match (from_layer, to_layer) {
                (Some(from), Some(to)) if from != to => {
                    if board.layer(&from).is_none() || board.layer(&to).is_none() {
                        (None, None)
                    }
                    else {
                        (Some(from), Some(to))
                    }
                }
                _ => (None, None),
            };
            let back_drill = match hole.number(&["backDrillDepth"]) {
                Some(depth) if depth > 0.0 => {
                    let stub = hole.number(&["backDrillStub"]).unwrap_or(0.0) * scale;
                    if stub < 0.0 {
                        loss(losses, "ipc2581/back-drill-invalid", "a hole has a negative back-drill stub".to_string());
                        None
                    }
                    else {
                        Some(BackDrill { depth: depth * scale, stub })
                    }
                }
                Some(_) => {
                    loss(losses, "ipc2581/back-drill-invalid", "a hole has a non-positive back-drill depth".to_string());
                    None
                }
                None => None,
            };
            let id = format!("via-{}", board.next_via);
            board.next_via += 1;
            vias.push(Via { id, net, at, diameter, drill, from_layer, to_layer, micro: false, back_drill });
        }
    }
    board.vias.extend(vias);
}

/// Resolves a source layer name to a declared layer id or a stable synonym.
///
/// Unknown names are kept verbatim; [`ensure_layer`] adds them with an inferred
/// kind, so inner copper and silkscreen features survive the import.
fn canonical_layer(raw: &str, board: &Board) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(layer) = board.layers.iter().find(|layer| layer.id.eq_ignore_ascii_case(trimmed)) {
        return Some(layer.id.clone());
    }
    let lowered = trimmed.to_ascii_lowercase();
    let synonym = match lowered.as_str() {
        "top" | "top.cu" | "front" | "component" => Some("F.Cu"),
        "bottom" | "bottom.cu" | "back" | "solder" => Some("B.Cu"),
        "top.silks" | "f.silks" | "top silk" => Some("F.SilkS"),
        "bottom.silks" | "b.silks" | "bottom silk" => Some("B.SilkS"),
        "profile" | "edge.cuts" | "outline" => Some("Edge.Cuts"),
        _ => None,
    };
    Some(synonym.map_or_else(|| trimmed.to_string(), str::to_string))
}

/// Parses one `kind` attribute the writer emits, if present.
fn layer_kind_of(raw: Option<&str>) -> Option<LayerKind> {
    match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("copper") | Some("conductor") => Some(LayerKind::Copper),
        Some("silkscreen") | Some("legend") => Some(LayerKind::Silkscreen),
        Some("soldermask") | Some("mask") => Some(LayerKind::Soldermask),
        Some("assembly") | Some("paste") => Some(LayerKind::Assembly),
        Some("mechanical") | Some("profile") => Some(LayerKind::Mechanical),
        _ => None,
    }
}

/// Adds one layer with an inferred kind when the document does not declare it.
fn ensure_layer(board: &mut Board, id: &str, kind: Option<LayerKind>) {
    if board.layers.iter().any(|layer| layer.id == id) {
        return;
    }
    let kind = kind.unwrap_or_else(|| infer_kind(id));
    board.layers.push(Layer {
        id: id.to_string(),
        name: id.to_string(),
        kind,
        color: color_of(kind).to_string(),
        visible: true,
        locked: false,
    });
}

/// Infers the stackup role from a layer name.
fn infer_kind(id: &str) -> LayerKind {
    let lowered = id.to_ascii_lowercase();
    if lowered.ends_with(".cu") || lowered.ends_with("_cu") || lowered.contains("copper") || is_inner_copper(&lowered) {
        LayerKind::Copper
    }
    else if lowered.contains("silk") {
        LayerKind::Silkscreen
    }
    else if lowered.contains("mask") {
        LayerKind::Soldermask
    }
    else if lowered.contains("assembly") || lowered.contains("fab") {
        LayerKind::Assembly
    }
    else {
        LayerKind::Mechanical
    }
}

/// Whether a name looks like an inner copper layer such as `In1`, `Inner2` or `L3`.
fn is_inner_copper(lowered: &str) -> bool {
    let inner = lowered.strip_prefix("inner").or_else(|| lowered.strip_prefix("in"));
    if let Some(rest) = inner {
        return !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit());
    }
    match lowered.strip_prefix('l') {
        Some(rest) => !rest.is_empty() && rest.chars().all(|character| character.is_ascii_digit()),
        None => false,
    }
}

/// Default CSS colour for a newly added layer.
fn color_of(kind: LayerKind) -> &'static str {
    match kind {
        LayerKind::Copper => "#7a5ea8",
        LayerKind::Silkscreen => "#e6e6e6",
        LayerKind::Soldermask => "#5b8a3c",
        LayerKind::Assembly => "#b0b0b0",
        LayerKind::Mechanical => "#d9b44a",
    }
}

/// Records one loss with a stable code.
fn loss(losses: &mut Vec<Loss>, code: &'static str, message: String) {
    losses.push(Loss { code, message });
}

/// Parses one numeric attribute value.
fn parse_number(raw: &str) -> Option<f64> {
    raw.trim().parse::<f64>().ok().filter(|value| value.is_finite())
}

/// Collects every descendant element with one local name, in document order.
fn descendants<'a>(node: &'a Element, name: &str, out: &mut Vec<&'a Element>) {
    for child in &node.children {
        if child.name == name {
            out.push(child);
        }
        descendants(child, name, out);
    }
}

/// Parses the whole document into its root element.
fn parse_document(source: &str) -> Result<Element, ServiceError> {
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut index = 0;
    while let Some(offset) = source[index..].find('<') {
        index += offset;
        let rest = &source[index..];
        if rest.starts_with("<!--") {
            index = after(source, index, "-->")?;
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            index = after(source, index, "]]>")?;
            continue;
        }
        if rest.starts_with("<?") {
            index = after(source, index, "?>")?;
            continue;
        }
        if rest.starts_with("<!") {
            index = after(source, index, ">")?;
            continue;
        }
        let end = end_of_tag(source, index)?;
        let body = source[index + 1..end].trim().to_string();
        index = end + 1;
        if body.is_empty() {
            continue;
        }
        if let Some(name) = body.strip_prefix('/') {
            let closing = local_name(name.trim());
            let Some(element) = stack.pop()
            else {
                return Err(crate::invalid(format!("the IPC-2581 document closes `<{closing}>` without opening it")));
            };
            if element.name != closing {
                return Err(crate::invalid(format!(
                    "the IPC-2581 document closes `<{closing}>` while `<{}>` is open",
                    element.name
                )));
            }
            attach(element, &mut stack, &mut root)?;
        }
        else {
            let self_closing = body.ends_with('/');
            let inner = if self_closing { body[..body.len() - 1].trim() } else { body.as_str() };
            let element = parse_tag(inner)?;
            if self_closing {
                attach(element, &mut stack, &mut root)?;
            }
            else {
                stack.push(element);
            }
        }
    }
    if let Some(open) = stack.first() {
        return Err(crate::invalid(format!("the IPC-2581 document never closes `<{}>`", open.name)));
    }
    root.ok_or_else(|| crate::invalid("the IPC-2581 document has no root element"))
}

/// Attaches one parsed element to its parent or to the root.
fn attach(element: Element, stack: &mut [Element], root: &mut Option<Element>) -> Result<(), ServiceError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(element);
    }
    else if root.is_none() {
        *root = Some(element);
    }
    else {
        return Err(crate::invalid("the IPC-2581 document has more than one root element"));
    }
    Ok(())
}

/// Returns the index just past one marker, or a stable failure.
fn after(source: &str, index: usize, marker: &str) -> Result<usize, ServiceError> {
    source[index..]
        .find(marker)
        .map(|offset| index + offset + marker.len())
        .ok_or_else(|| crate::invalid(format!("the IPC-2581 document has an unterminated `{marker}`")))
}

/// Returns the index of the `>` that ends the tag starting at `start`.
fn end_of_tag(source: &str, start: usize) -> Result<usize, ServiceError> {
    let mut quote: Option<char> = None;
    for (offset, character) in source[start + 1..].char_indices() {
        match character {
            '"' | '\'' => match quote {
                Some(active) if active == character => quote = None,
                Some(_) => {}
                None => quote = Some(character),
            },
            '>' if quote.is_none() => return Ok(start + 1 + offset),
            _ => {}
        }
    }
    Err(crate::invalid("the IPC-2581 document has an unterminated tag"))
}

/// Parses one tag body into a local name and its quoted attributes.
fn parse_tag(body: &str) -> Result<Element, ServiceError> {
    let body = body.trim();
    let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
    let name = local_name(&body[..name_end]);
    if name.is_empty() {
        return Err(crate::invalid("the IPC-2581 document has an element without a name"));
    }
    let mut attrs = BTreeMap::new();
    let mut rest = body[name_end..].trim_start();
    while !rest.is_empty() {
        let key_end = rest.find(|character: char| character.is_whitespace() || character == '=').unwrap_or(rest.len());
        let key = &rest[..key_end];
        rest = rest[key_end..].trim_start();
        if key.is_empty() {
            return Err(crate::invalid(format!("the IPC-2581 element `<{name}>` has a malformed attribute")));
        }
        if !rest.starts_with('=') {
            attrs.insert(local_name(key), String::new());
            continue;
        }
        rest = rest[1..].trim_start();
        let Some(quote) = rest.chars().next().filter(|character| *character == '"' || *character == '\'')
        else {
            return Err(crate::invalid(format!("the IPC-2581 element `<{name}>` has an unquoted attribute value")));
        };
        let value = &rest[quote.len_utf8()..];
        let Some(close) = value.find(quote)
        else {
            return Err(crate::invalid(format!("the IPC-2581 element `<{name}>` has an unterminated attribute value")));
        };
        attrs.insert(local_name(key), decode_entities(&value[..close]));
        rest = value[close + quote.len_utf8()..].trim_start();
    }
    Ok(Element { name, attrs, children: Vec::new() })
}

/// Drops any namespace prefix from one element or attribute name.
fn local_name(name: &str) -> String {
    name.rsplit(':').next().unwrap_or(name).to_string()
}

/// Decodes the XML entities the writer may produce.
fn decode_entities(value: &str) -> String {
    value.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}
