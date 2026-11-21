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
pub(crate) const SUPPORTED_EXPORTS: &[&str] = &["gbr", "drl", "bom", "pos", "svg", "pdf", "kicad_pcb", "ipc2581", "png"];

/// Points per millimetre used by the vector PDF writer.
const PT_PER_MM: f64 = 2.834646;

/// Width and height of one A4 page in landscape orientation, in points.
const A4_LANDSCAPE_PT: [f64; 2] = [841.89, 595.28];

/// Margin kept around the board inside a PDF page, in points (10 mm).
const PDF_MARGIN_PT: f64 = 28.34646;

/// Preview renderers this build can produce without writing a file.
pub(crate) const SUPPORTED_PREVIEWS: &[&str] = &["svg"];

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
        "bom" => bom(board).into_bytes(),
        "pos" => position(board).into_bytes(),
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
        let code = self.aperture(definition);
        let _ = writeln!(
            self.body,
            "D{code}*\nX{}Y{}D02*\nX{}Y{}D01*",
            gerber_number(from[0]),
            gerber_number(from[1]),
            gerber_number(to[0]),
            gerber_number(to[1])
        );
    }

    fn flash(&mut self, definition: String, at: [f64; 2]) {
        let code = self.aperture(definition);
        let _ = writeln!(self.body, "D{code}*\nX{}Y{}D03*", gerber_number(at[0]), gerber_number(at[1]));
    }

    fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("%FSLAX46Y46*%\n%MOMM*%\n%LPD*%\nG01*\n");
        for (index, definition) in self.apertures.iter().enumerate() {
            let _ = writeln!(out, "%ADD{}{definition}*%", 10 + index);
        }
        out.push_str(&self.body);
        out.push_str("M02*\n");
        out
    }
}

/// One Gerber file name for a layer id, with `.` replaced by `_`.
fn gerber_name(layer: &str) -> String {
    format!("{}.gbr", layer.replace('.', "_"))
}

/// Writes one copper layer: tracks as draws, pads and covered vias as flashes.
fn copper(board: &Board, layer: &str) -> String {
    let scale = board.millimetres();
    let mut gerber = Gerber::default();
    for track in board.tracks.iter().filter(|track| track.layer == layer) {
        gerber.draw(
            format!("C,{:.6}", track.width * scale),
            [track.from[0] * scale, track.from[1] * scale],
            [track.to[0] * scale, track.to[1] * scale],
        );
    }
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer == layer) {
        for pad in &footprint.pads {
            gerber.flash(
                format!("R,{:.6}X{:.6}", pad.w * scale, pad.h * scale),
                [(footprint.x + pad.x) * scale, (footprint.y + pad.y) * scale],
            );
        }
    }
    for via in board.vias.iter().filter(|via| board.via_covers(via, layer)) {
        gerber.flash(format!("C,{:.6}", via.diameter * scale), [via.at[0] * scale, via.at[1] * scale]);
    }
    gerber.render()
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
    gerber.render()
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
    gerber.render()
}

/// Formats millimetres in the 4.6 coordinate format used by every file.
fn gerber_number(value: f64) -> i64 {
    let scaled = (value * 1_000_000.0).round();
    if scaled.is_finite() { scaled as i64 } else { 0 }
}

/// Excellon drill file with one tool per distinct finished hole size.
fn drill(board: &Board) -> String {
    let scale = board.millimetres();
    let mut tools: BTreeMap<String, Vec<&crate::board::Via>> = BTreeMap::new();
    for via in &board.vias {
        tools.entry(number(via.drill * scale)).or_default().push(via);
    }
    let mut out = String::from("M48\n;OpenPCB drill file\n;Units: mm\nMETRIC,TZ\n");
    for (index, (size, _)) in tools.iter().enumerate() {
        let _ = writeln!(out, "T{}C{size}", index + 1);
    }
    out.push_str("%\nG90\nG05\n");
    for (index, (_, hits)) in tools.iter().enumerate() {
        let _ = writeln!(out, "T{}", index + 1);
        for via in hits {
            let _ = writeln!(out, "X{}Y{}", number(via.at[0] * scale), number(via.at[1] * scale));
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
    let mut out = String::from("Designator,Val,Package,Mid X,Mid Y,Rotation,Side\n");
    for footprint in footprints {
        let side = if footprint.layer.starts_with("B.") { "Bottom" } else { "Top" };
        let _ = writeln!(
            out,
            "{},{},{},{},{},{:.3},{}",
            csv(&footprint.ref_),
            csv(&footprint.value),
            csv(&footprint.footprint),
            number(footprint.x * scale),
            number(footprint.y * scale),
            footprint.rotation,
            side
        );
    }
    out
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
    };
    build_drawing(board, &layout, None)
        .to_pdf()
        .map_err(|error| crate::invalid(format!("cannot render the PDF preview: {error}")))
}

/// Maps board millimetres onto a drawing canvas with Y pointing down.
struct Layout {
    scale: f64,
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
}

impl Layout {
    fn point(&self, x: f64, y: f64) -> DrawPoint {
        DrawPoint::new((x - self.origin_x) * self.scale, (self.origin_y - y) * self.scale)
    }
}

/// Lowers the board onto a `document-drawing` canvas.
fn build_drawing(board: &Board, layout: &Layout, background: Option<Color>) -> Drawing {
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
            let points =
                pad_corners(footprint, pad).iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect();
            drawing.push(Element::new(Shape::Polygon { points }, Style::fill(color)));
        }
    }
    for via in &board.vias {
        drawing.push(Element::new(
            Shape::Circle {
                center: layout.point(via.at[0] * scale, via.at[1] * scale),
                radius: via.diameter * scale * layout.scale / 2.0,
            },
            Style::fill(layer_color_value(board, "F.Cu")),
        ));
    }
    drawing
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
    if !min[0].is_finite() || !min[1].is_finite() || !max[0].is_finite() || !max[1].is_finite() {
        return ([0.0, 0.0], [100.0, 60.0]);
    }
    (min, max)
}

/// Renders the composite and reports the canvas size the same viewBox used.
pub(crate) fn svg_preview(board: &Board) -> Result<(String, f64, f64), ServiceError> {
    let scale = board.millimetres();
    let mut min = [f64::INFINITY, f64::INFINITY];
    let mut max = [f64::NEG_INFINITY, f64::NEG_INFINITY];
    if let Some(outline) = board.outline {
        include(&mut min, &mut max, outline.from[0] * scale, outline.from[1] * scale);
        include(&mut min, &mut max, outline.to[0] * scale, outline.to[1] * scale);
    }
    for track in &board.tracks {
        for point in [track.from, track.to] {
            include(&mut min, &mut max, point[0] * scale, point[1] * scale);
        }
    }
    for footprint in &board.footprints {
        for pad in &footprint.pads {
            let x = (footprint.x + pad.x) * scale;
            let y = (footprint.y + pad.y) * scale;
            include(&mut min, &mut max, x - pad.w * scale / 2.0, y - pad.h * scale / 2.0);
            include(&mut min, &mut max, x + pad.w * scale / 2.0, y + pad.h * scale / 2.0);
        }
    }
    for via in &board.vias {
        include(&mut min, &mut max, via.at[0] * scale, via.at[1] * scale);
    }
    if !min[0].is_finite() || !min[1].is_finite() || !max[0].is_finite() || !max[1].is_finite() {
        min = [0.0, 0.0];
        max = [100.0, 60.0];
    }
    let margin = 5.0;
    let width = (max[0] - min[0]).max(1.0) + margin * 2.0;
    let height = (max[1] - min[1]).max(1.0) + margin * 2.0;
    let layout = Layout { scale: 1.0, origin_x: min[0] - margin, origin_y: max[1] + margin, width, height };
    let drawing = build_drawing(board, &layout, Some(Color::rgb(14, 26, 20)));
    let content = drawing.to_svg().map_err(|error| crate::invalid(format!("cannot render the SVG preview: {error}")))?;
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
/// Pad colour.
const PNG_PAD: [u8; 3] = [200, 200, 200];
/// Via colour.
const PNG_VIA: [u8; 3] = [20, 20, 20];
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
        for pad in &footprint.pads {
            pads += 1;
            let center = view
                .point([(footprint.x + pad.x * cos - pad.y * sin) * scale, (footprint.x + pad.x * sin + pad.y * cos) * scale]);
            let half_w = (cos.abs() * pad.w + sin.abs() * pad.h) / 2.0 * scale * scale_px;
            let half_h = (sin.abs() * pad.w + cos.abs() * pad.h) / 2.0 * scale * scale_px;
            let x0 = (center[0] - half_w).floor() as i32;
            let x1 = (center[0] + half_w).ceil() as i32;
            let y0 = (center[1] - half_h).floor() as i32;
            let y1 = (center[1] + half_h).ceil() as i32;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let dx = (x as f64 + 0.5 - center[0]) / scale_px;
                    let dy = -(y as f64 + 0.5 - center[1]) / scale_px;
                    let local_x = dx * cos + dy * sin;
                    let local_y = -dx * sin + dy * cos;
                    if local_x.abs() <= pad.w / 2.0 + 1e-9 && local_y.abs() <= pad.h / 2.0 + 1e-9 {
                        canvas.set(x, y, PNG_PAD);
                    }
                }
            }
        }
    }

    for via in &board.vias {
        let radius = (via.diameter * scale * scale_px / 2.0).max(0.5);
        canvas.circle(view.point([via.at[0] * scale, via.at[1] * scale]), radius, PNG_VIA);
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

/// Maps board millimetres onto image pixels with the Y axis pointing down.
struct View {
    scale: f64,
    offset_x: f64,
    offset_y: f64,
}

impl View {
    fn point(&self, point: [f64; 2]) -> [f64; 2] {
        [point[0] * self.scale + self.offset_x, PNG_HEIGHT as f64 - 1.0 - (point[1] * self.scale + self.offset_y)]
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
