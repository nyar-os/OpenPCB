//! KiCad PCB reader.
//!
//! The reader parses the S-expression dialect KiCad writes into the native
//! board model. It never invents geometry: constructs the model cannot hold are
//! skipped and reported one by one in the loss ledger, keyed by a stable code
//! from [`LOSS_CODES`]. A rectangular `Edge.Cuts` outline is the only outline
//! shape the document can carry, so any other outline is refused rather than
//! approximated by its bounding box.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::{
    ServiceError,
    board::{Board, Footprint, Layer, LayerKind, Net, Outline, Pad, Track, Units, Via},
};

/// Stable codes the reader may report in its loss ledger.
pub(crate) const LOSS_CODES: &[&str] = &[
    "kicad/construct-not-imported",
    "kicad/outline-not-rectangular",
    "kicad/net-zero-named",
    "kicad/net-duplicate",
    "kicad/footprint-invalid",
    "kicad/footprint-without-reference",
    "kicad/footprint-duplicate-reference",
    "kicad/footprint-layer-missing",
    "kicad/footprint-graphics",
    "kicad/footprint-property",
    "kicad/footprint-text",
    "kicad/footprint-model",
    "kicad/footprint-zone",
    "kicad/footprint-attr",
    "kicad/footprint-field-not-modeled",
    "kicad/pad-invalid",
    "kicad/pad-size-invalid",
    "kicad/pad-number-duplicate",
    "kicad/pad-shape",
    "kicad/pad-type",
    "kicad/pad-drill",
    "kicad/pad-rotation",
    "kicad/pad-layer",
    "kicad/pad-net-unknown",
    "kicad/pad-field-not-modeled",
    "kicad/track-invalid",
    "kicad/track-layer-missing",
    "kicad/track-net-unknown",
    "kicad/via-invalid",
    "kicad/via-layer-span",
    "kicad/via-type",
    "kicad/via-net-unknown",
];

/// One item the reader could not put in the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Loss {
    /// Stable code from [`LOSS_CODES`].
    pub(crate) code: &'static str,
    /// Human-readable detail naming the source construct.
    pub(crate) message: String,
}

/// A parsed KiCad board and everything the document did not keep.
pub(crate) struct Imported {
    /// The native board the file maps to.
    pub(crate) board: Board,
    /// Per-item losses in document order.
    pub(crate) losses: Vec<Loss>,
}

/// Parses one KiCad PCB document into a board the model can validate.
pub(crate) fn parse(source: &str, id: String) -> Result<Imported, ServiceError> {
    let document = Parser::run(source)?;
    let items = body(&document).ok_or_else(|| crate::invalid("the KiCad document is not a list"))?;
    if tag(&document) != Some("kicad_pcb") {
        return Err(crate::invalid("the document does not start with `(kicad_pcb`"));
    }

    let mut losses = Vec::new();
    let title = title_of(items);
    let mut board = Board::new(id, title, None)?;

    let copper = copper_layers(items);
    if !copper.is_empty() {
        let mut layers = copper;
        for (id, name, kind, color) in [
            ("F.SilkS", "F.SilkS", LayerKind::Silkscreen, "#e6e6e6"),
            ("B.SilkS", "B.SilkS", LayerKind::Silkscreen, "#c4c4c4"),
            ("Edge.Cuts", "Edge.Cuts", LayerKind::Mechanical, "#d9b44a"),
        ] {
            layers.push(Layer {
                id: id.to_string(),
                name: name.to_string(),
                kind,
                color: color.to_string(),
                visible: true,
                locked: false,
            });
        }
        board.layers = layers;
    }
    let layer_ids: BTreeSet<String> = board.layers.iter().map(|layer| layer.id.clone()).collect();
    let copper_order: Vec<String> =
        board.layers.iter().filter(|layer| layer.kind == LayerKind::Copper).map(|layer| layer.id.clone()).collect();

    let mut net_map = BTreeMap::new();
    for entry in items.iter().filter(|item| tag(item) == Some("net")) {
        let Some(values) = body(entry)
        else {
            continue;
        };
        let Some(number) = values.first().and_then(text).and_then(|value| value.parse::<u32>().ok())
        else {
            continue;
        };
        let name = values.get(1).and_then(text).unwrap_or("");
        if number == 0 {
            if !name.trim().is_empty() {
                loss(&mut losses, "kicad/net-zero-named", format!("net 0 is named `{name}`; KiCad net 0 means unconnected"));
            }
            continue;
        }
        if name.trim().is_empty() {
            continue;
        }
        if net_map.contains_key(&number) || net_map.values().any(|existing| existing == name) {
            loss(&mut losses, "kicad/net-duplicate", format!("net {number} `{name}` duplicates an imported net"));
            continue;
        }
        net_map.insert(number, name.to_string());
    }

    let mut edges = Edges::default();
    let mut footprints = Vec::new();
    let mut tracks = Vec::new();
    let mut vias = Vec::new();
    let mut references = HashSet::new();
    let mut unsupported_outline = false;

    for item in items {
        let Some(kind) = tag(item)
        else {
            continue;
        };
        match kind {
            "version" | "generator" | "generator_version" | "general" | "paper" | "title_block" | "setup" | "layers"
            | "net" => {}
            "footprint" | "module" => {
                if let Some(footprint) = footprint(item, &layer_ids, &net_map, &mut losses) {
                    if references.insert(footprint.ref_.clone()) {
                        footprints.push(footprint);
                    }
                    else {
                        loss(
                            &mut losses,
                            "kicad/footprint-duplicate-reference",
                            format!("footprint `{}` repeats a reference and is skipped", footprint.ref_),
                        );
                    }
                }
            }
            "segment" => {
                if let Some(track) = track(item, &layer_ids, &net_map, &mut losses) {
                    tracks.push(track);
                }
            }
            "via" => {
                if let Some(via) = via(item, &copper_order, &net_map, &mut losses) {
                    vias.push(via);
                }
            }
            "gr_line" | "gr_rect" => {
                if layer_of(item) == "Edge.Cuts" {
                    edges.push(item, kind, &mut losses);
                }
                else {
                    loss(&mut losses, "kicad/construct-not-imported", format!("`{kind}` is not imported"));
                }
            }
            other => {
                if layer_of(item) == "Edge.Cuts" {
                    unsupported_outline = true;
                }
                loss(&mut losses, "kicad/construct-not-imported", format!("`{other}` is not imported"));
            }
        }
    }

    let outline = edges.outline(unsupported_outline, &mut losses);

    let nets: Vec<Net> = net_map
        .values()
        .enumerate()
        .map(|(index, name)| Net { id: format!("net-{}", index + 1), name: name.clone() })
        .collect();
    for (index, footprint) in footprints.iter_mut().enumerate() {
        footprint.id = format!("fp-{}", index + 1);
    }
    for (index, track) in tracks.iter_mut().enumerate() {
        track.id = format!("track-{}", index + 1);
    }
    for (index, via) in vias.iter_mut().enumerate() {
        via.id = format!("via-{}", index + 1);
    }

    board.revision = 1;
    board.units = Units::Mm;
    board.outline = outline;
    board.next_net = nets.len() as u64 + 1;
    board.next_footprint = footprints.len() as u64 + 1;
    board.next_track = tracks.len() as u64 + 1;
    board.next_via = vias.len() as u64 + 1;
    board.nets = nets;
    board.footprints = footprints;
    board.tracks = tracks;
    board.vias = vias;

    Ok(Imported { board, losses })
}

/// Reads the board title from `(title_block (title ...))`.
fn title_of(items: &[Node]) -> Option<String> {
    let block = child(items, "title_block")?;
    let title = child(body(block)?, "title")?;
    let value = body(title)?.first().and_then(text)?.trim();
    if value.is_empty() { None } else { Some(value.to_string()) }
}

/// Reads the copper layers, ordered by their KiCad layer number.
fn copper_layers(items: &[Node]) -> Vec<Layer> {
    let Some(node) = child(items, "layers")
    else {
        return Vec::new();
    };
    let Some(entries) = body(node)
    else {
        return Vec::new();
    };
    let mut found: Vec<(u32, String)> = Vec::new();
    for entry in entries {
        let Some(values) = list_items(entry)
        else {
            continue;
        };
        let Some(name) = values.get(1).and_then(text)
        else {
            continue;
        };
        if !name.ends_with(".Cu") || name.contains('*') {
            continue;
        }
        let number = values.first().and_then(text).and_then(|value| value.parse::<u32>().ok()).unwrap_or(u32::MAX);
        if !found.iter().any(|(_, existing)| existing == name) {
            found.push((number, name.to_string()));
        }
    }
    found.sort_by_key(|(number, _)| *number);
    found
        .into_iter()
        .map(|(_, name)| Layer {
            id: name.clone(),
            color: copper_color(&name).to_string(),
            kind: LayerKind::Copper,
            visible: true,
            locked: false,
            name,
        })
        .collect()
}

fn copper_color(name: &str) -> &'static str {
    match name {
        "F.Cu" => "#c87533",
        "B.Cu" => "#2f6fb0",
        _ => "#6f9e4a",
    }
}

/// Parses one placed footprint, reporting every field the model cannot hold.
fn footprint(
    node: &Node,
    layers: &BTreeSet<String>,
    nets: &BTreeMap<u32, String>,
    losses: &mut Vec<Loss>,
) -> Option<Footprint> {
    let values = body(node)?;
    let package = values.first().and_then(text).unwrap_or_default().to_string();
    let layer = layer_of(node);
    if !layers.contains(&layer) {
        loss(losses, "kicad/footprint-layer-missing", format!("footprint `{package}` is on `{layer}`, which is not imported"));
        return None;
    }
    let Some(at) = child(values, "at").and_then(point3)
    else {
        loss(losses, "kicad/footprint-invalid", format!("footprint `{package}` has no readable `(at ...)`"));
        return None;
    };
    let reference = property(values, "Reference").or_else(|| fp_text(values, "reference"));
    let Some(reference) = reference
    else {
        loss(losses, "kicad/footprint-without-reference", format!("footprint `{package}` has no reference and is skipped"));
        return None;
    };
    let value = property(values, "Value").or_else(|| fp_text(values, "value")).unwrap_or_default();
    let mut pads = Vec::new();
    let mut numbers = HashSet::new();

    for item in values {
        let Some(kind) = tag(item)
        else {
            continue;
        };
        match kind {
            "layer" | "at" | "uuid" | "tstamp" | "path" | "sheetfile" | "sheetname" | "tedit" | "pad" => {}
            "property" => {
                let key = body(item).and_then(|fields| fields.first()).and_then(text).unwrap_or("");
                if key != "Reference" && key != "Value" {
                    loss(
                        losses,
                        "kicad/footprint-property",
                        format!("footprint `{reference}`: property `{key}` is not stored"),
                    );
                }
            }
            "fp_text" => {
                let field = body(item).and_then(|fields| fields.first()).and_then(text).unwrap_or("");
                if !field.eq_ignore_ascii_case("reference") && !field.eq_ignore_ascii_case("value") {
                    loss(losses, "kicad/footprint-text", format!("footprint `{reference}`: `fp_text {field}` is not stored"));
                }
            }
            "model" => {
                loss(losses, "kicad/footprint-model", format!("footprint `{reference}`: the 3D model is not stored"));
            }
            "zone" => {
                loss(losses, "kicad/footprint-zone", format!("footprint `{reference}`: a footprint zone is not stored"));
            }
            "attr" => {
                loss(losses, "kicad/footprint-attr", format!("footprint `{reference}`: the `attr` flags are not stored"));
            }
            "fp_line" | "fp_arc" | "fp_rect" | "fp_circle" | "fp_poly" | "fp_curve" => {
                loss(losses, "kicad/footprint-graphics", format!("footprint `{reference}`: `{kind}` graphics are not stored"));
            }
            "group"
            | "dimension"
            | "autoplace_cost90"
            | "autoplace_cost180"
            | "solder_mask_margin"
            | "solder_paste_ratio"
            | "solder_paste_margin"
            | "solder_paste_margin_ratio"
            | "clearance"
            | "courtyard"
            | "private_layers"
            | "net_tie_pad_groups" => {
                loss(losses, "kicad/footprint-field-not-modeled", format!("footprint `{reference}`: `{kind}` is not stored"));
            }
            _ => {
                loss(losses, "kicad/footprint-field-not-modeled", format!("footprint `{reference}`: `{kind}` is not stored"));
            }
        }
    }

    for item in values.iter().filter(|item| tag(item) == Some("pad")) {
        if let Some(pad) = pad(item, &reference, &layer, nets, losses) {
            if numbers.insert(pad.number.clone()) {
                pads.push(pad);
            }
            else {
                loss(losses, "kicad/pad-number-duplicate", format!("footprint `{reference}`: pad `{}` repeats", pad.number));
            }
        }
    }

    Some(Footprint {
        id: String::new(),
        ref_: reference,
        value,
        footprint: package,
        x: at[0],
        y: at[1],
        rotation: at[2],
        layer,
        pads,
    })
}

/// Parses one pad, keeping only number, position, size and net.
fn pad(
    node: &Node,
    reference: &str,
    footprint_layer: &str,
    nets: &BTreeMap<u32, String>,
    losses: &mut Vec<Loss>,
) -> Option<Pad> {
    let values = body(node)?;
    let number = values.first().and_then(text)?.to_string();
    if number.trim().is_empty() {
        loss(losses, "kicad/pad-invalid", format!("footprint `{reference}`: a pad has no number"));
        return None;
    }
    let kind = values.get(1).and_then(text).unwrap_or("");
    let shape = values.get(2).and_then(text).unwrap_or("");
    let Some(at) = child(values, "at").and_then(point3)
    else {
        loss(losses, "kicad/pad-invalid", format!("footprint `{reference}`: pad `{number}` has no readable `(at ...)`"));
        return None;
    };
    let Some(size) = child(values, "size").and_then(size_pair)
    else {
        loss(losses, "kicad/pad-size-invalid", format!("footprint `{reference}`: pad `{number}` has no readable `(size ...)`"));
        return None;
    };
    let (w, h) = (size[0], if size[1] > 0.0 { size[1] } else { size[0] });
    if w <= 0.0 || h <= 0.0 {
        loss(losses, "kicad/pad-size-invalid", format!("footprint `{reference}`: pad `{number}` has a non-positive size"));
        return None;
    }
    if shape != "rect" {
        loss(
            losses,
            "kicad/pad-shape",
            format!("footprint `{reference}`: pad `{number}` shape `{shape}` is stored as a rectangle"),
        );
    }
    if kind != "smd" {
        loss(losses, "kicad/pad-type", format!("footprint `{reference}`: pad `{number}` type `{kind}` is not stored"));
    }
    if child(values, "drill").is_some() {
        loss(losses, "kicad/pad-drill", format!("footprint `{reference}`: pad `{number}` drill is not stored"));
    }
    if at[2].abs() > 0.000_001 {
        loss(losses, "kicad/pad-rotation", format!("footprint `{reference}`: pad `{number}` rotation is not stored"));
    }
    if let Some(declared) = child(values, "layers") {
        for layer in text_values(declared) {
            if layer.ends_with(".Cu") && !layer.contains('*') && layer != footprint_layer {
                loss(
                    losses,
                    "kicad/pad-layer",
                    format!("footprint `{reference}`: pad `{number}` copper layer `{layer}` is not stored"),
                );
            }
        }
    }
    let net = match child(values, "net").and_then(net_number) {
        None | Some(0) => None,
        Some(number) => match nets.get(&number) {
            Some(name) => Some(name.clone()),
            None => {
                loss(
                    losses,
                    "kicad/pad-net-unknown",
                    format!("footprint `{reference}`: pad `{number}` uses undeclared net {number}"),
                );
                None
            }
        },
    };
    for item in values {
        let Some(field) = tag(item)
        else {
            continue;
        };
        if matches!(field, "at" | "size" | "layers" | "net" | "drill") {
            continue;
        }
        loss(
            losses,
            "kicad/pad-field-not-modeled",
            format!("footprint `{reference}`: pad `{number}` field `{field}` is not stored"),
        );
    }

    Some(Pad { number, x: at[0], y: at[1], w, h, net })
}

/// Parses one routed segment.
fn track(node: &Node, layers: &BTreeSet<String>, nets: &BTreeMap<u32, String>, losses: &mut Vec<Loss>) -> Option<Track> {
    let values = body(node)?;
    let layer = layer_of(node);
    if !layers.contains(&layer) {
        loss(
            losses,
            "kicad/track-layer-missing",
            format!("a segment on `{layer}` is skipped because the layer is not imported"),
        );
        return None;
    }
    let Some(from) = child(values, "start").and_then(point2)
    else {
        loss(losses, "kicad/track-invalid", "a segment has no readable `(start ...)`".to_string());
        return None;
    };
    let Some(to) = child(values, "end").and_then(point2)
    else {
        loss(losses, "kicad/track-invalid", "a segment has no readable `(end ...)`".to_string());
        return None;
    };
    let width = child(values, "width").and_then(point1).unwrap_or(0.0);
    if width <= 0.0 {
        loss(losses, "kicad/track-invalid", "a segment has a non-positive width".to_string());
        return None;
    }
    let net = match child(values, "net").and_then(net_number) {
        None | Some(0) => None,
        Some(number) => match nets.get(&number) {
            Some(name) => Some(name.clone()),
            None => {
                loss(losses, "kicad/track-net-unknown", format!("a segment uses undeclared net {number}"));
                None
            }
        },
    };
    Some(Track { id: String::new(), net, layer, width, from, to, teardrop: false })
}

/// Parses one through, blind, buried or micro via; unknown types are refused.
///
/// The layer pair is read from `(layers ...)`: exactly two distinct copper
/// layers are required, and they are reordered by stack position. An outer pair
/// imports as a through via, any other pair keeps its endpoint ids. A `micro`
/// type keeps its type only when the pair is adjacent and touches an outer
/// layer; any other micro span is reported instead of approximated.
fn via(node: &Node, copper: &[String], nets: &BTreeMap<u32, String>, losses: &mut Vec<Loss>) -> Option<Via> {
    let values = body(node)?;
    let Some(at) = child(values, "at").and_then(point2)
    else {
        loss(losses, "kicad/via-invalid", "a via has no readable `(at ...)`".to_string());
        return None;
    };
    let diameter = child(values, "size").and_then(point1).unwrap_or(0.0);
    let drill = child(values, "drill").and_then(point1).unwrap_or(0.0);
    if diameter <= 0.0 || drill <= 0.0 {
        loss(losses, "kicad/via-invalid", "a via has a non-positive size or drill".to_string());
        return None;
    }
    let micro = match child(values, "type").and_then(|node| body(node)?.first()).and_then(text) {
        None | Some("through") | Some("blind") | Some("buried") => false,
        Some("micro") => true,
        Some(kind) => {
            loss(
                losses,
                "kicad/via-type",
                format!("via type `{kind}` is not stored; only through, blind, buried and micro vias are imported"),
            );
            return None;
        }
    };
    let mut span: Vec<String> = Vec::new();
    for layer in child(values, "layers").map(text_values).unwrap_or_default() {
        if !span.contains(&layer) {
            span.push(layer);
        }
    }
    if span.len() != 2 {
        loss(losses, "kicad/via-layer-span", "a via must name exactly two distinct copper layers".to_string());
        return None;
    }
    let (Some(first), Some(second)) =
        (copper.iter().position(|id| id == &span[0]), copper.iter().position(|id| id == &span[1]))
    else {
        loss(losses, "kicad/via-layer-span", "a via names a layer that is not imported copper".to_string());
        return None;
    };
    let (low, high) = if first < second { (first, second) } else { (second, first) };
    if micro && !(high == low + 1 && (low == 0 || high + 1 == copper.len())) {
        loss(
            losses,
            "kicad/via-layer-span",
            "a micro via must span two adjacent copper layers with one outer layer".to_string(),
        );
        return None;
    }
    let (from, to) = (copper[low].clone(), copper[high].clone());
    let (from_layer, to_layer) = if from == "F.Cu" && to == "B.Cu" { (None, None) } else { (Some(from), Some(to)) };
    let net = match child(values, "net").and_then(net_number) {
        None | Some(0) => None,
        Some(number) => match nets.get(&number) {
            Some(name) => Some(name.clone()),
            None => {
                loss(losses, "kicad/via-net-unknown", format!("a via uses undeclared net {number}"));
                None
            }
        },
    };
    Some(Via { id: String::new(), net, at, diameter, drill, from_layer, to_layer, micro, back_drill: None })
}

/// Edge.Cuts geometry collected before the outline is decided.
#[derive(Default)]
struct Edges {
    lines: Vec<([f64; 2], [f64; 2])>,
    rects: Vec<([f64; 2], [f64; 2])>,
    broken: bool,
}

impl Edges {
    fn push(&mut self, node: &Node, kind: &str, losses: &mut Vec<Loss>) {
        let Some(values) = body(node)
        else {
            self.broken = true;
            return;
        };
        let (Some(start), Some(end)) = (child(values, "start").and_then(point2), child(values, "end").and_then(point2))
        else {
            self.broken = true;
            loss(losses, "kicad/outline-not-rectangular", format!("`{kind}` on Edge.Cuts has no readable start and end"));
            return;
        };
        if kind == "gr_rect" {
            self.rects.push((start, end));
        }
        else {
            self.lines.push((start, end));
        }
    }

    fn outline(&self, unsupported: bool, losses: &mut Vec<Loss>) -> Option<Outline> {
        if self.broken || unsupported {
            loss(losses, "kicad/outline-not-rectangular", "the Edge.Cuts geometry is not a single rectangle".to_string());
            return None;
        }
        if !self.rects.is_empty() {
            if self.rects.len() != 1 || !self.lines.is_empty() {
                loss(losses, "kicad/outline-not-rectangular", "the Edge.Cuts geometry is not a single rectangle".to_string());
                return None;
            }
            let (start, end) = self.rects[0];
            return rectangle(start, end, losses);
        }
        if self.lines.is_empty() {
            return None;
        }
        rectangle_from_lines(&self.lines, losses)
    }
}

/// Accepts one `gr_rect` as the board outline after normalising its corners.
fn rectangle(start: [f64; 2], end: [f64; 2], losses: &mut Vec<Loss>) -> Option<Outline> {
    let from = [start[0].min(end[0]), start[1].min(end[1])];
    let to = [start[0].max(end[0]), start[1].max(end[1])];
    if to[0] - from[0] <= 0.000_001 || to[1] - from[1] <= 0.000_001 {
        loss(losses, "kicad/outline-not-rectangular", "the Edge.Cuts rectangle has no area".to_string());
        return None;
    }
    Some(Outline { from, to })
}

/// Accepts four axis-aligned `gr_line` edges only when they close one rectangle.
fn rectangle_from_lines(lines: &[([f64; 2], [f64; 2])], losses: &mut Vec<Loss>) -> Option<Outline> {
    const EPSILON: f64 = 0.000_001;
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    let mut total = 0.0;
    for (start, end) in lines {
        let axis_aligned = (start[0] - end[0]).abs() <= EPSILON || (start[1] - end[1]).abs() <= EPSILON;
        if !axis_aligned {
            loss(losses, "kicad/outline-not-rectangular", "an Edge.Cuts line is not axis-aligned".to_string());
            return None;
        }
        for point in [start, end] {
            min[0] = min[0].min(point[0]);
            min[1] = min[1].min(point[1]);
            max[0] = max[0].max(point[0]);
            max[1] = max[1].max(point[1]);
        }
        total += (start[0] - end[0]).abs() + (start[1] - end[1]).abs();
    }
    let width = max[0] - min[0];
    let height = max[1] - min[1];
    if width <= EPSILON || height <= EPSILON {
        loss(losses, "kicad/outline-not-rectangular", "the Edge.Cuts lines have no area".to_string());
        return None;
    }
    let perimeter = 2.0 * (width + height);
    if (total - perimeter).abs() > EPSILON {
        loss(losses, "kicad/outline-not-rectangular", "the Edge.Cuts lines do not close exactly one rectangle".to_string());
        return None;
    }
    for (start, end) in lines {
        for point in [start, end] {
            if point[0] < min[0] - EPSILON
                || point[0] > max[0] + EPSILON
                || point[1] < min[1] - EPSILON
                || point[1] > max[1] + EPSILON
            {
                loss(losses, "kicad/outline-not-rectangular", "an Edge.Cuts line leaves the bounding rectangle".to_string());
                return None;
            }
        }
        let horizontal = (start[1] - end[1]).abs() <= EPSILON;
        let on_boundary = if horizontal {
            (start[1] - min[1]).abs() <= EPSILON || (start[1] - max[1]).abs() <= EPSILON
        }
        else {
            (start[0] - min[0]).abs() <= EPSILON || (start[0] - max[0]).abs() <= EPSILON
        };
        if !on_boundary {
            loss(losses, "kicad/outline-not-rectangular", "an Edge.Cuts line is not on the rectangle border".to_string());
            return None;
        }
    }
    Some(Outline { from: min, to: max })
}

fn loss(losses: &mut Vec<Loss>, code: &'static str, message: impl Into<String>) {
    losses.push(Loss { code, message: message.into() });
}

fn property(items: &[Node], key: &str) -> Option<String> {
    items.iter().filter(|item| tag(item) == Some("property")).find_map(|item| {
        let fields = body(item)?;
        if fields.first().and_then(text) != Some(key) {
            return None;
        }
        fields.get(1).and_then(text).map(str::to_string)
    })
}

fn fp_text(items: &[Node], field: &str) -> Option<String> {
    items.iter().filter(|item| tag(item) == Some("fp_text")).find_map(|item| {
        let fields = body(item)?;
        if !fields.first().and_then(text)?.eq_ignore_ascii_case(field) {
            return None;
        }
        fields.get(1).and_then(text).map(str::to_string)
    })
}

fn layer_of(node: &Node) -> String {
    child(body(node).unwrap_or_default(), "layer")
        .and_then(|node| body(node)?.first())
        .and_then(text)
        .unwrap_or_default()
        .to_string()
}

fn net_number(node: &Node) -> Option<u32> {
    let value = body(node)?.first().and_then(text)?.parse::<f64>().ok()?;
    if value >= 0.0 && value.fract() == 0.0 { Some(value as u32) } else { None }
}

fn point1(node: &Node) -> Option<f64> {
    numbers(node).first().copied()
}

fn point2(node: &Node) -> Option<[f64; 2]> {
    let values = numbers(node);
    Some([*values.first()?, *values.get(1)?])
}

fn size_pair(node: &Node) -> Option<[f64; 2]> {
    let values = numbers(node);
    let first = *values.first()?;
    Some([first, values.get(1).copied().unwrap_or(first)])
}

fn point3(node: &Node) -> Option<[f64; 3]> {
    let values = numbers(node);
    Some([*values.first()?, *values.get(1)?, values.get(2).copied().unwrap_or(0.0)])
}

fn numbers(node: &Node) -> Vec<f64> {
    body(node)
        .map(|items| {
            items
                .iter()
                .filter_map(text)
                .filter_map(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite())
                .collect()
        })
        .unwrap_or_default()
}

fn text_values(node: &Node) -> Vec<String> {
    body(node).map(|items| items.iter().filter_map(text).map(str::to_string).collect()).unwrap_or_default()
}

/// Returns the keyword of a list. Quoted and numeric first elements are values,
/// not keywords, so a pad `("1" smd rect ...)` has no tag.
fn tag(node: &Node) -> Option<&str> {
    match node {
        Node::List(items) => match items.first()? {
            Node::Atom(value) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

fn list_items(node: &Node) -> Option<&[Node]> {
    match node {
        Node::List(items) => Some(items),
        _ => None,
    }
}

fn body(node: &Node) -> Option<&[Node]> {
    match node {
        Node::List(items) => items.get(1..),
        _ => None,
    }
}

fn text(node: &Node) -> Option<&str> {
    match node {
        Node::Atom(value) | Node::Str(value) => Some(value),
        Node::List(_) => None,
    }
}

fn child<'a>(items: &'a [Node], name: &str) -> Option<&'a Node> {
    items.iter().find(|item| tag(item) == Some(name))
}

/// One S-expression node.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    Atom(String),
    Str(String),
    List(Vec<Node>),
}

/// Recursive-descent reader for the KiCad S-expression dialect.
struct Parser<'a> {
    input: &'a [u8],
    index: usize,
}

impl<'a> Parser<'a> {
    fn run(text: &'a str) -> Result<Node, ServiceError> {
        let mut parser = Parser { input: text.as_bytes(), index: 0 };
        parser.skip();
        if parser.index == parser.input.len() {
            return Err(crate::invalid("the KiCad file is empty"));
        }
        let node = parser.node()?;
        parser.skip();
        if parser.index != parser.input.len() {
            return Err(crate::invalid(format!("unexpected content at byte {}", parser.index)));
        }
        Ok(node)
    }

    fn node(&mut self) -> Result<Node, ServiceError> {
        match self.input[self.index] {
            b'(' => self.list(),
            b')' => Err(crate::invalid(format!("unexpected `)` at byte {}", self.index))),
            b'"' => Ok(Node::Str(self.string()?)),
            _ => self.atom(),
        }
    }

    fn list(&mut self) -> Result<Node, ServiceError> {
        let start = self.index;
        self.index += 1;
        let mut items = Vec::new();
        loop {
            self.skip();
            if self.index >= self.input.len() {
                return Err(crate::invalid(format!("unterminated list starting at byte {start}")));
            }
            if self.input[self.index] == b')' {
                self.index += 1;
                return Ok(Node::List(items));
            }
            items.push(self.node()?);
        }
    }

    fn string(&mut self) -> Result<String, ServiceError> {
        let start = self.index;
        self.index += 1;
        let mut out = String::new();
        loop {
            if self.index >= self.input.len() {
                return Err(crate::invalid(format!("unterminated string starting at byte {start}")));
            }
            let character = self.next_char();
            match character {
                '"' => return Ok(out),
                '\\' => {
                    if self.index >= self.input.len() {
                        return Err(crate::invalid(format!("unterminated escape starting at byte {start}")));
                    }
                    let escaped = self.next_char();
                    match escaped {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        '0' => out.push('\0'),
                        '\\' => out.push('\\'),
                        '"' => out.push('"'),
                        'u' => out.push(self.unicode_escape()?),
                        other => out.push(other),
                    }
                }
                other => out.push(other),
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, ServiceError> {
        if self.input.get(self.index) == Some(&b'{') {
            self.index += 1;
            let start = self.index;
            while self.index < self.input.len() && self.input[self.index] != b'}' {
                self.index += 1;
            }
            if self.index >= self.input.len() {
                return Err(crate::invalid("unterminated `\\u{...}` escape"));
            }
            let digits = std::str::from_utf8(&self.input[start..self.index]).unwrap_or("");
            self.index += 1;
            return u32::from_str_radix(digits, 16)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| crate::invalid("invalid `\\u{...}` escape"));
        }
        if self.index + 4 > self.input.len() {
            return Err(crate::invalid("truncated `\\uXXXX` escape"));
        }
        let digits = std::str::from_utf8(&self.input[self.index..self.index + 4]).unwrap_or("");
        self.index += 4;
        u32::from_str_radix(digits, 16).ok().and_then(char::from_u32).ok_or_else(|| crate::invalid("invalid `\\uXXXX` escape"))
    }

    fn atom(&mut self) -> Result<Node, ServiceError> {
        let start = self.index;
        while self.index < self.input.len() {
            match self.input[self.index] {
                b'(' | b')' | b'"' | b' ' | b'\t' | b'\r' | b'\n' | 0x0c => break,
                _ => self.index += 1,
            }
        }
        if self.index == start {
            return Err(crate::invalid(format!("unexpected byte at {}", self.index)));
        }
        let value = std::str::from_utf8(&self.input[start..self.index])
            .map_err(|_| crate::invalid(format!("invalid UTF-8 at byte {start}")))?
            .to_string();
        Ok(Node::Atom(value))
    }

    fn next_char(&mut self) -> char {
        let rest = std::str::from_utf8(&self.input[self.index..]).unwrap_or("");
        let character = rest.chars().next().unwrap_or('\u{fffd}');
        self.index += character.len_utf8();
        character
    }

    fn skip(&mut self) {
        while self.index < self.input.len() && matches!(self.input[self.index], b' ' | b'\t' | b'\r' | b'\n' | 0x0c) {
            self.index += 1;
        }
    }
}
