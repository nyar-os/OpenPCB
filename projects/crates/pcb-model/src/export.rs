//! Fabrication, assembly and preview writers.
//!
//! Only targets listed as `supported` in the capability matrix reach a writer;
//! everything else returns `capability/not-implemented` instead of an empty
//! file. Every writer is atomic: the file appears only after the full content
//! was written and renamed into place.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

use document_drawing::{Color, Drawing, Element, Point as DrawPoint, Shape, Size, Style};
use serde_json::{Value, json};

use crate::{ServiceError, board::Board};

/// Formats this build can write.
pub(crate) const SUPPORTED_EXPORTS: &[&str] = &[
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
];

/// Points per millimetre used by the vector PDF writer.
const PT_PER_MM: f64 = 2.834646;

/// Width and height of one A4 page in landscape orientation, in points.
const A4_LANDSCAPE_PT: [f64; 2] = [841.89, 595.28];

/// Margin kept around the board inside a PDF page, in points (10 mm).
const PDF_MARGIN_PT: f64 = 28.34646;

/// Preview renderers this build can produce without writing a file.
pub(crate) const SUPPORTED_PREVIEWS: &[&str] = &["svg"];

/// Gap between a footprint's pad bounding box and its silkscreen outline.
const SILK_MARGIN: f64 = 0.8;

/// Cap height of a footprint reference designator, in millimetres.
const REFERENCE_SIZE: f64 = 1.6;

/// Writes one export target and returns its report.
pub(crate) fn run(board: &Board, format: &str, path: &Path) -> Result<Value, ServiceError> {
    if format == "gbr" {
        return gerber(board, path);
    }
    if format == "png" {
        return png(board, path);
    }
    let content = match format {
        "drl" => drill(board).into_bytes(),
        "drill_map" => drill_map(board).into_bytes(),
        "bom" => bom(board).into_bytes(),
        "pos" => position(board).into_bytes(),
        "testpoints" => testpoints(board).into_bytes(),
        "ipc_d_356" => ipc_d_356(board).into_bytes(),
        "placement" => placement(board)?.into_bytes(),
        "svg" => svg(board)?.into_bytes(),
        "pdf" => pdf(board)?,
        "kicad_pcb" => crate::kicad_pcb::write(board).into_bytes(),
        "ipc2581" => crate::ipc2581::write(board).into_bytes(),
        other => {
            return Err(ServiceError::new(
                "capability/not-implemented",
                format!("`{other}` export is declared but not implemented"),
            ));
        }
    };
    let bytes = crate::atomic_write(path, &content)?;
    Ok(json!({
        "revision": board.revision,
        "format": format,
        "path": path.display().to_string(),
        "bytes": bytes,
        "editable": false,
    }))
}

/// Writes a Gerber RS-274X directory and reports each artifact.
fn gerber(board: &Board, path: &Path) -> Result<Value, ServiceError> {
    let outline = board
        .outline
        .ok_or_else(|| crate::invalid("the board has no outline; set one with `board.outline set` before exporting Gerber"))?;
    if path.exists() && !path.is_dir() {
        return Err(crate::invalid(format!(
            "`{}` exists and is not a directory; Gerber export writes a directory of layer files",
            path.display()
        )));
    }
    if !path.is_dir() {
        fs::create_dir_all(path).map_err(|error| crate::invalid(format!("cannot create `{}`: {error}", path.display())))?;
    }

    let mut files = board.copper_stack().into_iter().map(|id| (gerber_name(id), copper(board, id))).collect::<Vec<_>>();
    files.push(("F_SilkS.gbr".to_string(), silkscreen(board, "F.SilkS")));
    files.push(("B_SilkS.gbr".to_string(), silkscreen(board, "B.SilkS")));
    files.push(("F_Mask.gbr".to_string(), solder_mask(board, "F.Cu")));
    files.push(("B_Mask.gbr".to_string(), solder_mask(board, "B.Cu")));
    files.push(("F_Paste.gbr".to_string(), solder_paste(board, "F.Cu")));
    files.push(("B_Paste.gbr".to_string(), solder_paste(board, "B.Cu")));
    files.push(("Edge_Cuts.gbr".to_string(), edge_cuts(outline.from, outline.to, board.millimetres())));
    let mut artifacts = Vec::new();
    let mut total = 0u64;
    for (name, content) in files {
        let bytes = crate::atomic_write(&path.join(&name), content.as_bytes())?;
        total += bytes;
        artifacts.push(json!({ "name": name, "bytes": bytes }));
    }
    Ok(json!({
        "revision": board.revision,
        "format": "gbr",
        "path": path.display().to_string(),
        "bytes": total,
        "editable": false,
        "artifacts": artifacts,
    }))
}

/// Accumulates apertures and draws for one Gerber file.
#[derive(Default)]
struct Gerber {
    apertures: Vec<String>,
    body: String,
}

impl Gerber {
    /// Returns the D-code of an aperture definition, adding it on first use.
    fn aperture(&mut self, definition: String) -> u32 {
        if let Some(index) = self.apertures.iter().position(|item| *item == definition) {
            return 10 + index as u32;
        }
        self.apertures.push(definition);
        10 + (self.apertures.len() - 1) as u32
    }

    fn draw(&mut self, definition: String, from: [f64; 2], to: [f64; 2]) {
        self.draw_with(definition, from, to, &[]);
    }

    /// Draws one segment with optional X2 object attributes.
    fn draw_with(&mut self, definition: String, from: [f64; 2], to: [f64; 2], attributes: &[String]) {
        let code = self.aperture(definition);
        for attribute in attributes {
            self.body.push_str(attribute);
        }
        let _ = writeln!(
            self.body,
            "D{code}*\nX{}Y{}D02*\nX{}Y{}D01*",
            gerber_number(from[0]),
            gerber_number(from[1]),
            gerber_number(to[0]),
            gerber_number(to[1])
        );
        self.close_attributes(attributes);
    }

    fn flash(&mut self, definition: String, at: [f64; 2]) {
        self.flash_with(definition, at, &[]);
    }

    /// Flashes one aperture with optional X2 object attributes.
    fn flash_with(&mut self, definition: String, at: [f64; 2], attributes: &[String]) {
        let code = self.aperture(definition);
        for attribute in attributes {
            self.body.push_str(attribute);
        }
        let _ = writeln!(self.body, "D{code}*\nX{}Y{}D03*", gerber_number(at[0]), gerber_number(at[1]));
        self.close_attributes(attributes);
    }

    /// Ends an object attribute block; Gerber X2 requires it after the object.
    fn close_attributes(&mut self, attributes: &[String]) {
        if !attributes.is_empty() {
            self.body.push_str("%TD*%\n");
        }
    }

    /// Writes one closed polygon as a `G36`/`G37` region.
    ///
    /// The contour starts with a `D02` move and continues with `D01` draws; the
    /// first vertex is repeated to close the polygon. A small circular aperture
    /// is selected first so readers that expect a current aperture still parse
    /// the region.
    fn region(&mut self, points: &[[f64; 2]], scale: f64) {
        if points.len() < 3 {
            return;
        }
        let code = self.aperture("C,0.100000".to_string());
        let scaled: Vec<[f64; 2]> = points.iter().map(|point| [point[0] * scale, point[1] * scale]).collect();
        let _ = writeln!(self.body, "D{code}*\nG36*");
        for (index, point) in scaled.iter().enumerate() {
            let operation = if index == 0 { "D02" } else { "D01" };
            let _ = writeln!(self.body, "X{}Y{}{operation}*", gerber_number(point[0]), gerber_number(point[1]));
        }
        let first = scaled[0];
        let _ = writeln!(self.body, "X{}Y{}D01*\nG37*", gerber_number(first[0]), gerber_number(first[1]));
    }

    /// Renders the file with Gerber X2 file, aperture and object attributes.
    ///
    /// The header declares the generation software, the shared coordinate
    /// origin, the file function, positive polarity and a single part. The
    /// aperture block carries one `.AperFunction` attribute derived from the
    /// file function, then resets it with `%TD*%` before the draws. Copper
    /// objects carry their own `%TO` block, closed with `%TD*%` after the object.
    fn render(&self, file_function: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "%TF.GenerationSoftware,OpenPCB,open-pcb,{}*%", env!("CARGO_PKG_VERSION"));
        out.push_str("%TF.SameCoordinates,Original*%\n");
        let _ = writeln!(out, "%TF.FileFunction,{file_function}*%");
        out.push_str("%TF.FilePolarity,Positive*%\n%TF.Part,Single*%\n%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG01*\n");
        if !self.apertures.is_empty() {
            let _ = writeln!(out, "%TA.AperFunction,{}*%", aperture_function(file_function));
            for (index, definition) in self.apertures.iter().enumerate() {
                let _ = writeln!(out, "%ADD{}{definition}*%", 10 + index);
            }
            out.push_str("%TD*%\n");
        }
        out.push_str(&self.body);
        out.push_str("M02*\n");
        out
    }
}

/// Maps an X2 file function to the aperture function of its objects.
fn aperture_function(file_function: &str) -> &'static str {
    if file_function.starts_with("Copper") {
        "Conductor"
    }
    else if file_function.starts_with("Legend") {
        "Legend"
    }
    else if file_function.starts_with("Soldermask") {
        "Soldermask"
    }
    else if file_function.starts_with("Paste") {
        "Paste"
    }
    else if file_function.starts_with("Profile") {
        "Profile"
    }
    else {
        "Other"
    }
}

/// One Gerber file name for a layer id, with `.` replaced by `_`.
fn gerber_name(layer: &str) -> String {
    format!("{}.gbr", layer.replace('.', "_"))
}

/// Builds one `%TO.<key>,<value>*%` line, or nothing when the value is empty.
fn object_attribute(key: &str, value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    Some(format!("%TO.{key},{}*%\n", attribute_value(value)))
}

/// Replaces characters the Gerber attribute grammar reserves as separators.
fn attribute_value(value: &str) -> String {
    value.chars().map(|character| if matches!(character, ',' | '*' | '%' | '\r' | '\n') { '_' } else { character }).collect()
}

/// Net attribute of a track, via or pad; empty when the object has no net.
fn net_attributes(net: Option<&str>) -> Vec<String> {
    net.and_then(|net| object_attribute("N", net)).into_iter().collect()
}

/// Net and component attributes of a pad.
///
/// The component reference and the pin number are written separately: `.C`
/// carries the reference, `.P` carries the reference and the pin. A pad without
/// a number keeps only the reference.
fn pad_attributes(net: Option<&str>, reference: &str, pin: &str) -> Vec<String> {
    let mut attributes = net_attributes(net);
    if let Some(attribute) = object_attribute("C", reference) {
        attributes.push(attribute);
    }
    let reference = attribute_value(reference);
    let pin = attribute_value(pin);
    if !reference.is_empty() && !pin.is_empty() {
        attributes.push(format!("%TO.P,{reference},{pin}*%\n"));
    }
    attributes
}

/// Writes one copper layer: tracks as draws, pads and covered vias as flashes.
///
/// Each object carries its X2 attributes: tracks and vias declare their net, and
/// a pad declares its net, component reference and pin. The block is closed with
/// `%TD*%` after the object so attributes never leak into the next one.
fn copper(board: &Board, layer: &str) -> String {
    let scale = board.millimetres();
    let mut gerber = Gerber::default();
    for track in board.tracks.iter().filter(|track| track.layer == layer) {
        gerber.draw_with(
            format!("C,{:.6}", track.width * scale),
            [track.from[0] * scale, track.from[1] * scale],
            [track.to[0] * scale, track.to[1] * scale],
            &net_attributes(track.net.as_deref()),
        );
    }
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer == layer) {
        for pad in &footprint.pads {
            gerber.flash_with(
                format!("R,{:.6}X{:.6}", pad.w * scale, pad.h * scale),
                [(footprint.x + pad.x) * scale, (footprint.y + pad.y) * scale],
                &pad_attributes(pad.net.as_deref(), &footprint.ref_, &pad.number),
            );
        }
    }
    for via in board.vias.iter().filter(|via| board.via_covers(via, layer)) {
        gerber.flash_with(
            format!("C,{:.6}", via.diameter * scale),
            [via.at[0] * scale, via.at[1] * scale],
            &net_attributes(via.net.as_deref()),
        );
    }
    for zone in board.zones.iter().filter(|zone| zone.layer == layer) {
        gerber.region(&zone.points, scale);
    }
    let stack = board.copper_stack();
    let index = stack.iter().position(|id| *id == layer).unwrap_or(0);
    let position = if index == 0 {
        "Top"
    }
    else if index + 1 == stack.len() {
        "Bot"
    }
    else {
        "Inr"
    };
    gerber.render(&format!("Copper,L{},{}", index + 1, position))
}

/// Writes one silkscreen layer: the document stores legend geometry as tracks
/// on the same layer ids the copper stack uses.
fn silkscreen(board: &Board, layer: &str) -> String {
    let scale = board.millimetres();
    let mut gerber = Gerber::default();
    for track in board.tracks.iter().filter(|track| track.layer == layer) {
        gerber.draw(
            format!("C,{:.6}", track.width * scale),
            [track.from[0] * scale, track.from[1] * scale],
            [track.to[0] * scale, track.to[1] * scale],
        );
    }
    let position = if layer.starts_with("B.") { "Bot" } else { "Top" };
    gerber.render(&format!("Legend,{position}"))
}

/// Writes one solder-mask layer: every pad on the matching copper side is a
/// rectangular opening expanded by the document's solder-mask rule per side.
/// Vias stay tented, so they contribute no opening.
fn solder_mask(board: &Board, layer: &str) -> String {
    let scale = board.millimetres();
    let expansion = board.rules.solder_mask_expansion * 2.0;
    let mut gerber = Gerber::default();
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer == layer) {
        for pad in &footprint.pads {
            gerber.flash(
                format!("R,{:.6}X{:.6}", pad.w * scale + expansion, pad.h * scale + expansion),
                [(footprint.x + pad.x) * scale, (footprint.y + pad.y) * scale],
            );
        }
    }
    let position = if layer.starts_with("B.") { "Bot" } else { "Top" };
    gerber.render(&format!("Soldermask,{position}"))
}

/// Writes one solder-paste layer: every outer-layer pad opens a stencil window
/// sized exactly to the copper pad. Vias stay tented and contribute no paste.
fn solder_paste(board: &Board, layer: &str) -> String {
    let scale = board.millimetres();
    let mut gerber = Gerber::default();
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer == layer) {
        for pad in &footprint.pads {
            gerber.flash(
                format!("R,{:.6}X{:.6}", pad.w * scale, pad.h * scale),
                [(footprint.x + pad.x) * scale, (footprint.y + pad.y) * scale],
            );
        }
    }
    let position = if layer.starts_with("B.") { "Bot" } else { "Top" };
    gerber.render(&format!("Paste,{position}"))
}

/// Writes the rectangular Edge.Cuts outline as four draws.
fn edge_cuts(from: [f64; 2], to: [f64; 2], scale: f64) -> String {
    let mut gerber = Gerber::default();
    let lower_left = [from[0] * scale, from[1] * scale];
    let lower_right = [to[0] * scale, from[1] * scale];
    let upper_right = [to[0] * scale, to[1] * scale];
    let upper_left = [from[0] * scale, to[1] * scale];
    for (start, end) in
        [(lower_left, lower_right), (lower_right, upper_right), (upper_right, upper_left), (upper_left, lower_left)]
    {
        gerber.draw("C,0.100000".to_string(), start, end);
    }
    gerber.render("Profile,NP")
}

/// Formats millimetres in the 4.6 coordinate format used by every file.
fn gerber_number(value: f64) -> i64 {
    let scaled = (value * 1_000_000.0).round();
    if scaled.is_finite() { scaled as i64 } else { 0 }
}

/// Hits that share one Excellon tool diameter.
#[derive(Default)]
struct DrillTool<'a> {
    vias: Vec<&'a crate::board::Via>,
    slots: Vec<&'a crate::board::Slot>,
    pads: Vec<[f64; 2]>,
}

/// Excellon drill file with one tool per distinct finished hole or slot width.
fn drill(board: &Board) -> String {
    let scale = board.millimetres();
    let mut tools: BTreeMap<String, DrillTool<'_>> = BTreeMap::new();
    for via in &board.vias {
        tools.entry(number(via.drill * scale)).or_default().vias.push(via);
    }
    for slot in &board.slots {
        tools.entry(number(slot.width * scale)).or_default().slots.push(slot);
    }
    for pad in board.placed_pads() {
        if let Some(drill) = pad.drill {
            tools.entry(number(drill * scale)).or_default().pads.push(pad.at);
        }
    }
    let mut out = String::from("M48\n;OpenPCB drill file\n;Units: mm\nMETRIC,TZ\n");
    for (index, (size, _)) in tools.iter().enumerate() {
        let _ = writeln!(out, "T{}C{size}", index + 1);
    }
    out.push_str("%\nG90\nG05\n");
    for (index, tool) in tools.values().enumerate() {
        let _ = writeln!(out, "T{}", index + 1);
        for via in &tool.vias {
            let _ = writeln!(out, "X{}Y{}", number(via.at[0] * scale), number(via.at[1] * scale));
        }
        for at in &tool.pads {
            let _ = writeln!(out, "X{}Y{}", number(at[0] * scale), number(at[1] * scale));
        }
        for slot in &tool.slots {
            let _ = writeln!(
                out,
                "X{}Y{}G85X{}Y{}",
                number(slot.from[0] * scale),
                number(slot.from[1] * scale),
                number(slot.to[0] * scale),
                number(slot.to[1] * scale)
            );
        }
    }
    for via in board.vias.iter().filter(|via| via.back_drill.is_some()) {
        let Some(back) = via.back_drill
        else {
            continue;
        };
        let size = number(via.drill * scale);
        let tool = tools.keys().position(|key| *key == size).map_or(1, |index| index + 1);
        let _ = writeln!(out, ";Back drill depth={} stub={}", number(back.depth * scale), number(back.stub * scale));
        let _ = writeln!(out, "T{tool}");
        let _ = writeln!(out, "X{}Y{}", number(via.at[0] * scale), number(via.at[1] * scale));
    }
    out.push_str("M30\n");
    out
}

/// Drill map with one circle per drilled hole, a tool legend and back-drill marks.
///
/// The map is a standalone SVG in millimetres. Every via and every drilled pad
/// contributes one plated hole, and routed slots are drawn as lines.
/// Back-drilled vias keep their hole and gain a dashed outer ring plus a legend
/// line, so a reader can tell which hits are back drilled without re-reading the
/// Excellon file.
fn drill_map(board: &Board) -> String {
    let scale = board.millimetres();
    let (min, max) = match board.outline {
        Some(outline) => ([outline.from[0] * scale, outline.from[1] * scale], [outline.to[0] * scale, outline.to[1] * scale]),
        None => drawing_bounds(board),
    };
    let span_x = (max[0] - min[0]).max(1.0);
    let span_y = (max[1] - min[1]).max(1.0);
    let margin = 6.0;
    let width = span_x + margin * 2.0;
    let height = span_y + margin * 2.0;
    let map_x = |x: f64| x - min[0] + margin;
    let map_y = |y: f64| max[1] - y + margin;

    let mut vias: Vec<&crate::board::Via> = board.vias.iter().collect();
    vias.sort_by(|left, right| left.drill.total_cmp(&right.drill).then_with(|| left.id.cmp(&right.id)));

    let mut tools: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    let mut back_drilled = 0usize;
    let mut out = String::new();
    let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.3}mm\" height=\"{height:.3}mm\" viewBox=\"0 0 {width:.3} {height:.3}\">"
    );
    let _ = writeln!(out, "<rect x=\"0\" y=\"0\" width=\"{width:.3}\" height=\"{height:.3}\" fill=\"#ffffff\"/>");
    let _ = writeln!(
        out,
        "<text x=\"4\" y=\"5\" font-family=\"monospace\" font-size=\"2.4\" fill=\"#111111\">OpenPCB drill map</text>"
    );
    if let Some(outline) = board.outline {
        let _ = writeln!(
            out,
            "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"none\" stroke=\"#8a6d1f\" stroke-width=\"0.15\"/>",
            map_x(outline.from[0] * scale),
            map_y(outline.to[1] * scale),
            span_x,
            span_y
        );
    }
    for via in &vias {
        let diameter = via.drill * scale;
        let radius = (diameter / 2.0).max(0.05);
        let _ = writeln!(
            out,
            "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{radius:.3}\" fill=\"#111111\" stroke=\"#000000\" stroke-width=\"0.05\"/>",
            map_x(via.at[0] * scale),
            map_y(via.at[1] * scale)
        );
        if via.back_drill.is_some() {
            back_drilled += 1;
            let _ = writeln!(
                out,
                "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"none\" stroke=\"#b03a2e\" stroke-width=\"0.08\" stroke-dasharray=\"0.4 0.3\"/>",
                map_x(via.at[0] * scale),
                map_y(via.at[1] * scale),
                radius + 0.5
            );
        }
        tools.entry(format!("{diameter:.3}")).or_insert((diameter, 0)).1 += 1;
    }
    for pad in board.placed_pads() {
        let Some(drill) = pad.drill
        else {
            continue;
        };
        let diameter = drill * scale;
        let radius = (diameter / 2.0).max(0.05);
        let _ = writeln!(
            out,
            "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{radius:.3}\" fill=\"#111111\" stroke=\"#000000\" stroke-width=\"0.05\"/>",
            map_x(pad.at[0] * scale),
            map_y(pad.at[1] * scale)
        );
        tools.entry(format!("{diameter:.3}")).or_insert((diameter, 0)).1 += 1;
    }
    for slot in &board.slots {
        let width = slot.width * scale;
        let _ = writeln!(
            out,
            "<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"#111111\" stroke-width=\"{width:.3}\" stroke-linecap=\"round\"/>",
            map_x(slot.from[0] * scale),
            map_y(slot.from[1] * scale),
            map_x(slot.to[0] * scale),
            map_y(slot.to[1] * scale)
        );
        tools.entry(format!("{width:.3}")).or_insert((width, 0)).1 += 1;
    }

    let mut line = 0usize;
    for (index, (_, (diameter, count))) in tools.iter().enumerate() {
        let _ = writeln!(
            out,
            "<text x=\"4\" y=\"{:.3}\" font-family=\"monospace\" font-size=\"2.2\" fill=\"#111111\">T{} dia {:.3} mm plated x{}</text>",
            8.0 + line as f64 * 3.0,
            index + 1,
            diameter,
            count
        );
        line += 1;
    }
    if !board.slots.is_empty() {
        let _ = writeln!(
            out,
            "<text x=\"4\" y=\"{:.3}\" font-family=\"monospace\" font-size=\"2.2\" fill=\"#111111\">routed slots x{} (line)</text>",
            8.0 + line as f64 * 3.0,
            board.slots.len()
        );
        line += 1;
    }
    if back_drilled > 0 {
        let _ = writeln!(
            out,
            "<text x=\"4\" y=\"{:.3}\" font-family=\"monospace\" font-size=\"2.2\" fill=\"#b03a2e\">back drill x{back_drilled} (dashed ring)</text>",
            8.0 + line as f64 * 3.0
        );
    }
    let _ = writeln!(out, "</svg>");
    out
}

/// Bill of materials grouped by value and package.
fn bom(board: &Board) -> String {
    let mut groups: BTreeMap<(String, String), Vec<&str>> = BTreeMap::new();
    for footprint in &board.footprints {
        groups.entry((footprint.value.clone(), footprint.footprint.clone())).or_default().push(footprint.ref_.as_str());
    }
    let mut out = String::from("Quantity,Value,Footprint,Designators\n");
    for ((value, footprint), mut designators) in groups {
        designators.sort_unstable();
        let _ = writeln!(out, "{},{},{},{}", designators.len(), csv(&value), csv(&footprint), csv(&designators.join(" ")));
    }
    out
}

/// Pick-and-place file in board coordinates.
fn position(board: &Board) -> String {
    let scale = board.millimetres();
    let mut footprints: Vec<_> = board.footprints.iter().collect();
    footprints.sort_by(|left, right| left.ref_.cmp(&right.ref_));
    let mut out = String::from("Designator,Val,Package,Mid X,Mid Y,Rotation,Side,Layer\n");
    for footprint in footprints {
        let side = if footprint.layer.starts_with("B.") { "Bottom" } else { "Top" };
        let _ = writeln!(
            out,
            "{},{},{},{},{},{:.3},{},{}",
            csv(&footprint.ref_),
            csv(&footprint.value),
            csv(&footprint.footprint),
            number(footprint.x * scale),
            number(footprint.y * scale),
            footprint.rotation,
            side,
            csv(&footprint.layer)
        );
    }
    out
}

/// Placement projection with one record per footprint and millimetre coordinates.
///
/// The JSON keeps the reference, value, package, position, rotation, side and
/// layer of every footprint. Coordinates are always in millimetres, so an
/// assembly cell does not have to convert the document unit. Pads count the
/// pads the document holds; no package geometry is invented.
fn placement(board: &Board) -> Result<String, ServiceError> {
    let scale = board.millimetres();
    let mut footprints: Vec<_> = board.footprints.iter().collect();
    footprints.sort_by(|left, right| left.ref_.cmp(&right.ref_));
    let placements: Vec<Value> = footprints
        .iter()
        .map(|footprint| {
            json!({
                "ref": footprint.ref_,
                "value": footprint.value,
                "footprint": footprint.footprint,
                "x": round3(footprint.x * scale),
                "y": round3(footprint.y * scale),
                "rotation": round3(footprint.rotation),
                "side": if footprint.layer.starts_with("B.") { "Bottom" } else { "Top" },
                "layer": footprint.layer,
                "pads": footprint.pads.len(),
            })
        })
        .collect();
    let document = json!({
        "revision": board.revision,
        "units": "mm",
        "placements": placements,
    });
    serde_json::to_string_pretty(&document)
        .map_err(|error| crate::invalid(format!("cannot serialize the placement export: {error}")))
}

/// Probe report with one CSV row per declared test point.
///
/// Pad test points carry the reference designator and pad number; via test
/// points keep the designator and pad cells empty and report the via span in
/// the layer cell. Coordinates are millimetres, like the other assembly files.
fn testpoints(board: &Board) -> String {
    let scale = board.millimetres();
    let mut out = String::from("Target,Designator,Pad,Kind,Net,Layer,X,Y\n");
    for point in board.resolved_test_points() {
        let _ = writeln!(
            out,
            "{},{},{},{},{},{},{:.3},{:.3}",
            csv(&point.target),
            csv(&point.designator),
            csv(&point.pad),
            point.kind,
            csv(point.net.as_deref().unwrap_or("")),
            csv(&point.layer),
            point.at[0] * scale,
            point.at[1] * scale
        );
    }
    out
}

/// IPC-D-356A bare-board test netlist.
///
/// Every pad becomes a `327` surface-mount record and every via a `317` through
/// or `307` blind/buried via record. Each line is padded to the 80-column record
/// width, units are `CUST 1` (millimetres) and coordinates use the 0.001 mm
/// field, so a test fixture can read probe positions without unit conversion.
fn ipc_d_356(board: &Board) -> String {
    let scale = board.millimetres();
    let mut records: Vec<Ipc356Record> = Vec::new();
    for footprint in &board.footprints {
        let angle = footprint.rotation.to_radians();
        let (sin, cos) = (angle.sin(), angle.cos());
        let side = if footprint.layer.starts_with("B.") { "02" } else { "01" };
        let mask = if footprint.layer.starts_with("B.") { 2 } else { 1 };
        for pad in &footprint.pads {
            let x = footprint.x + pad.x * cos - pad.y * sin;
            let y = footprint.y + pad.x * sin + pad.y * cos;
            records.push(Ipc356Record {
                code: "327",
                net: pad.net.clone(),
                reference: footprint.ref_.clone(),
                pin: pad.number.clone(),
                hole: None,
                access: side,
                at: [x, y],
                size: [pad.w, pad.h],
                rotation: footprint.rotation,
                mask,
            });
        }
    }
    for (index, via) in board.vias.iter().enumerate() {
        let kind = board.via_kind(via);
        records.push(Ipc356Record {
            code: if kind == "through" { "317" } else { "307" },
            net: via.net.clone(),
            reference: "VIA".to_string(),
            pin: format!("{}", index + 1),
            hole: Some(via.drill),
            access: "00",
            at: via.at,
            size: [via.diameter, via.diameter],
            rotation: 0.0,
            mask: 0,
        });
    }
    records.sort_by(|left, right| {
        let left_net = left.net.as_deref().unwrap_or("N/C");
        let right_net = right.net.as_deref().unwrap_or("N/C");
        left_net.cmp(right_net).then_with(|| left.reference.cmp(&right.reference)).then_with(|| left.pin.cmp(&right.pin))
    });

    let title = board.title.as_deref().filter(|title| !title.trim().is_empty()).unwrap_or(&board.id);
    let mut out = String::new();
    out.push_str(&ipc_record_line("C  IPC-D-356A"));
    out.push_str(&ipc_parameter("JOB", &ipc_text(title)));
    out.push_str(&ipc_parameter("CODE", "00"));
    out.push_str(&ipc_parameter("UNITS", "CUST 1"));
    out.push_str(&ipc_parameter("TITLE", &ipc_text(title)));
    out.push_str(&ipc_parameter("NUM", &ipc_text(&board.id)));
    out.push_str(&ipc_parameter("REV", &board.revision.to_string()));
    out.push_str(&ipc_parameter("VER", "IPC-D-356A"));
    out.push_str(&ipc_parameter("IMAGE", "PRIMARY"));
    for record in &records {
        out.push_str(&record.render(scale));
    }
    out.push_str(&ipc_record_line("999"));
    out
}

/// One standard electrical test record of an IPC-D-356A file.
struct Ipc356Record {
    code: &'static str,
    net: Option<String>,
    reference: String,
    pin: String,
    hole: Option<f64>,
    access: &'static str,
    at: [f64; 2],
    size: [f64; 2],
    rotation: f64,
    mask: u8,
}

impl Ipc356Record {
    /// Renders the 80-column record.
    fn render(&self, scale: f64) -> String {
        let net = self.net.as_deref().map(ipc_text).filter(|name| !name.is_empty()).unwrap_or_else(|| "N/C".to_string());
        let reference = ipc_text(&self.reference);
        let pin = ipc_text(&self.pin);
        let mut line = format!("{}{net:<14.14}   {reference:<6.6}-{pin:<4.4} ", self.code);
        match self.hole {
            Some(diameter) => {
                line.push('D');
                line.push_str(&format!("{:0>4}", ipc_micrometres(diameter, scale).min(9999)));
                line.push('P');
            }
            None => line.push_str("      "),
        }
        line.push('A');
        line.push_str(self.access);
        let (x_sign, x) = ipc_coordinate(self.at[0], scale);
        let (y_sign, y) = ipc_coordinate(self.at[1], scale);
        line.push_str(&format!("X{x_sign}{x}Y{y_sign}{y}"));
        line.push_str(&format!(
            "X{:0>4}Y{:0>4}R{:0>3} ",
            ipc_micrometres(self.size[0], scale).min(9999),
            ipc_micrometres(self.size[1], scale).min(9999),
            self.rotation.round().rem_euclid(360.0) as u32
        ));
        line.push('S');
        line.push_str(&self.mask.to_string());
        ipc_record_line(&line)
    }
}

/// Pads an IPC-D-356A record to its fixed 80-column width.
fn ipc_record_line(line: &str) -> String {
    let mut record = line.chars().take(80).collect::<String>();
    while record.len() < 80 {
        record.push(' ');
    }
    record.push('\n');
    record
}

/// Formats one `P` header parameter with its value in columns 10-72.
fn ipc_parameter(parameter: &str, value: &str) -> String {
    let mut line = format!("P  {parameter:<4}  {value}");
    if line.len() > 72 {
        line.truncate(72);
    }
    ipc_record_line(&line)
}

/// Replaces whitespace so a name fits one IPC-D-356A field.
fn ipc_text(value: &str) -> String {
    value.chars().map(|character| if character.is_whitespace() { '_' } else { character }).collect()
}

/// Converts one board coordinate to the 0.001 mm field and its sign column.
fn ipc_coordinate(value: f64, scale: f64) -> (char, String) {
    let micrometres = (value * scale * 1000.0).round();
    let digits = if micrometres < 0.0 { -micrometres } else { micrometres };
    let number = if digits.is_finite() { digits.min(999_999.0) as u64 } else { 0 };
    (if micrometres < 0.0 { '-' } else { '+' }, format!("{number:0>6}"))
}

/// Converts one size to the 0.001 mm field.
fn ipc_micrometres(value: f64, scale: f64) -> u64 {
    let micrometres = (value * scale * 1000.0).round();
    if micrometres.is_finite() && micrometres > 0.0 { micrometres.min(9999.0) as u64 } else { 0 }
}

/// Rounds a coordinate to three decimals without a negative zero.
fn round3(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    let value = (value * 1000.0).round() / 1000.0;
    if value == 0.0 { 0.0 } else { value }
}

/// Top and bottom composite of tracks, pads and vias.
fn svg(board: &Board) -> Result<String, ServiceError> {
    let (content, _, _) = svg_preview(board)?;
    Ok(content)
}

/// Single-page vector PDF sized to A4 landscape or to the board itself.
///
/// The page uses point coordinates (1 mm = 2.834646 pt). `document-drawing`
/// owns the geometry lowering and the PDF backend, so this writer only places
/// the same outline, tracks, pads and vias the preview draws.
fn pdf(board: &Board) -> Result<Vec<u8>, ServiceError> {
    let scale = board.millimetres();
    let (min, max) = drawing_bounds(board);
    let board_width = (max[0] - min[0]) * scale * PT_PER_MM;
    let board_height = (max[1] - min[1]) * scale * PT_PER_MM;
    let page_width = A4_LANDSCAPE_PT[0].max(board_width + PDF_MARGIN_PT * 2.0);
    let page_height = A4_LANDSCAPE_PT[1].max(board_height + PDF_MARGIN_PT * 2.0);
    let layout = Layout {
        scale: PT_PER_MM,
        origin_x: min[0] - (page_width - board_width) / (2.0 * PT_PER_MM),
        origin_y: max[1] + (page_height - board_height) / (2.0 * PT_PER_MM),
        width: page_width,
        height: page_height,
        y_up: true,
    };
    build_drawing(board, &layout, None, true)
        .to_pdf()
        .map_err(|error| crate::invalid(format!("cannot render the PDF preview: {error}")))
}

/// Maps board millimetres onto a drawing canvas.
///
/// Board documents use the KiCad convention of Y pointing down. SVG and the
/// raster canvas share that direction, while PDF pages count Y upwards, so the
/// layout records which way its target grows instead of always flipping.
struct Layout {
    scale: f64,
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
    y_up: bool,
}

impl Layout {
    fn point(&self, x: f64, y: f64) -> DrawPoint {
        let y = if self.y_up { self.origin_y - y } else { y - self.origin_y };
        DrawPoint::new((x - self.origin_x) * self.scale, y * self.scale)
    }
}

/// Lowers the board onto a `document-drawing` canvas.
///
/// `with_text_items` adds silkscreen text items; the SVG preview appends them
/// itself so it can keep their rotation, while the PDF writer takes them here.
fn build_drawing(board: &Board, layout: &Layout, background: Option<Color>, with_text_items: bool) -> Drawing {
    let scale = board.millimetres();
    let mut drawing = Drawing::new(Size::new(layout.width, layout.height));
    if let Some(background) = background {
        drawing = drawing.with_background(background);
    }
    if let Some(outline) = board.outline {
        let corners = [
            [outline.from[0], outline.from[1]],
            [outline.to[0], outline.from[1]],
            [outline.to[0], outline.to[1]],
            [outline.from[0], outline.to[1]],
        ];
        drawing.push(Element::new(
            Shape::Polygon {
                points: corners.iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect(),
            },
            Style::stroke(layer_color_value(board, "Edge.Cuts"), 0.15 * layout.scale),
        ));
    }
    for zone in &board.zones {
        let points = zone.points.iter().map(|point| layout.point(point[0] * scale, point[1] * scale)).collect();
        let mut style = fill_style(layer_color_value(board, &zone.layer));
        style.opacity = 0.35;
        drawing.push(Element::new(Shape::Polygon { points }, style));
    }
    for track in &board.tracks {
        drawing.push(Element::new(
            Shape::Line {
                from: layout.point(track.from[0] * scale, track.from[1] * scale),
                to: layout.point(track.to[0] * scale, track.to[1] * scale),
            },
            Style::stroke(layer_color_value(board, &track.layer), track.width * scale * layout.scale),
        ));
    }
    for footprint in &board.footprints {
        let color = layer_color_value(board, &footprint.layer);
        for pad in &footprint.pads {
            let corners = pad_corners(footprint, pad);
            let points = corners.iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect();
            drawing.push(Element::new(Shape::Polygon { points }, fill_style(color)));
            if let Some(drill) = pad.drill.filter(|drill| *drill > 0.0) {
                let center = pad_center(footprint, pad);
                drawing.push(Element::new(
                    Shape::Circle {
                        center: layout.point(center[0] * scale, center[1] * scale),
                        radius: (drill * scale * layout.scale / 2.0).max(0.1),
                    },
                    fill_style(DRILL_COLOR),
                ));
            }
        }
        let Some((min, max)) = footprint_silk(footprint)
        else {
            continue;
        };
        let silk = layer_color_value(board, silk_layer(&footprint.layer));
        let corners = [[min[0], min[1]], [max[0], min[1]], [max[0], max[1]], [min[0], max[1]]];
        drawing.push(Element::new(
            Shape::Polygon {
                points: corners.iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect(),
            },
            Style::stroke(silk, 0.15 * layout.scale),
        ));
        drawing.push(Element::new(
            Shape::Text {
                at: layout.point(min[0] * scale, (min[1] - 0.6) * scale),
                text: footprint.ref_.clone(),
                size: REFERENCE_SIZE * layout.scale,
            },
            fill_style(silk),
        ));
    }
    for via in &board.vias {
        let center = layout.point(via.at[0] * scale, via.at[1] * scale);
        drawing.push(Element::new(
            Shape::Circle { center, radius: via.diameter * scale * layout.scale / 2.0 },
            fill_style(layer_color_value(board, "F.Cu")),
        ));
        if via.drill > 0.0 {
            drawing.push(Element::new(
                Shape::Circle { center, radius: (via.drill * scale * layout.scale / 2.0).max(0.1) },
                fill_style(DRILL_COLOR),
            ));
        }
    }
    for slot in &board.slots {
        drawing.push(Element::new(
            Shape::Line {
                from: layout.point(slot.from[0] * scale, slot.from[1] * scale),
                to: layout.point(slot.to[0] * scale, slot.to[1] * scale),
            },
            Style::stroke(layer_color_value(board, "F.Cu"), slot.width * scale * layout.scale),
        ));
    }
    if with_text_items {
        for text in &board.texts {
            drawing.push(Element::new(
                Shape::Text {
                    at: layout.point(text.at[0] * scale, text.at[1] * scale),
                    text: text.text.clone(),
                    size: text.size * scale * layout.scale,
                },
                fill_style(layer_color_value(board, &text.layer)),
            ));
        }
    }
    drawing
}

/// Filled paint with no outline.
///
/// `Style::fill` keeps the library's default 1-unit black stroke, which turns
/// every pad, zone and via into a black-ringed blob. Copper and silkscreen are
/// single-colour areas, so a preview wants the fill alone.
fn fill_style(color: Color) -> Style {
    Style { stroke: None, fill: Some(color), ..Style::default() }
}

/// Colour of a drilled hole in every preview writer.
const DRILL_COLOR: Color = Color::rgb(12, 20, 16);

/// Centre of one pad in board coordinates, with the footprint rotation folded in.
fn pad_center(footprint: &crate::board::Footprint, pad: &crate::board::Pad) -> [f64; 2] {
    let angle = footprint.rotation.to_radians();
    let (sin, cos) = (angle.sin(), angle.cos());
    [footprint.x + pad.x * cos - pad.y * sin, footprint.y + pad.x * sin + pad.y * cos]
}

/// Silkscreen outline of one footprint in board units: its pad bounding box
/// grown by [`SILK_MARGIN`].
fn footprint_silk(footprint: &crate::board::Footprint) -> Option<([f64; 2], [f64; 2])> {
    if footprint.pads.is_empty() {
        return None;
    }
    let mut min = [f64::INFINITY, f64::INFINITY];
    let mut max = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    for pad in &footprint.pads {
        for corner in pad_corners(footprint, pad) {
            min[0] = min[0].min(corner[0]);
            min[1] = min[1].min(corner[1]);
            max[0] = max[0].max(corner[0]);
            max[1] = max[1].max(corner[1]);
        }
    }
    Some(([min[0] - SILK_MARGIN, min[1] - SILK_MARGIN], [max[0] + SILK_MARGIN, max[1] + SILK_MARGIN]))
}

/// Silkscreen layer of the side a footprint is placed on.
fn silk_layer(layer: &str) -> &'static str {
    if layer.starts_with("B.") { "B.SilkS" } else { "F.SilkS" }
}

/// Returns the four rotated corners of one pad in board units.
fn pad_corners(footprint: &crate::board::Footprint, pad: &crate::board::Pad) -> [[f64; 2]; 4] {
    let angle = footprint.rotation.to_radians();
    let (sin, cos) = (angle.sin(), angle.cos());
    let center_x = footprint.x + pad.x * cos - pad.y * sin;
    let center_y = footprint.y + pad.x * sin + pad.y * cos;
    let half_w = pad.w / 2.0;
    let half_h = pad.h / 2.0;
    [[-half_w, -half_h], [half_w, -half_h], [half_w, half_h], [-half_w, half_h]].map(|[local_x, local_y]| {
        let x = center_x + local_x * cos - local_y * sin;
        let y = center_y + local_x * sin + local_y * cos;
        [x, y]
    })
}

/// Resolves a layer colour, falling back to the default copper tone.
fn layer_color_value(board: &Board, layer: &str) -> Color {
    let text = layer_color(board, layer);
    let Some(hex) = text.strip_prefix('#').filter(|hex| hex.len() == 6)
    else {
        return Color::rgb(200, 117, 51);
    };
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&hex[range], 16).ok();
    match (channel(0..2), channel(2..4), channel(4..6)) {
        (Some(red), Some(green), Some(blue)) => Color::rgb(red, green, blue),
        _ => Color::rgb(200, 117, 51),
    }
}

/// Collects the board bounds in board units from every drawn object.
fn drawing_bounds(board: &Board) -> ([f64; 2], [f64; 2]) {
    let scale = board.millimetres();
    let mut min = [f64::INFINITY, f64::INFINITY];
    let mut max = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    if let Some(outline) = board.outline {
        include(&mut min, &mut max, outline.from[0] * scale, outline.from[1] * scale);
        include(&mut min, &mut max, outline.to[0] * scale, outline.to[1] * scale);
    }
    for track in &board.tracks {
        include(&mut min, &mut max, track.from[0] * scale, track.from[1] * scale);
        include(&mut min, &mut max, track.to[0] * scale, track.to[1] * scale);
    }
    for footprint in &board.footprints {
        let angle = footprint.rotation.to_radians();
        let (sin, cos) = (angle.sin(), angle.cos());
        for pad in &footprint.pads {
            let center_x = footprint.x + pad.x * cos - pad.y * sin;
            let center_y = footprint.y + pad.x * sin + pad.y * cos;
            let half_w = (cos.abs() * pad.w + sin.abs() * pad.h) / 2.0;
            let half_h = (sin.abs() * pad.w + cos.abs() * pad.h) / 2.0;
            include(&mut min, &mut max, (center_x - half_w) * scale, (center_y - half_h) * scale);
            include(&mut min, &mut max, (center_x + half_w) * scale, (center_y + half_h) * scale);
        }
    }
    for via in &board.vias {
        let radius = via.diameter / 2.0;
        include(&mut min, &mut max, (via.at[0] - radius) * scale, (via.at[1] - radius) * scale);
        include(&mut min, &mut max, (via.at[0] + radius) * scale, (via.at[1] + radius) * scale);
    }
    for slot in &board.slots {
        let radius = slot.width / 2.0;
        include(&mut min, &mut max, (slot.from[0] - radius) * scale, (slot.from[1] - radius) * scale);
        include(&mut min, &mut max, (slot.to[0] + radius) * scale, (slot.to[1] + radius) * scale);
    }
    for zone in &board.zones {
        for point in &zone.points {
            include(&mut min, &mut max, point[0] * scale, point[1] * scale);
        }
    }
    if !min[0].is_finite() || !min[1].is_finite() || !max[0].is_finite() || !max[1].is_finite() {
        return ([0.0, 0.0], [100.0, 60.0]);
    }
    (min, max)
}

/// Renders the composite and reports the canvas size the same viewBox used.
pub(crate) fn svg_preview(board: &Board) -> Result<(String, f64, f64), ServiceError> {
    let scale = board.millimetres();
    let (mut min, mut max) = drawing_bounds(board);
    for text in &board.texts {
        let width = text.text.chars().count() as f64 * text.size * 0.6;
        include(&mut min, &mut max, text.at[0] * scale, (text.at[1] - text.size) * scale);
        include(&mut min, &mut max, (text.at[0] + width) * scale, (text.at[1] + text.size) * scale);
    }
    for footprint in &board.footprints {
        let Some((silk_min, silk_max)) = footprint_silk(footprint)
        else {
            continue;
        };
        include(&mut min, &mut max, silk_min[0] * scale, (silk_min[1] - REFERENCE_SIZE - 0.6) * scale);
        include(&mut min, &mut max, silk_max[0] * scale, silk_max[1] * scale);
    }
    if !min[0].is_finite() || !min[1].is_finite() || !max[0].is_finite() || !max[1].is_finite() {
        min = [0.0, 0.0];
        max = [100.0, 60.0];
    }
    let margin = 5.0;
    let width = (max[0] - min[0]).max(1.0) + margin * 2.0;
    let height = (max[1] - min[1]).max(1.0) + margin * 2.0;
    let layout = Layout { scale: 1.0, origin_x: min[0] - margin, origin_y: min[1] - margin, width, height, y_up: false };
    let drawing = build_drawing(board, &layout, Some(Color::rgb(14, 26, 20)), false);
    let mut content = drawing.to_svg().map_err(|error| crate::invalid(format!("cannot render the SVG preview: {error}")))?;
    if !board.texts.is_empty()
        && let Some(index) = content.rfind("</svg>")
    {
        let mut group = String::from("    <g class=\"board-text\">\n");
        for text in &board.texts {
            let x = (text.at[0] * scale - layout.origin_x) * layout.scale;
            let y = (text.at[1] * scale - layout.origin_y) * layout.scale;
            let _ = writeln!(
                group,
                "      <text x=\"{x:.3}\" y=\"{y:.3}\" font-family=\"monospace\" font-size=\"{:.3}\" fill=\"{}\" transform=\"rotate({:.3} {x:.3} {y:.3})\">{}</text>",
                text.size * scale * layout.scale,
                layer_color(board, &text.layer),
                -text.rotation,
                xml(&text.text)
            );
        }
        group.push_str("    </g>\n");
        content.insert_str(index, &group);
    }
    Ok((content, width, height))
}

fn include(min: &mut [f64; 2], max: &mut [f64; 2], x: f64, y: f64) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    min[0] = min[0].min(x);
    min[1] = min[1].min(y);
    max[0] = max[0].max(x);
    max[1] = max[1].max(y);
}

fn layer_color(board: &Board, layer: &str) -> String {
    board.layer(layer).map(|layer| xml(&layer.color)).filter(|color| !color.is_empty()).unwrap_or_else(|| "#c87533".to_string())
}

/// Formats a coordinate with three decimals and no negative zero.
fn number(value: f64) -> String {
    if !value.is_finite() {
        return "0.000".to_string();
    }
    let value = if value.abs() < 0.0005 { 0.0 } else { value };
    format!("{value:.3}")
}

fn csv(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) { format!("\"{}\"", value.replace('"', "\"\"")) } else { value.to_string() }
}

fn xml(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Width of the PNG preview, in pixels.
const PNG_WIDTH: u32 = 1024;
/// Height of the PNG preview, in pixels.
const PNG_HEIGHT: u32 = 768;
/// Margin between the board and the image edge, in pixels.
const PNG_MARGIN: f64 = 16.0;
/// Preview background.
const PNG_BACKGROUND: [u8; 3] = [14, 26, 20];
/// Preview outline colour.
const PNG_OUTLINE: [u8; 3] = [217, 180, 74];
/// Front copper colour.
const PNG_FRONT: [u8; 3] = [200, 117, 51];
/// Back copper colour.
const PNG_BACK: [u8; 3] = [47, 111, 176];
/// Silkscreen colour.
const PNG_SILK: [u8; 3] = [230, 230, 230];
/// Drilled-hole colour.
const PNG_DRILL: [u8; 3] = [12, 20, 16];
/// Monotonic suffix for the temporary file the encoder writes before the
/// atomic rename, so concurrent exports never collide.
static PNG_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Writes a top-down raster preview as a valid 8-bit RGBA PNG.
///
/// Encoding is delegated to `media-codec`; the image bytes are staged in a
/// temporary file in the destination directory and then published atomically.
fn png(board: &Board, path: &Path) -> Result<Value, ServiceError> {
    let scale = board.millimetres();
    let has_pads = board.footprints.iter().any(|footprint| !footprint.pads.is_empty());
    if board.outline.is_none() && board.tracks.is_empty() && board.vias.is_empty() && !has_pads {
        return Err(crate::invalid("the board has no outline or geometry; nothing can be rendered to PNG"));
    }

    let (min, max) = match board.outline {
        Some(outline) => ([outline.from[0] * scale, outline.from[1] * scale], [outline.to[0] * scale, outline.to[1] * scale]),
        None => drawing_bounds(board),
    };
    let span_x = (max[0] - min[0]).max(1e-6);
    let span_y = (max[1] - min[1]).max(1e-6);
    let usable_width = (PNG_WIDTH as f64 - PNG_MARGIN * 2.0).max(1.0);
    let usable_height = (PNG_HEIGHT as f64 - PNG_MARGIN * 2.0).max(1.0);
    let scale_px = (usable_width / span_x).min(usable_height / span_y).floor().max(1.0);
    let content_width = span_x * scale_px;
    let content_height = span_y * scale_px;
    let view = View {
        scale: scale_px,
        offset_x: (PNG_WIDTH as f64 - content_width) / 2.0 - min[0] * scale_px,
        offset_y: (PNG_HEIGHT as f64 - content_height) / 2.0 - min[1] * scale_px,
    };

    let mut canvas = Canvas::new(PNG_WIDTH, PNG_HEIGHT, PNG_BACKGROUND);
    if let Some(outline) = board.outline {
        let corners = [
            view.point([outline.from[0] * scale, outline.from[1] * scale]),
            view.point([outline.to[0] * scale, outline.from[1] * scale]),
            view.point([outline.to[0] * scale, outline.to[1] * scale]),
            view.point([outline.from[0] * scale, outline.to[1] * scale]),
        ];
        for index in 0..corners.len() {
            canvas.line(corners[index], corners[(index + 1) % corners.len()], 0.75, PNG_OUTLINE);
        }
    }

    let mut tracks = 0usize;
    for track in &board.tracks {
        let color = match track.layer.as_str() {
            "F.Cu" => PNG_FRONT,
            "B.Cu" => PNG_BACK,
            _ => continue,
        };
        tracks += 1;
        let radius = (track.width * scale * scale_px / 2.0).max(0.5);
        canvas.line(view.point(track.from), view.point(track.to), radius, color);
    }

    let mut pads = 0usize;
    for footprint in &board.footprints {
        let angle = footprint.rotation.to_radians();
        let (sin, cos) = (angle.sin(), angle.cos());
        let color = if footprint.layer.starts_with("B.") { PNG_BACK } else { PNG_FRONT };
        for pad in &footprint.pads {
            pads += 1;
            let center = view.point(pad_center(footprint, pad).map(|value| value * scale));
            let half_w = (cos.abs() * pad.w + sin.abs() * pad.h) / 2.0 * scale * scale_px;
            let half_h = (sin.abs() * pad.w + cos.abs() * pad.h) / 2.0 * scale * scale_px;
            let x0 = (center[0] - half_w).floor() as i32;
            let x1 = (center[0] + half_w).ceil() as i32;
            let y0 = (center[1] - half_h).floor() as i32;
            let y1 = (center[1] + half_h).ceil() as i32;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let dx = (x as f64 + 0.5 - center[0]) / scale_px;
                    let dy = (y as f64 + 0.5 - center[1]) / scale_px;
                    let local_x = dx * cos + dy * sin;
                    let local_y = -dx * sin + dy * cos;
                    if local_x.abs() <= pad.w / 2.0 + 1e-9 && local_y.abs() <= pad.h / 2.0 + 1e-9 {
                        canvas.set(x, y, color);
                    }
                }
            }
            if let Some(drill) = pad.drill.filter(|drill| *drill > 0.0) {
                canvas.circle(center, (drill * scale * scale_px / 2.0).max(0.5), PNG_DRILL);
            }
        }
        let Some((min, max)) = footprint_silk(footprint)
        else {
            continue;
        };
        let corners = [
            view.point([min[0] * scale, min[1] * scale]),
            view.point([max[0] * scale, min[1] * scale]),
            view.point([max[0] * scale, max[1] * scale]),
            view.point([min[0] * scale, max[1] * scale]),
        ];
        for index in 0..corners.len() {
            canvas.line(corners[index], corners[(index + 1) % corners.len()], 0.6, PNG_SILK);
        }
        draw_text(
            &mut canvas,
            view.point([min[0] * scale, (min[1] - 0.6) * scale]),
            &footprint.ref_,
            REFERENCE_SIZE * scale * scale_px,
            PNG_SILK,
        );
    }

    for via in &board.vias {
        let center = view.point([via.at[0] * scale, via.at[1] * scale]);
        let radius = (via.diameter * scale * scale_px / 2.0).max(0.5);
        canvas.circle(center, radius, PNG_FRONT);
        if via.drill > 0.0 {
            canvas.circle(center, (via.drill * scale * scale_px / 2.0).max(0.5), PNG_DRILL);
        }
    }

    for text in &board.texts {
        draw_text(
            &mut canvas,
            view.point([text.at[0] * scale, text.at[1] * scale]),
            &text.text,
            text.size * scale * scale_px,
            PNG_SILK,
        );
    }

    let mut rgba = Vec::with_capacity(canvas.data.len() / 3 * 4);
    for pixel in canvas.data.as_chunks::<3>().0 {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 0xff]);
    }
    let temporary = path.with_file_name(format!(
        ".{}.png-{}-{}.tmp",
        path.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id(),
        PNG_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let staged = (|| {
        media_codec::write_png(&temporary, PNG_WIDTH, PNG_HEIGHT, &rgba)
            .map_err(|error| crate::invalid(format!("cannot encode `{}`: {error}", path.display())))?;
        let bytes = fs::read(&temporary)
            .map_err(|error| crate::invalid(format!("cannot read the encoded `{}`: {error}", path.display())))?;
        crate::atomic_write(path, &bytes)
    })();
    let _ = fs::remove_file(&temporary);
    let written = staged?;
    Ok(json!({
        "revision": board.revision,
        "format": "png",
        "path": path.display().to_string(),
        "bytes": written,
        "editable": false,
        "artifact": {
            "kind": "png",
            "width": PNG_WIDTH,
            "height": PNG_HEIGHT,
            "tracks": tracks,
            "pads": pads,
            "vias": board.vias.len(),
        },
    }))
}

/// Maps board millimetres onto image pixels with the same Y direction as the
/// document, so a board placed with small Y near the top renders near the top.
struct View {
    scale: f64,
    offset_x: f64,
    offset_y: f64,
}

impl View {
    fn point(&self, point: [f64; 2]) -> [f64; 2] {
        [point[0] * self.scale + self.offset_x, point[1] * self.scale + self.offset_y]
    }
}

/// RGB pixel buffer with clipped drawing primitives.
struct Canvas {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl Canvas {
    fn new(width: u32, height: u32, background: [u8; 3]) -> Self {
        let mut data = Vec::with_capacity(width as usize * height as usize * 3);
        for _ in 0..width as usize * height as usize {
            data.extend_from_slice(&background);
        }
        Self { width, height, data }
    }

    fn set(&mut self, x: i32, y: i32, color: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        let index = (y as usize * self.width as usize + x as usize) * 3;
        self.data[index..index + 3].copy_from_slice(&color);
    }

    fn circle(&mut self, center: [f64; 2], radius: f64, color: [u8; 3]) {
        let x0 = (center[0] - radius).floor() as i32;
        let x1 = (center[0] + radius).ceil() as i32;
        let y0 = (center[1] - radius).floor() as i32;
        let y1 = (center[1] + radius).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f64 + 0.5 - center[0];
                let dy = y as f64 + 0.5 - center[1];
                if dx * dx + dy * dy <= radius * radius {
                    self.set(x, y, color);
                }
            }
        }
    }

    fn line(&mut self, from: [f64; 2], to: [f64; 2], radius: f64, color: [u8; 3]) {
        let x0 = (from[0].min(to[0]) - radius).floor() as i32;
        let x1 = (from[0].max(to[0]) + radius).ceil() as i32;
        let y0 = (from[1].min(to[1]) - radius).floor() as i32;
        let y1 = (from[1].max(to[1]) + radius).ceil() as i32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let point = [x as f64 + 0.5, y as f64 + 0.5];
                if pixel_segment_distance(point, from, to) <= radius {
                    self.set(x, y, color);
                }
            }
        }
    }
}

fn pixel_segment_distance(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> f64 {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let length = dx * dx + dy * dy;
    if length <= 1e-12 {
        return ((point[0] - start[0]).powi(2) + (point[1] - start[1]).powi(2)).sqrt();
    }
    let t = (((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / length).clamp(0.0, 1.0);
    let closest = [start[0] + t * dx, start[1] + t * dy];
    ((point[0] - closest[0]).powi(2) + (point[1] - closest[1]).powi(2)).sqrt()
}

/// Draws one line of silkscreen text with the built-in 5×7 font.
///
/// `baseline` is the left end of the baseline in pixels and glyphs grow upwards
/// from it. Characters the font does not know are skipped, so a legend in a
/// script the font lacks leaves a gap instead of drawing noise.
fn draw_text(canvas: &mut Canvas, baseline: [f64; 2], text: &str, height: f64, color: [u8; 3]) {
    let unit = (height / 7.0).round().max(1.0);
    let mut x = baseline[0];
    for character in text.chars() {
        let Some(rows) = glyph(character)
        else {
            x += 6.0 * unit;
            continue;
        };
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..5 {
                if bits & (0b10000 >> column) == 0 {
                    continue;
                }
                let left = (x + column as f64 * unit).round() as i32;
                let top = (baseline[1] - 7.0 * unit + row as f64 * unit).round() as i32;
                for dy in 0..unit as i32 {
                    for dx in 0..unit as i32 {
                        canvas.set(left + dx, top + dy, color);
                    }
                }
            }
        }
        x += 6.0 * unit;
    }
}

/// 5×7 bitmap of one character; bit 4 is the leftmost column of a row.
fn glyph(character: char) -> Option<[u8; 7]> {
    let rows = match character.to_ascii_uppercase() {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111],
        'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'N' => [0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001],
        'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        'S' => [0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110],
        'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100],
        '_' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b11111],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        ':' => [0b00000, 0b01100, 0b01100, 0b00000, 0b01100, 0b01100, 0b00000],
        ' ' => [0; 7],
        _ => return None,
    };
    Some(rows)
}
