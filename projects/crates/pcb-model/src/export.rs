//! Fabrication, assembly and preview writers.
//!
//! Only targets listed as `supported` in the capability matrix reach a writer;
//! everything else returns `capability/not-implemented` instead of an empty
//! file. Every writer is atomic: the file appears only after the full content
//! was written and renamed into place.

use std::{collections::BTreeMap, fmt::Write as _, fs, path::Path};

use document_drawing::{Color, Drawing, Element, Point as DrawPoint, Shape, Size, Style};
use serde_json::{Value, json};

use crate::{ServiceError, board::Board};

/// Formats this build can write.
pub(crate) const SUPPORTED_EXPORTS: &[&str] = &[
    "gbr",
    "drl",
    "drill_map",
    "dxf",
    "bom",
    "pos",
    "placement",
    "asm",
    "testpoints",
    "ipc_d_356",
    "layers",
    "stackup",
    "panel",
    "svg",
    "pdf",
    "kicad_pcb",
    "ipc2581",
    "png",
];

/// Points per millimetre used by the vector PDF writer at its 1:1 scale.
pub(crate) const PT_PER_MM: f64 = 2.834646;

/// Width and height of one A4 page in landscape orientation, in points.
const A4_LANDSCAPE_PT: [f64; 2] = [841.89, 595.28];

/// Margin kept around the board inside a PDF page, in points (10 mm).
pub(crate) const PDF_MARGIN_PT: f64 = 28.34646;

/// Height reserved at the top of a PDF page for the drawing title, in points.
pub(crate) const PDF_TITLE_BAND_PT: f64 = 38.0;

/// Dark background shared by the SVG preview canvas and its title bands.
const PREVIEW_BACKGROUND: Color = Color::rgb(14, 26, 20);

/// Preview renderers this build can produce without writing a file.
pub(crate) const SUPPORTED_PREVIEWS: &[&str] = &["svg"];

/// Gap between a footprint's pad bounding box and its silkscreen outline.
const SILK_MARGIN: f64 = 0.8;

/// Cap height of a footprint reference designator, in millimetres.
const REFERENCE_SIZE: f64 = 1.6;

/// Height of the drawing title band at the top of a sheet, in millimetres.
const SHEET_TITLE_BAND: f64 = 16.0;

/// Inset of the sheet frame from the paper edge, in millimetres.
const SHEET_FRAME_INSET: f64 = 2.0;

/// Padding around drawn content, as a fraction of the content span.
///
/// A margin of 10% of the span leaves at least 8% of the final page width on
/// each side, which keeps every sheet from hugging the paper edge.
const SHEET_PADDING_FRACTION: f64 = 0.10;

/// Smallest padding a sheet keeps, in millimetres.
const SHEET_PADDING_MIN: f64 = 4.0;

/// Line style of one legend entry, so a print in black and white stays readable.
#[derive(Clone, Copy, PartialEq, Eq)]
enum LegendMark {
    /// Filled area: copper, paste or mask.
    Area,
    /// Stroke: a track or a silkscreen outline.
    Line,
    /// Open circle: a drilled hole.
    Hole,
    /// Double line: the board outline.
    Edge,
}

/// Fields the title block prints on every drawing sheet.
struct SheetInfo<'a> {
    /// Drawing name, for example `F.Cu - Front copper`.
    name: &'a str,
    /// Drawing number; the board id is the stable choice.
    number: &'a str,
    /// Drawing scale, for example `1:1`.
    scale: &'a str,
    /// Measurement unit printed in the title block.
    units: &'a str,
    /// Layer name or board side the sheet shows.
    view: &'a str,
}

/// Pads the corners of a drawing by at least [`SHEET_PADDING_FRACTION`].
fn sheet_padding(span: f64) -> f64 {
    (span * SHEET_PADDING_FRACTION).max(SHEET_PADDING_MIN)
}

/// Estimated advance width of one monospace run at `size`, in page millimetres.
///
/// Every sheet writer measures with this factor, so a fit computed for layout
/// and a fit asserted by a test always agree. `0.6` is the advance of the
/// 10-pixel monospace cell the drawing helpers use, rounded up.
fn text_width(text: &str, size: f64) -> f64 {
    text.chars().count() as f64 * size * 0.6
}

/// Shrinks a font until its estimated width fits `max_width`.
fn fit_font(text: &str, max_width: f64, desired: f64) -> f64 {
    let width = text_width(text, desired).max(1e-6);
    if width <= max_width { desired } else { (desired * max_width / width).max(1.4) }
}

/// Splits one sentence into lines that each fit `max_width` at `size`.
///
/// Words longer than the column are split by character so a long identifier
/// still stays inside the page instead of being clipped.
fn wrap_text(text: &str, max_width: f64, size: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        while text_width(&word, size) > max_width && word.chars().count() > 1 {
            let take = ((max_width / (size * 0.6)).floor() as usize).max(1);
            let head: String = word.chars().take(take).collect();
            let tail: String = word.chars().skip(take).collect();
            if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            lines.push(head);
            word = tail;
        }
        if current.is_empty() {
            current = word;
        }
        else if text_width(&format!("{current} {word}"), size) <= max_width {
            current.push(' ');
            current.push_str(&word);
        }
        else {
            lines.push(std::mem::take(&mut current));
            current = word;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// White paper every drawing sheet is printed on.
const SHEET_PAPER: Color = Color::rgb(255, 255, 255);

/// Ink of sheet chrome: frame, title block and table rules.
const SHEET_INK_HEX: &str = "#111111";

/// One legend entry of a composed sheet: x position, swatch colour, mark and
/// label.
type LegendEntry = (f64, Option<String>, LegendMark, String);

/// Wraps one drawing in a printable sheet: white paper, frame, a title band
/// whose text stays inside its cells, the content translated below the band
/// and an optional legend under the drawing.
///
/// The content keeps its own coordinates (`0..width`, `0..height`) and is moved
/// down by the band height inside a `<g transform>`. Nothing is placed at a
/// negative coordinate, so every viewer — including ones that ignore a negative
/// `viewBox` origin — shows the whole title block.
fn compose_sheet(
    content: &str,
    width: f64,
    height: f64,
    title: &str,
    subtitle: &str,
    number: &str,
    legend: &[(Option<String>, LegendMark, String)],
) -> String {
    let Some(start) = content.find("<svg")
    else {
        return content.to_string();
    };
    let Some(offset) = content[start..].find('>')
    else {
        return content.to_string();
    };
    let root_end = start + offset;
    let Some(index) = content.rfind("</svg>")
    else {
        return content.to_string();
    };
    let band = SHEET_TITLE_BAND;
    let size = 2.6;
    let swatch = 2.6;
    let row_gap = 5.2;
    let mut legend_rows: Vec<Vec<LegendEntry>> = Vec::new();
    let mut row: Vec<LegendEntry> = Vec::new();
    let mut x = 4.0;
    for (color, mark, label) in legend {
        let entry_width = text_width(label, size) + if color.is_some() { swatch + size * 0.4 } else { 0.0 };
        if x + entry_width > width - 4.0 && !row.is_empty() {
            legend_rows.push(std::mem::take(&mut row));
            x = 4.0;
        }
        row.push((x, color.clone(), *mark, label.clone()));
        x += entry_width + size * 1.6;
    }
    if !row.is_empty() {
        legend_rows.push(row);
    }
    let legend_band = if legend_rows.is_empty() { 0.0 } else { 6.0 + legend_rows.len() as f64 * row_gap };
    let total = band + height + legend_band;
    let number_text = format!("DWG {number}");
    let number_width = text_width(&number_text, 2.4);
    let name_size = fit_font(title, (width - 14.0 - number_width).max(12.0), 4.2);
    let subtitle_size = fit_font(subtitle, width - 8.0, 2.3);
    let mut out = String::new();
    if start > 0 {
        out.push_str(&content[..start]);
    }
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {width:.3} {total:.3}\">",
        width * SVG_PREVIEW_PX_PER_MM,
        total * SVG_PREVIEW_PX_PER_MM
    );
    let _ = writeln!(out, "<rect x=\"0\" y=\"0\" width=\"{width:.3}\" height=\"{total:.3}\" fill=\"#ffffff\"/>");
    let _ = writeln!(
        out,
        "<rect x=\"{SHEET_FRAME_INSET}\" y=\"{SHEET_FRAME_INSET}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"none\" stroke=\"{SHEET_INK_HEX}\" stroke-width=\"0.25\"/>",
        width - SHEET_FRAME_INSET * 2.0,
        total - SHEET_FRAME_INSET * 2.0
    );
    let _ = writeln!(
        out,
        "<line x1=\"{SHEET_FRAME_INSET}\" y1=\"{band:.3}\" x2=\"{:.3}\" y2=\"{band:.3}\" stroke=\"{SHEET_INK_HEX}\" stroke-width=\"0.2\"/>",
        width - SHEET_FRAME_INSET
    );
    let _ = writeln!(
        out,
        "<text x=\"4\" y=\"8.600\" font-family=\"monospace\" font-size=\"{name_size:.3}\" fill=\"{SHEET_INK_HEX}\">{}</text>",
        xml(title)
    );
    let _ = writeln!(
        out,
        "<text x=\"{:.3}\" y=\"8.600\" font-family=\"monospace\" font-size=\"2.4\" fill=\"{SHEET_INK_HEX}\" text-anchor=\"end\">{}</text>",
        width - 4.0,
        xml(&number_text)
    );
    let _ = writeln!(
        out,
        "<text x=\"4\" y=\"13.900\" font-family=\"monospace\" font-size=\"{subtitle_size:.3}\" fill=\"{SHEET_INK_HEX}\">{}</text>",
        xml(subtitle)
    );
    let _ = writeln!(out, "<g class=\"sheet-body\" transform=\"translate(0,{band:.3})\">");
    out.push_str(&content[root_end + 1..index]);
    out.push_str("</g>\n");
    if legend_rows.is_empty() {
        out.push_str("</svg>\n");
        return out;
    }
    let separator = band + height;
    let _ = writeln!(
        out,
        "<line x1=\"{SHEET_FRAME_INSET}\" y1=\"{separator:.3}\" x2=\"{:.3}\" y2=\"{separator:.3}\" stroke=\"{SHEET_INK_HEX}\" stroke-width=\"0.2\"/>",
        width - SHEET_FRAME_INSET
    );
    for (row_index, entries) in legend_rows.iter().enumerate() {
        let baseline = separator + 5.0 + row_index as f64 * row_gap;
        for (x, color, mark, label) in entries {
            if let Some(color) = color {
                out.push_str(&legend_mark_svg(*x, baseline, swatch, *mark, color));
            }
            let text_x = x + if color.is_some() { swatch + size * 0.4 } else { 0.0 };
            let _ = writeln!(
                out,
                "<text x=\"{text_x:.3}\" y=\"{baseline:.3}\" font-family=\"monospace\" font-size=\"{size}\" fill=\"{SHEET_INK_HEX}\">{}</text>",
                xml(label)
            );
        }
    }
    out.push_str("</svg>\n");
    out
}

/// One legend mark drawn in page coordinates, above `baseline`.
fn legend_mark_svg(x: f64, baseline: f64, swatch: f64, mark: LegendMark, color: &str) -> String {
    let mut out = String::new();
    match mark {
        LegendMark::Area => {
            let _ = writeln!(
                out,
                "<rect x=\"{x:.3}\" y=\"{:.3}\" width=\"{swatch:.3}\" height=\"{swatch:.3}\" fill=\"{color}\" stroke=\"{SHEET_INK_HEX}\" stroke-width=\"0.15\"/>",
                baseline - swatch
            );
        }
        LegendMark::Line => {
            let _ = writeln!(
                out,
                "<line x1=\"{x:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"{color}\" stroke-width=\"0.8\"/>",
                baseline - swatch / 2.0,
                x + swatch,
                baseline - swatch / 2.0
            );
        }
        LegendMark::Hole => {
            let _ = writeln!(
                out,
                "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"0.35\"/>",
                x + swatch / 2.0,
                baseline - swatch / 2.0,
                swatch * 0.32
            );
        }
        LegendMark::Edge => {
            let _ = writeln!(
                out,
                "<line x1=\"{x:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"{color}\" stroke-width=\"0.3\"/>\n<line x1=\"{x:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"{color}\" stroke-width=\"0.3\"/>",
                baseline - swatch * 0.8,
                x + swatch,
                baseline - swatch * 0.8,
                baseline - swatch * 0.2,
                x + swatch,
                baseline - swatch * 0.2
            );
        }
    }
    out
}

/// Writes one export target and returns its report.
pub(crate) fn run(board: &Board, format: &str, path: &Path) -> Result<Value, ServiceError> {
    if format == "gbr" {
        return gerber(board, path);
    }
    if format == "asm" {
        return assembly(board, path);
    }
    if format == "layers" {
        return layer_views(board, path);
    }
    if format == "stackup" {
        return stackup_sheet(board, path);
    }
    if format == "panel" {
        return panel_sheet(board, path);
    }
    if format == "png" {
        return png(board, path);
    }
    let mut losses = Vec::new();
    let content = match format {
        "drl" => {
            let partial = board.vias.iter().filter(|via| board.via_kind(via) != "through").count();
            if partial > 0 {
                losses.push(json!({
                    "code": "drl/partial-span-via",
                    "detail": format!("{partial} blind, buried or micro via(s) are written as through holes; one Excellon file cannot carry per-hole depth, export `ipc2581` for the exact spans"),
                }));
            }
            drill(board).into_bytes()
        }
        "drill_map" => drill_map(board).into_bytes(),
        "dxf" => dxf(board)?.into_bytes(),
        "bom" => bom(board).into_bytes(),
        "pos" => position(board).into_bytes(),
        "testpoints" => testpoints(board).into_bytes(),
        "ipc_d_356" => ipc_d_356(board).into_bytes(),
        "placement" => placement(board)?.into_bytes(),
        "svg" => svg(board)?.into_bytes(),
        "pdf" => {
            let (bytes, dropped) = pdf(board)?;
            if dropped > 0 {
                losses.push(json!({
                    "code": "pdf/text-encoding",
                    "detail": format!("{dropped} character(s) outside printable ASCII were written as `?` in the PDF; export SVG for full Unicode text"),
                }));
            }
            bytes
        }
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
    let mut report = json!({
        "revision": board.revision,
        "format": format,
        "path": path.display().to_string(),
        "bytes": bytes,
        "editable": false,
    });
    if !losses.is_empty() {
        report["losses"] = json!(losses);
    }
    Ok(report)
}

/// Writes a Gerber RS-274X directory and reports each artifact.
fn gerber(board: &Board, path: &Path) -> Result<Value, ServiceError> {
    let outline = board
        .outline
        .ok_or_else(|| crate::invalid("the board has no outline; set one with `board.outline set` before exporting Gerber"))?;
    export_directory("gbr", path)?;

    let mut drafts = board.copper_stack().into_iter().map(|id| (gerber_name(id), copper(board, id))).collect::<Vec<_>>();
    drafts.push(("F_SilkS.gbr".to_string(), silkscreen(board, "F.SilkS")));
    drafts.push(("B_SilkS.gbr".to_string(), silkscreen(board, "B.SilkS")));
    drafts.push(("F_Mask.gbr".to_string(), solder_mask(board, "F.Cu")));
    drafts.push(("B_Mask.gbr".to_string(), solder_mask(board, "B.Cu")));
    drafts.push(("F_Paste.gbr".to_string(), solder_paste(board, "F.Cu")));
    drafts.push(("B_Paste.gbr".to_string(), solder_paste(board, "B.Cu")));
    drafts.push(("Edge_Cuts.gbr".to_string(), edge_cuts(outline.from, outline.to, board.millimetres())));
    if drafts.iter().any(|(_, (_, out_of_range))| *out_of_range) {
        return Err(crate::invalid(
            "a coordinate exceeds the ±9999.999999 mm range of the declared 4.6 Gerber format; keep the board inside a 10 m panel",
        ));
    }
    let files: Vec<(String, String)> = drafts.into_iter().map(|(name, (content, _))| (name, content)).collect();
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
    /// Whether any coordinate exceeds the declared `X46Y46` format.
    out_of_range: bool,
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
        for value in [from[0], from[1], to[0], to[1]] {
            self.note(value);
        }
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

    /// Flashes one aperture with optional X2 object attributes.
    fn flash_with(&mut self, definition: String, at: [f64; 2], attributes: &[String]) {
        let code = self.aperture(definition);
        self.note(at[0]);
        self.note(at[1]);
        for attribute in attributes {
            self.body.push_str(attribute);
        }
        let _ = writeln!(self.body, "D{code}*\nX{}Y{}D03*", gerber_number(at[0]), gerber_number(at[1]));
        self.close_attributes(attributes);
    }

    /// Marks a coordinate the declared `X46Y46` format cannot carry.
    fn note(&mut self, value: f64) {
        if !value.is_finite() || value.abs() > 9_999.999_999 {
            self.out_of_range = true;
        }
    }

    /// Draws one footprint pad honouring the footprint rotation.
    ///
    /// An axis-aligned footprint flashes its rectangle, swapping the aperture
    /// sides at a quarter turn; any other angle draws the rotated rectangle as
    /// a `G36` region so the copper lands where the pad actually sits.
    /// `expansion` grows every side of the rectangle in millimetres.
    fn pad(
        &mut self,
        footprint: &crate::board::Footprint,
        pad: &crate::board::Pad,
        scale: f64,
        expansion: f64,
        attributes: &[String],
    ) {
        let center = pad_center(footprint, pad);
        let width = pad.w * scale + expansion;
        let height = pad.h * scale + expansion;
        let quarter = (footprint.rotation / 90.0).round();
        let at = [center[0] * scale, center[1] * scale];
        if (footprint.rotation - quarter * 90.0).abs() <= 1e-9 {
            let (w, h) = if quarter.rem_euclid(2.0) == 1.0 { (height, width) } else { (width, height) };
            self.flash_with(format!("R,{w:.6}X{h:.6}"), at, attributes);
            return;
        }
        let angle = footprint.rotation.to_radians();
        let (sin, cos) = angle.sin_cos();
        let points = [
            [-width / 2.0, -height / 2.0],
            [width / 2.0, -height / 2.0],
            [width / 2.0, height / 2.0],
            [-width / 2.0, height / 2.0],
        ]
        .map(|[x, y]| [at[0] + x * cos - y * sin, at[1] + x * sin + y * cos]);
        self.region(&points, 1.0, attributes);
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
    /// the region. Optional X2 object attributes wrap the region like they wrap
    /// a flash.
    fn region(&mut self, points: &[[f64; 2]], scale: f64, attributes: &[String]) {
        if points.len() < 3 {
            return;
        }
        let code = self.aperture("C,0.100000".to_string());
        let scaled: Vec<[f64; 2]> = points.iter().map(|point| [point[0] * scale, point[1] * scale]).collect();
        for point in &scaled {
            self.note(point[0]);
            self.note(point[1]);
        }
        for attribute in attributes {
            self.body.push_str(attribute);
        }
        let _ = writeln!(self.body, "D{code}*\nG36*");
        for (index, point) in scaled.iter().enumerate() {
            let operation = if index == 0 { "D02" } else { "D01" };
            let _ = writeln!(self.body, "X{}Y{}{operation}*", gerber_number(point[0]), gerber_number(point[1]));
        }
        let first = scaled[0];
        let _ = writeln!(self.body, "X{}Y{}D01*\nG37*", gerber_number(first[0]), gerber_number(first[1]));
        self.close_attributes(attributes);
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
    else if file_function.starts_with("Silkscreen") {
        "ElectronicLabel"
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

/// Writes one copper layer: tracks as draws, pads as rotated flashes or regions
/// and covered vias as flashes.
///
/// Each object carries its X2 attributes: tracks and vias declare their net, and
/// a pad declares its net, component reference and pin. The block is closed with
/// `%TD*%` after the object so attributes never leak into the next one. The
/// second element of the result reports a coordinate beyond the declared format.
fn copper(board: &Board, layer: &str) -> (String, bool) {
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
            gerber.pad(footprint, pad, scale, 0.0, &pad_attributes(pad.net.as_deref(), &footprint.ref_, &pad.number));
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
        if zone.fill == "grid" {
            let definition = format!("C,{:.6}", zone.grid_width() * scale);
            for (from, to) in crate::board::grid_lines(&zone.points, zone.grid_spacing()) {
                gerber.draw(definition.clone(), [from[0] * scale, from[1] * scale], [to[0] * scale, to[1] * scale]);
            }
        }
        else {
            gerber.region(&zone.points, scale, &[]);
        }
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
    let content = gerber.render(&format!("Copper,L{},{}", index + 1, position));
    (content, gerber.out_of_range)
}

/// Writes one silkscreen layer: the document stores legend geometry as tracks
/// on the same layer ids the copper stack uses, plus legend text rendered as
/// stroke segments of the built-in 5×7 font.
fn silkscreen(board: &Board, layer: &str) -> (String, bool) {
    let scale = board.millimetres();
    let mut gerber = Gerber::default();
    for track in board.tracks.iter().filter(|track| track.layer == layer) {
        gerber.draw(
            format!("C,{:.6}", track.width * scale),
            [track.from[0] * scale, track.from[1] * scale],
            [track.to[0] * scale, track.to[1] * scale],
        );
    }
    for text in board.texts.iter().filter(|text| text.layer == layer) {
        gerber_text(&mut gerber, text, scale);
    }
    let position = if layer.starts_with("B.") { "Bot" } else { "Top" };
    let content = gerber.render(&format!("Silkscreen,{position}"));
    (content, gerber.out_of_range)
}

/// Renders one legend text as horizontal stroke runs of the 5×7 glyph bitmap.
/// The anchor is the left end of the baseline; positive rotation turns the text
/// counter-clockwise, matching the SVG and PDF projections.
fn gerber_text(gerber: &mut Gerber, text: &crate::board::TextItem, scale: f64) {
    let unit = text.size / 7.0;
    if unit <= 0.0 || unit.is_nan() {
        return;
    }
    let stroke = if text.thickness > 0.0 { text.thickness } else { unit };
    let definition = format!("C,{:.6}", stroke * scale);
    let angle = text.rotation.to_radians();
    let cos = angle.cos();
    let sin = angle.sin();
    let point = |dx: f64, dy: f64| [(text.at[0] + dx * cos + dy * sin) * scale, (text.at[1] - dx * sin + dy * cos) * scale];
    let mut cursor = 0.0;
    for character in text.text.chars() {
        let Some(rows) = glyph(character)
        else {
            cursor += 6.0 * unit;
            continue;
        };
        for (row, bits) in rows.iter().enumerate() {
            let mut column = 0;
            while column < 5 {
                if bits & (0b10000 >> column) == 0 {
                    column += 1;
                    continue;
                }
                let start = column;
                while column < 5 && bits & (0b10000 >> column) != 0 {
                    column += 1;
                }
                let dy = -(6.5 - row as f64) * unit;
                gerber.draw(
                    definition.clone(),
                    point(cursor + start as f64 * unit, dy),
                    point(cursor + column as f64 * unit, dy),
                );
            }
        }
        cursor += 6.0 * unit;
    }
}

/// Writes one solder-mask layer: every pad on the matching copper side is a
/// rectangular opening expanded by the document's solder-mask rule per side.
/// Vias stay tented, so they contribute no opening.
fn solder_mask(board: &Board, layer: &str) -> (String, bool) {
    let scale = board.millimetres();
    let expansion = board.rules.solder_mask_expansion * 2.0;
    let mut gerber = Gerber::default();
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer == layer) {
        for pad in &footprint.pads {
            gerber.pad(footprint, pad, scale, expansion, &[]);
        }
    }
    let position = if layer.starts_with("B.") { "Bot" } else { "Top" };
    let content = gerber.render(&format!("Soldermask,{position}"));
    (content, gerber.out_of_range)
}

/// Writes one solder-paste layer: every outer-layer pad opens a stencil window
/// sized exactly to the copper pad. Vias stay tented and contribute no paste.
fn solder_paste(board: &Board, layer: &str) -> (String, bool) {
    let scale = board.millimetres();
    let mut gerber = Gerber::default();
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer == layer) {
        for pad in &footprint.pads {
            gerber.pad(footprint, pad, scale, 0.0, &[]);
        }
    }
    let position = if layer.starts_with("B.") { "Bot" } else { "Top" };
    let content = gerber.render(&format!("Paste,{position}"));
    (content, gerber.out_of_range)
}

/// Writes the rectangular Edge.Cuts outline as four draws.
fn edge_cuts(from: [f64; 2], to: [f64; 2], scale: f64) -> (String, bool) {
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
    let content = gerber.render("Profile,NP");
    (content, gerber.out_of_range)
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

/// Tool symbols that mark holes on the drill map and in the drill chart.
///
/// Both drawings index the same symbol by tool number, so a reader can match a
/// bar in the chart to the marks on the map.
const DRILL_SYMBOLS: &[&str] = &["circle", "square", "diamond", "triangle", "cross", "hexagon"];

/// Name of the symbol of one zero-based tool index.
fn drill_symbol_name(index: usize) -> &'static str {
    DRILL_SYMBOLS[index % DRILL_SYMBOLS.len()]
}

/// Mark of one tool, drawn as an outline on the drill map.
fn drill_symbol_svg(x: f64, y: f64, radius: f64, index: usize) -> String {
    let radius = radius.max(0.4);
    let ink = "#111111";
    match drill_symbol_name(index) {
        "square" => format!(
            "<rect class=\"drill-hole\" x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"0.12\"/>",
            x - radius,
            y - radius,
            radius * 2.0,
            radius * 2.0
        ),
        "diamond" => format!(
            "<polygon class=\"drill-hole\" points=\"{:.3},{:.3} {:.3},{:.3} {:.3},{:.3} {:.3},{:.3}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"0.12\"/>",
            x,
            y - radius,
            x + radius,
            y,
            x,
            y + radius,
            x - radius,
            y
        ),
        "triangle" => format!(
            "<polygon class=\"drill-hole\" points=\"{:.3},{:.3} {:.3},{:.3} {:.3},{:.3}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"0.12\"/>",
            x,
            y - radius,
            x + radius * 0.9,
            y + radius * 0.8,
            x - radius * 0.9,
            y + radius * 0.8
        ),
        "cross" => format!(
            "<line class=\"drill-hole\" x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"{ink}\" stroke-width=\"0.18\"/>\n<line class=\"drill-hole\" x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"{ink}\" stroke-width=\"0.18\"/>",
            x - radius,
            y,
            x + radius,
            y,
            x,
            y - radius,
            x,
            y + radius
        ),
        "hexagon" => {
            let points = (0..6)
                .map(|corner| {
                    let angle = std::f64::consts::PI / 3.0 * corner as f64;
                    format!("{:.3},{:.3}", x + radius * angle.cos(), y + radius * angle.sin())
                })
                .collect::<Vec<_>>()
                .join(" ");
            format!("<polygon class=\"drill-hole\" points=\"{points}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"0.12\"/>")
        }
        _ => format!(
            "<circle class=\"drill-hole\" cx=\"{x:.3}\" cy=\"{y:.3}\" r=\"{radius:.3}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"0.12\"/>"
        ),
    }
}

/// Drill map with one symbol per drilled hole, a tool legend and back-drill marks.
///
/// The map is a standalone white SVG in millimetres. Every via and every drilled
/// pad contributes one plated hole, marked with the symbol of its tool, and
/// routed slots are drawn as lines. The sheet is composed like the other
/// drawings, so the title block and the legend stay on the page.
fn drill_map(board: &Board) -> String {
    let scale = board.millimetres();
    let (min, max) = match board.outline {
        Some(outline) => ([outline.from[0] * scale, outline.from[1] * scale], [outline.to[0] * scale, outline.to[1] * scale]),
        None => drawing_bounds(board),
    };
    let span_x = (max[0] - min[0]).max(1.0);
    let span_y = (max[1] - min[1]).max(1.0);
    let margin = sheet_padding(span_x.max(span_y));

    let mut vias: Vec<&crate::board::Via> = board.vias.iter().collect();
    vias.sort_by(|left, right| left.drill.total_cmp(&right.drill).then_with(|| left.id.cmp(&right.id)));

    let mut tools: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    let mut back_drilled = 0usize;
    for via in &vias {
        let diameter = via.drill * scale;
        if via.back_drill.is_some() {
            back_drilled += 1;
        }
        tools.entry(format!("{diameter:.3}")).or_insert((diameter, 0)).1 += 1;
    }
    for pad in board.placed_pads() {
        let Some(drill) = pad.drill
        else {
            continue;
        };
        let diameter = drill * scale;
        tools.entry(format!("{diameter:.3}")).or_insert((diameter, 0)).1 += 1;
    }

    let mut circles: Vec<(f64, usize)> = tools.values().copied().collect();
    circles.sort_by(|left, right| left.0.total_cmp(&right.0));
    let tool_of = |diameter: f64| circles.iter().position(|(size, _)| (size - diameter).abs() < 1e-6).unwrap_or(0);
    let mut legend: Vec<(Option<String>, LegendMark, String)> = circles
        .iter()
        .enumerate()
        .map(|(index, (diameter, count))| {
            (
                Some(SHEET_INK_HEX.to_string()),
                LegendMark::Hole,
                format!("T{} dia {:.3} mm plated x{}", index + 1, diameter, count),
            )
        })
        .collect();
    if !board.slots.is_empty() {
        legend.push((Some(SHEET_INK_HEX.to_string()), LegendMark::Line, format!("routed slots x{} (line)", board.slots.len())));
    }
    if back_drilled > 0 {
        legend.push((Some("#b03a2e".to_string()), LegendMark::Hole, format!("back drill x{back_drilled} (dashed ring)")));
    }

    let width = span_x + margin * 2.0;
    let height = span_y + margin * 2.0;
    let map_x = |x: f64| x - min[0] + margin;
    let map_y = |y: f64| y - min[1] + margin;
    let mut out = String::new();
    let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {width:.3} {height:.3}\">",
        width * SVG_PREVIEW_PX_PER_MM,
        height * SVG_PREVIEW_PX_PER_MM
    );
    let _ = writeln!(out, "<rect x=\"0\" y=\"0\" width=\"{width:.3}\" height=\"{height:.3}\" fill=\"#ffffff\"/>");
    let _ = writeln!(
        out,
        "<rect x=\"0.5\" y=\"0.5\" width=\"{:.3}\" height=\"{:.3}\" fill=\"none\" stroke=\"#8a6d1f\" stroke-width=\"0.2\"/>",
        width - 1.0,
        height - 1.0
    );
    for via in &vias {
        let diameter = via.drill * scale;
        let radius = (diameter / 2.0).max(0.35);
        out.push_str(&drill_symbol_svg(map_x(via.at[0] * scale), map_y(via.at[1] * scale), radius, tool_of(diameter)));
        if via.back_drill.is_some() {
            let _ = writeln!(
                out,
                "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"none\" stroke=\"#b03a2e\" stroke-width=\"0.1\" stroke-dasharray=\"0.4 0.3\"/>",
                map_x(via.at[0] * scale),
                map_y(via.at[1] * scale),
                radius + 0.6
            );
        }
    }
    for pad in board.placed_pads() {
        let Some(drill) = pad.drill
        else {
            continue;
        };
        let diameter = drill * scale;
        let radius = (diameter / 2.0).max(0.35);
        out.push_str(&drill_symbol_svg(map_x(pad.at[0] * scale), map_y(pad.at[1] * scale), radius, tool_of(diameter)));
    }
    for slot in &board.slots {
        let slot_width = slot.width * scale;
        let _ = writeln!(
            out,
            "<line x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"#111111\" stroke-width=\"{slot_width:.3}\" stroke-linecap=\"round\"/>",
            map_x(slot.from[0] * scale),
            map_y(slot.from[1] * scale),
            map_x(slot.to[0] * scale),
            map_y(slot.to[1] * scale)
        );
    }
    let subtitle = format!(
        "{} · SCALE 1:1 · UNITS mm · {} holes",
        board_title(board),
        board.vias.len() + board.placed_pads().into_iter().filter(|pad| pad.drill.is_some()).count()
    );
    out.push_str("</svg>\n");
    compose_sheet(&out, width, height, "OpenPCB drill map", &subtitle, &board.id, &legend)
}

/// Creates or validates one export target directory shared by the multi-file writers.
fn export_directory(format: &str, path: &Path) -> Result<(), ServiceError> {
    if path.exists() && !path.is_dir() {
        return Err(crate::invalid(format!(
            "`{}` exists and is not a directory; {format} export writes a directory of files",
            path.display()
        )));
    }
    if !path.is_dir() {
        fs::create_dir_all(path).map_err(|error| crate::invalid(format!("cannot create `{}`: {error}", path.display())))?;
    }
    Ok(())
}

/// Writes one SVG file per view into a directory and reports every artifact.
fn write_views(
    board: &Board,
    format: &str,
    path: &Path,
    views: &[(String, String)],
    render: fn(&Board, &str) -> Result<String, ServiceError>,
) -> Result<Value, ServiceError> {
    export_directory(format, path)?;
    let mut artifacts = Vec::new();
    let mut total = 0u64;
    for (name, view) in views {
        let content = render(board, view)?;
        let bytes = crate::atomic_write(&path.join(name), content.as_bytes())?;
        total += bytes;
        artifacts.push(json!({ "name": name, "bytes": bytes }));
    }
    Ok(json!({
        "revision": board.revision,
        "format": format,
        "path": path.display().to_string(),
        "bytes": total,
        "editable": false,
        "artifacts": artifacts,
    }))
}

/// Writes the board outline as an AutoCAD R12 DXF with four `LINE` entities.
///
/// The file carries the minimal header and entities sections a reader expects;
/// every corner is written in millimetres on the `Edge.Cuts` layer, so a
/// panel router or milling host can cut the board without a Gerber reader.
fn dxf(board: &Board) -> Result<String, ServiceError> {
    let outline = board
        .outline
        .ok_or_else(|| crate::invalid("the board has no outline; set one with `board.outline set` before exporting DXF"))?;
    let scale = board.millimetres();
    let corners = [
        [outline.from[0] * scale, outline.from[1] * scale],
        [outline.to[0] * scale, outline.from[1] * scale],
        [outline.to[0] * scale, outline.to[1] * scale],
        [outline.from[0] * scale, outline.to[1] * scale],
    ];
    let mut out = String::new();
    let _ = writeln!(out, "{}", dxf_pair(0, "SECTION"));
    let _ = writeln!(out, "{}", dxf_pair(2, "HEADER"));
    let _ = writeln!(out, "{}", dxf_pair(9, "$ACADVER"));
    let _ = writeln!(out, "{}", dxf_pair(1, "AC1009"));
    let _ = writeln!(out, "{}", dxf_pair(0, "ENDSEC"));
    let _ = writeln!(out, "{}", dxf_pair(0, "SECTION"));
    let _ = writeln!(out, "{}", dxf_pair(2, "ENTITIES"));
    for index in 0..4 {
        let from = corners[index];
        let to = corners[(index + 1) % 4];
        let _ = writeln!(out, "{}", dxf_pair(0, "LINE"));
        let _ = writeln!(out, "{}", dxf_pair(8, "Edge.Cuts"));
        let _ = writeln!(out, "{}", dxf_pair(10, &number(from[0])));
        let _ = writeln!(out, "{}", dxf_pair(20, &number(from[1])));
        let _ = writeln!(out, "{}", dxf_pair(30, "0.000"));
        let _ = writeln!(out, "{}", dxf_pair(11, &number(to[0])));
        let _ = writeln!(out, "{}", dxf_pair(21, &number(to[1])));
        let _ = writeln!(out, "{}", dxf_pair(31, "0.000"));
    }
    let _ = writeln!(out, "{}", dxf_pair(0, "ENDSEC"));
    let _ = writeln!(out, "{}", dxf_pair(0, "EOF"));
    Ok(out)
}

/// Writes one DXF group code and its value as the two lines the format expects.
fn dxf_pair(code: u32, value: &str) -> String {
    format!("{code:>3}\n{value}")
}

/// Writes one assembly drawing per board side and reports each artifact.
fn assembly(board: &Board, path: &Path) -> Result<Value, ServiceError> {
    let views = vec![("F_Asm.svg".to_string(), "Top".to_string()), ("B_Asm.svg".to_string(), "Bottom".to_string())];
    write_views(board, "asm", path, &views, assembly_view)
}

/// File name of one layer view: `F.Cu` becomes `F_Cu.svg`.
fn view_file(layer: &str) -> String {
    format!("{}.svg", layer.replace('.', "_"))
}

/// Writes one fabrication view per layer the document actually declares.
///
/// Copper layers follow the stack order, then every declared silkscreen,
/// soldermask, assembly and mechanical layer. Solder-paste stencils are
/// synthesized for each side that carries pads, and a drill view closes the set.
fn layer_views(board: &Board, path: &Path) -> Result<Value, ServiceError> {
    let mut views: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut push = |layer: &str, views: &mut Vec<(String, String)>| {
        let name = view_file(layer);
        if seen.insert(name.clone()) {
            views.push((name, layer.to_string()));
        }
    };
    for layer in board.copper_stack() {
        push(layer, &mut views);
    }
    for layer in &board.layers {
        if layer.kind != crate::board::LayerKind::Copper {
            push(&layer.id, &mut views);
        }
    }
    for side in ["F", "B"] {
        let prefix = format!("{side}.");
        if board.footprints.iter().any(|footprint| footprint.layer.starts_with(&prefix) && !footprint.pads.is_empty()) {
            push(&format!("{side}.Mask"), &mut views);
            push(&format!("{side}.Paste"), &mut views);
        }
    }
    push("Drill", &mut views);
    write_views(board, "layers", path, &views, layer_view)
}

/// Renders one layer in isolation on the canvas the other views share.
///
/// Copper and silkscreen views use the composite renderer restricted to one
/// layer; soldermask, paste, assembly and drill views render the objects their
/// fabrication film carries. Every view keeps the board outline and shares the
/// same canvas and coordinate mapping so a reader can stack them like Gerber
/// layers.
fn layer_view(board: &Board, view: &str) -> Result<String, ServiceError> {
    let (min, max) = preview_bounds(board);
    let margin = sheet_padding((max[0] - min[0]).max(max[1] - min[1]).max(1.0));
    let width = (max[0] - min[0]).max(1.0) + margin * 2.0;
    let height = (max[1] - min[1]).max(1.0) + margin * 2.0;
    let layout = Layout { scale: 1.0, origin_x: min[0] - margin, origin_y: min[1] - margin, width, height, y_up: false };
    let kind = board.layer(view).map(|layer| layer.kind);
    let drawing = if view.ends_with(".Mask") {
        mask_layer_drawing(board, &layout, view)
    }
    else if view.ends_with(".Paste") {
        paste_layer_drawing(board, &layout, view)
    }
    else {
        match kind {
            Some(crate::board::LayerKind::Soldermask) => mask_layer_drawing(board, &layout, view),
            Some(crate::board::LayerKind::Assembly) => assembly_layer_drawing(board, &layout, view),
            _ if view == "Drill" => drill_layer_drawing(board, &layout),
            _ => build_drawing(board, &layout, None, true, print_color, Some(view)),
        }
    };
    let content =
        drawing.to_svg().map_err(|error| crate::invalid(format!("cannot render the `{view}` layer view: {error}")))?;
    let subtitle = format!("{} · SCALE 1:1 · UNITS mm · LAYER {view}", board_title(board));
    Ok(compose_sheet(&content, width, height, &view_title(board, view), &subtitle, &board.id, &view_legend(board, view)))
}

/// Starts one single-layer drawing on white paper with the board outline.
fn layer_drawing(board: &Board, layout: &Layout) -> Drawing {
    let scale = board.millimetres();
    let mut drawing = Drawing::new(Size::new(layout.width, layout.height)).with_background(SHEET_PAPER);
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
            Style::stroke(print_color(layer_color_value(board, "Edge.Cuts")), 0.2 * layout.scale),
        ));
    }
    drawing
}

/// Color of a synthesized soldermask opening.
const MASK_INK_HEX: &str = "#7aa874";

/// Ink of one solder-mask film, declared or synthesized.
fn mask_ink(board: &Board, view: &str) -> Color {
    if board.layer(view).is_some() { print_color(layer_color_value(board, view)) } else { color_from_hex(MASK_INK_HEX) }
}

/// Renders one solder-mask film: every pad opening on the matching side.
fn mask_layer_drawing(board: &Board, layout: &Layout, view: &str) -> Drawing {
    let scale = board.millimetres();
    let side = if view.starts_with("B.") { "B." } else { "F." };
    let expansion = board.rules.solder_mask_expansion;
    let ink = mask_ink(board, view);
    let mut drawing = layer_drawing(board, layout);
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer.starts_with(side)) {
        for pad in &footprint.pads {
            let corners = pad_corners_expanded(footprint, pad, expansion);
            drawing.push(Element::new(
                Shape::Polygon {
                    points: corners.iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect(),
                },
                fill_style(ink),
            ));
        }
    }
    drawing
}

/// Renders one solder-paste stencil: every pad window on the matching side.
fn paste_layer_drawing(board: &Board, layout: &Layout, view: &str) -> Drawing {
    let scale = board.millimetres();
    let side = if view.starts_with("B.") { "B." } else { "F." };
    let copper = format!("{side}Cu");
    let ink = print_color(layer_color_value(board, &copper));
    let mut drawing = layer_drawing(board, layout);
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer == copper) {
        for pad in &footprint.pads {
            let corners = pad_corners(footprint, pad);
            drawing.push(Element::new(
                Shape::Polygon {
                    points: corners.iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect(),
                },
                fill_style(ink),
            ));
        }
    }
    drawing
}

/// Renders one assembly layer: pads as outlines, silkscreen boxes and references.
fn assembly_layer_drawing(board: &Board, layout: &Layout, view: &str) -> Drawing {
    let scale = board.millimetres();
    let side = if view.starts_with("B.") { "B." } else { "F." };
    let ink = print_color(layer_color_value(board, view));
    let mut drawing = layer_drawing(board, layout);
    for footprint in board.footprints.iter().filter(|footprint| footprint.layer.starts_with(side)) {
        for pad in &footprint.pads {
            let corners = pad_corners(footprint, pad);
            drawing.push(Element::new(
                Shape::Polygon {
                    points: corners.iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect(),
                },
                Style::stroke(ink, 0.12 * layout.scale),
            ));
        }
        let Some((min, max)) = footprint_silk(footprint)
        else {
            continue;
        };
        let corners = [[min[0], min[1]], [max[0], min[1]], [max[0], max[1]], [min[0], max[1]]];
        drawing.push(Element::new(
            Shape::Polygon {
                points: corners.iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect(),
            },
            Style::stroke(ink, 0.15 * layout.scale),
        ));
        if !footprint.ref_.is_empty() {
            drawing.push(Element::new(
                Shape::Text {
                    at: layout.point((min[0] - 0.6) * scale, ((min[1] + max[1]) / 2.0 + 0.7) * scale),
                    text: footprint.ref_.clone(),
                    size: REFERENCE_SIZE * layout.scale,
                },
                fill_style(ink),
            ));
        }
    }
    drawing
}

/// Renders one drill film: vias, drilled pads, routed slots and back-drill rings.
fn drill_layer_drawing(board: &Board, layout: &Layout) -> Drawing {
    let scale = board.millimetres();
    let mut drawing = layer_drawing(board, layout);
    let hole_ink = Color::rgb(17, 17, 17);
    let mut holes: Vec<[f64; 3]> = Vec::new();
    for via in &board.vias {
        holes.push([via.at[0], via.at[1], via.drill]);
    }
    for pad in board.placed_pads() {
        if let Some(drill) = pad.drill.filter(|drill| *drill > 0.0) {
            holes.push([pad.at[0], pad.at[1], drill]);
        }
    }
    for [x, y, drill] in holes {
        drawing.push(Element::new(
            Shape::Circle { center: layout.point(x * scale, y * scale), radius: (drill * scale / 2.0).max(0.15) },
            fill_style(hole_ink),
        ));
    }
    for slot in &board.slots {
        drawing.push(Element::new(
            Shape::Line {
                from: layout.point(slot.from[0] * scale, slot.from[1] * scale),
                to: layout.point(slot.to[0] * scale, slot.to[1] * scale),
            },
            Style::stroke(hole_ink, slot.width * scale),
        ));
    }
    for via in board.vias.iter().filter(|via| via.back_drill.is_some()) {
        let radius = (via.drill * scale / 2.0).max(0.15) + 0.5;
        drawing.push(Element::new(
            Shape::Circle { center: layout.point(via.at[0] * scale, via.at[1] * scale), radius },
            Style::stroke(Color::rgb(176, 58, 46), 0.15),
        ));
    }
    drawing
}

#[derive(Clone, Copy)]
struct Box2 {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

impl Box2 {
    fn overlaps(&self, other: &Box2) -> bool {
        self.x < other.x + other.w + 0.3
            && other.x < self.x + self.w + 0.3
            && self.y < other.y + other.h + 0.3
            && other.y < self.y + self.h + 0.3
    }
}

/// Renders one assembly sheet: outline, component outlines, references,
/// polarity marks, BOM item bubbles and the BOM table below the board.
///
/// Coordinates are content-page millimetres; the shared sheet composer adds
/// the frame and title band. The board is scaled to fill at least 60% of the
/// page width, references move around their component to the first of eight
/// candidate positions that collides with nothing, and the BOM columns are
/// measured from their content, so nothing leaves the page.
fn assembly_view(board: &Board, side: &str) -> Result<String, ServiceError> {
    let scale = board.millimetres();
    let bottom = side == "Bottom";
    let parts: Vec<&crate::board::Footprint> =
        board.footprints.iter().filter(|footprint| footprint.layer.starts_with(if bottom { "B." } else { "F." })).collect();

    let mut groups: BTreeMap<(String, String), Vec<&str>> = BTreeMap::new();
    for footprint in &board.footprints {
        groups.entry((footprint.value.clone(), footprint.footprint.clone())).or_default().push(footprint.ref_.as_str());
    }
    let item_of: BTreeMap<(String, String), usize> =
        groups.keys().enumerate().map(|(index, key)| (key.clone(), index + 1)).collect();

    let (mut min, mut max) = match board.outline {
        Some(outline) => ([outline.from[0] * scale, outline.from[1] * scale], [outline.to[0] * scale, outline.to[1] * scale]),
        None => drawing_bounds(board),
    };
    for footprint in &parts {
        if let Some((silk_min, silk_max)) = footprint_silk(footprint) {
            include(&mut min, &mut max, (silk_min[0] - 0.5) * scale, (silk_min[1] - 0.5) * scale);
            include(&mut min, &mut max, (silk_max[0] + 0.5) * scale, (silk_max[1] + 0.5) * scale);
        }
    }
    let span_x = (max[0] - min[0]).max(1.0);
    let span_y = (max[1] - min[1]).max(1.0);
    let margin = 8.0;
    let width = (span_x * 1.25 + margin * 2.0).max(150.0);
    let fit = ((width - margin * 2.0) / span_x).max(0.1);
    let board_height = span_y * fit;
    let map_x = |x: f64| margin + (x - min[0]) * fit;
    let map_y = |y: f64| margin + (y - min[1]) * fit;

    // Bill-of-materials table geometry, measured from the content.
    let headers = ["Item", "Qty", "Value", "Footprint", "Designators"];
    let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
    for (index, ((value, package), designators)) in groups.iter().enumerate() {
        let mut designators = designators.clone();
        designators.sort_unstable();
        rows.push((
            index + 1,
            vec![(index + 1).to_string(), designators.len().to_string(), value.clone(), package.clone(), designators.join(" ")],
        ));
    }
    let table_width = width - margin * 2.0;
    let mut font = 2.4;
    let gap = 4.0;
    let mut designator_lines: Vec<Vec<String>>;
    let (widths, designator_lines) = loop {
        let fixed: f64 = headers
            .iter()
            .take(4)
            .zip(0..4)
            .map(|(header, column)| {
                rows.iter().map(|row| text_width(&row.1[column], font)).fold(text_width(header, font), f64::max) + 3.0
            })
            .sum::<f64>()
            + gap * 4.0;
        let designator_width = (table_width - fixed).max(16.0);
        designator_lines = rows.iter().map(|row| wrap_text(&row.1[4], designator_width, font)).collect();
        let mut widths = vec![0.0_f64; headers.len()];
        for (column, header) in headers.iter().enumerate() {
            let content = if column == 4 {
                designator_lines
                    .iter()
                    .map(|lines| lines.iter().map(|line| text_width(line, font)).fold(0.0, f64::max))
                    .fold(0.0, f64::max)
            }
            else {
                rows.iter().map(|row| text_width(&row.1[column], font)).fold(0.0, f64::max)
            };
            widths[column] = content.max(text_width(header, font)) + 3.0;
        }
        let total = widths.iter().sum::<f64>() + gap * 4.0;
        if total <= table_width || font <= 1.8 {
            break (widths, designator_lines);
        }
        font *= table_width / total;
    };
    let mut column_x = Vec::with_capacity(widths.len());
    let mut cursor = margin;
    for (column, column_width) in widths.iter().enumerate() {
        column_x.push(cursor);
        cursor += column_width + if column + 1 < widths.len() { gap } else { 0.0 };
    }
    let row_heights: Vec<f64> = designator_lines.iter().map(|lines| lines.len().max(1) as f64 * (font * 1.7) + 1.4).collect();
    let table_height = 7.0 + row_heights.iter().sum::<f64>() + 3.0;
    let content_height = margin + board_height + 12.0 + table_height + margin;

    let mut out = String::new();
    let _ = writeln!(out, "<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.0}\" height=\"{:.0}\" viewBox=\"0 0 {width:.3} {content_height:.3}\">",
        width * SVG_PREVIEW_PX_PER_MM,
        content_height * SVG_PREVIEW_PX_PER_MM
    );
    let _ = writeln!(out, "<rect x=\"0\" y=\"0\" width=\"{width:.3}\" height=\"{content_height:.3}\" fill=\"#ffffff\"/>");
    if let Some(outline) = board.outline {
        let corners = [
            (outline.from[0] * scale, outline.from[1] * scale),
            (outline.to[0] * scale, outline.from[1] * scale),
            (outline.to[0] * scale, outline.to[1] * scale),
            (outline.from[0] * scale, outline.to[1] * scale),
        ];
        let points = corners.iter().map(|(x, y)| format!("{:.3},{:.3}", map_x(*x), map_y(*y))).collect::<Vec<_>>().join(" ");
        let _ = writeln!(out, "<polygon points=\"{points}\" fill=\"none\" stroke=\"#8a6d1f\" stroke-width=\"0.3\"/>");
    }

    let mut occupied: Vec<Box2> = Vec::new();
    let mut bubbles: Vec<(&crate::board::Footprint, usize, f64, f64)> = Vec::new();
    for footprint in &parts {
        for pad in &footprint.pads {
            let points = pad_corners(footprint, pad)
                .iter()
                .map(|corner| format!("{:.3},{:.3}", map_x(corner[0] * scale), map_y(corner[1] * scale)))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = writeln!(out, "<polygon points=\"{points}\" fill=\"none\" stroke=\"#111111\" stroke-width=\"0.12\"/>");
            if let Some(drill) = pad.drill.filter(|drill| *drill > 0.0) {
                let center = pad_center(footprint, pad);
                let _ = writeln!(
                    out,
                    "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"{:.3}\" fill=\"none\" stroke=\"#111111\" stroke-width=\"0.1\"/>",
                    map_x(center[0] * scale),
                    map_y(center[1] * scale),
                    drill * scale * fit / 2.0
                );
            }
        }
        let Some((silk_min, silk_max)) = footprint_silk(footprint)
        else {
            continue;
        };
        let corners = [
            (silk_min[0] * scale, silk_min[1] * scale),
            (silk_max[0] * scale, silk_min[1] * scale),
            (silk_max[0] * scale, silk_max[1] * scale),
            (silk_min[0] * scale, silk_max[1] * scale),
        ];
        let points = corners.iter().map(|(x, y)| format!("{:.3},{:.3}", map_x(*x), map_y(*y))).collect::<Vec<_>>().join(" ");
        let _ = writeln!(out, "<polygon points=\"{points}\" fill=\"none\" stroke=\"#111111\" stroke-width=\"0.18\"/>");
        let boxed = Box2 {
            x: map_x(silk_min[0] * scale),
            y: map_y(silk_min[1] * scale),
            w: (silk_max[0] - silk_min[0]) * scale * fit,
            h: (silk_max[1] - silk_min[1]) * scale * fit,
        };
        occupied.push(boxed);
        if footprint.pads.iter().any(|pad| pad.number == "1") {
            let _ = writeln!(
                out,
                "<circle cx=\"{:.3}\" cy=\"{:.3}\" r=\"0.45\" fill=\"#111111\"/>",
                map_x((silk_min[0] - 0.6) * scale),
                map_y((silk_max[1] + 0.6) * scale)
            );
        }
        if let Some(item) = item_of.get(&(footprint.value.clone(), footprint.footprint.clone())) {
            let cx = map_x((silk_max[0] + 1.8) * scale);
            let cy = map_y((silk_max[1] + 1.8) * scale);
            bubbles.push((footprint, *item, cx, cy));
            occupied.push(Box2 { x: cx - 1.6, y: cy - 1.6, w: 3.2, h: 3.2 });
        }
    }
    for (_, item, cx, cy) in &bubbles {
        let _ = writeln!(
            out,
            "<circle cx=\"{cx:.3}\" cy=\"{cy:.3}\" r=\"1.5\" fill=\"#ffffff\" stroke=\"#111111\" stroke-width=\"0.12\"/>"
        );
        let _ = writeln!(
            out,
            "<text x=\"{cx:.3}\" y=\"{:.3}\" font-family=\"monospace\" font-size=\"1.6\" fill=\"#111111\" text-anchor=\"middle\">{item}</text>",
            cy + 0.55
        );
    }

    // Place every reference at the first collision-free candidate around its
    // component; a label that has to move further out gains a leader line.
    let label_size = 2.6;
    let label_height = label_size + 0.9;
    for footprint in &parts {
        if footprint.ref_.is_empty() {
            continue;
        }
        let Some((silk_min, silk_max)) = footprint_silk(footprint)
        else {
            continue;
        };
        let x0 = map_x(silk_min[0] * scale);
        let x1 = map_x(silk_max[0] * scale);
        let y0 = map_y(silk_min[1] * scale);
        let y1 = map_y(silk_max[1] * scale);
        let label_width = text_width(&footprint.ref_, label_size) + 0.6;
        let candidates = [
            (x0, y0 - label_height - 0.6),
            (x1 - label_width, y0 - label_height - 0.6),
            ((x0 + x1) / 2.0 - label_width / 2.0, y0 - label_height - 0.6),
            (x0, y1 + 0.6),
            (x1 - label_width, y1 + 0.6),
            (x0 - label_width - 0.6, (y0 + y1) / 2.0 - label_height / 2.0),
            (x1 + 0.6, (y0 + y1) / 2.0 - label_height / 2.0),
            ((x0 + x1) / 2.0 - label_width / 2.0, y0 - label_height - 3.4),
        ];
        let in_page = |candidate: Box2| {
            candidate.x >= margin - 1.0
                && candidate.x + candidate.w <= width - margin + 1.0
                && candidate.y >= 2.0
                && candidate.y + candidate.h <= margin + board_height
        };
        let mut placed = candidates
            .iter()
            .map(|(x, y)| Box2 { x: *x, y: *y, w: label_width, h: label_height })
            .find(|candidate| in_page(*candidate) && occupied.iter().all(|other| !candidate.overlaps(other)));
        let mut leader = None;
        if placed.is_none() {
            let x = (x0 + x1) / 2.0 - label_width / 2.0;
            let y = (y0 - label_height - 3.4).max(2.0);
            let candidate = Box2 { x, y, w: label_width, h: label_height };
            leader = Some((x + label_width / 2.0, y + label_height, (x0 + x1) / 2.0, y0));
            placed = Some(candidate);
        }
        let Some(placed) = placed
        else {
            continue;
        };
        if let Some((fx, fy, tx, ty)) = leader {
            let _ = writeln!(
                out,
                "<line x1=\"{fx:.3}\" y1=\"{fy:.3}\" x2=\"{tx:.3}\" y2=\"{ty:.3}\" stroke=\"#111111\" stroke-width=\"0.1\"/>"
            );
        }
        let _ = writeln!(
            out,
            "<rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"#ffffff\"/>",
            placed.x - 0.2,
            placed.y,
            placed.w + 0.4,
            placed.h
        );
        let _ = writeln!(
            out,
            "<text x=\"{:.3}\" y=\"{:.3}\" font-family=\"monospace\" font-size=\"{label_size}\" fill=\"#111111\">{}</text>",
            placed.x,
            placed.y + label_size,
            xml(&footprint.ref_)
        );
        occupied.push(placed);
    }

    // Bill of materials below the drawing, aligned per measured column.
    let table_top = margin + board_height + 12.0;
    let _ = writeln!(
        out,
        "<line x1=\"{margin:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"#111111\" stroke-width=\"0.2\"/>",
        table_top - 4.0,
        width - margin,
        table_top - 4.0
    );
    for (column, header) in headers.iter().enumerate() {
        let (x, anchor) =
            if column < 2 { (column_x[column] + widths[column] - 1.0, "end") } else { (column_x[column], "start") };
        let _ = writeln!(
            out,
            "<text x=\"{x:.3}\" y=\"{:.3}\" font-family=\"monospace\" font-size=\"{font:.3}\" fill=\"#111111\" text-anchor=\"{anchor}\">{header}</text>",
            table_top
        );
    }
    let _ = writeln!(
        out,
        "<line x1=\"{margin:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\" stroke=\"#111111\" stroke-width=\"0.15\"/>",
        table_top + 2.4,
        width - margin,
        table_top + 2.4
    );
    let mut y = table_top + 2.4;
    for (index, (_, cells)) in rows.iter().enumerate() {
        let lines = &designator_lines[index];
        for column in 0..4 {
            if cells[column].is_empty() {
                continue;
            }
            let (x, anchor) =
                if column < 2 { (column_x[column] + widths[column] - 1.0, "end") } else { (column_x[column], "start") };
            let _ = writeln!(
                out,
                "<text x=\"{x:.3}\" y=\"{:.3}\" font-family=\"monospace\" font-size=\"{font:.3}\" fill=\"#111111\" text-anchor=\"{anchor}\">{}</text>",
                y + font * 1.5,
                xml(&cells[column])
            );
        }
        for (line, text) in lines.iter().enumerate() {
            let _ = writeln!(
                out,
                "<text x=\"{:.3}\" y=\"{:.3}\" font-family=\"monospace\" font-size=\"{font:.3}\" fill=\"#111111\">{}</text>",
                column_x[4],
                y + font * 1.5 + line as f64 * font * 1.7,
                xml(text)
            );
        }
        y += row_heights[index];
    }
    let _ = writeln!(
        out,
        "<line x1=\"{margin:.3}\" y1=\"{y:.3}\" x2=\"{:.3}\" y2=\"{y:.3}\" stroke=\"#111111\" stroke-width=\"0.2\"/>",
        width - margin
    );

    let subtitle = format!("{} · SCALE NTS · UNITS mm · {side} side", board_title(board));
    out.push_str("</svg>\n");
    Ok(compose_sheet(&out, width, content_height, &format!("Assembly drawing - {side} side"), &subtitle, &board.id, &[]))
}

/// Human name of one layer view: a layer id, a paste film or the drill map.
fn view_title(board: &Board, view: &str) -> String {
    if view == "Drill" {
        return "Drill map".to_string();
    }
    if view.ends_with(".Paste") {
        return format!("Solder paste stencil - {} side", side_word(view));
    }
    if view.ends_with(".Mask") && board.layer(view).is_none() {
        return format!("{view} - {} solder mask", side_word(view));
    }
    let kind = board.layer(view).map(|layer| layer.kind);
    match kind {
        Some(crate::board::LayerKind::Copper) => format!("{view} - {} copper", side_word(view)),
        Some(crate::board::LayerKind::Silkscreen) => format!("{view} - {} silkscreen", side_word(view)),
        Some(crate::board::LayerKind::Soldermask) => format!("{view} - {} solder mask", side_word(view)),
        Some(crate::board::LayerKind::Assembly) => format!("{view} - {} assembly", side_word(view)),
        Some(crate::board::LayerKind::Mechanical) if view == "Edge.Cuts" => "Edge.Cuts - board outline".to_string(),
        Some(crate::board::LayerKind::Mechanical) => format!("{view} - mechanical"),
        None => format!("{view} - layer"),
    }
}

/// `Front` for an `F.` layer, `Back` for a `B.` layer, `Inner` otherwise.
fn side_word(layer: &str) -> &'static str {
    if layer.starts_with("B.") {
        "Back"
    }
    else if layer.starts_with("F.") {
        "Front"
    }
    else {
        "Inner"
    }
}

/// Legend entries a single-layer view actually uses.
fn view_legend(board: &Board, view: &str) -> Vec<(Option<String>, LegendMark, String)> {
    if view == "Drill" {
        let mut items = vec![(Some("#e8e8e8".to_string()), LegendMark::Hole, "Drill hole".to_string())];
        if board.vias.iter().any(|via| via.back_drill.is_some()) {
            items.push((Some("#b03a2e".to_string()), LegendMark::Hole, "Back drill (dashed)".to_string()));
        }
        return items;
    }
    if view.ends_with(".Paste") {
        return vec![(
            Some(layer_color(board, &format!("{}Cu", if view.starts_with("B.") { "B." } else { "F." }))),
            LegendMark::Area,
            format!("{view} window"),
        )];
    }
    if view.ends_with(".Mask") && board.layer(view).is_none() {
        return vec![(Some(MASK_INK_HEX.to_string()), LegendMark::Area, format!("{view} opening"))];
    }
    let color = layer_color(board, view);
    match board.layer(view).map(|layer| layer.kind) {
        Some(crate::board::LayerKind::Soldermask) => vec![(Some(color), LegendMark::Area, format!("{view} opening"))],
        Some(crate::board::LayerKind::Assembly) => vec![(Some(color), LegendMark::Line, format!("{view} outline"))],
        Some(crate::board::LayerKind::Mechanical) => vec![(Some(color), LegendMark::Edge, "Board outline".to_string())],
        _ => vec![(Some(color), LegendMark::Line, format!("{view} geometry"))],
    }
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

/// Top and bottom composite of tracks, pads and vias with a title block.
fn svg(board: &Board) -> Result<String, ServiceError> {
    let (content, width, height) = svg_preview_with(board, Some(SHEET_PAPER), print_color)?;
    Ok(compose_sheet(
        &content,
        width,
        height,
        board_title(board),
        "SCALE 1:1 · UNITS mm · COMPOSITE · COPPER + SILKSCREEN",
        &board.id,
        &legend_items(board),
    ))
}

/// Drawing title: the board title, or the board id when the document has none.
fn board_title(board: &Board) -> &str {
    board.title.as_deref().filter(|title| !title.is_empty()).unwrap_or(board.id.as_str())
}

/// Whether one layer carries any object the composite preview draws.
fn layer_has_content(board: &Board, layer: &str) -> bool {
    board.tracks.iter().any(|track| track.layer == layer)
        || board.zones.iter().any(|zone| zone.layer == layer)
        || board.texts.iter().any(|text| text.layer == layer)
        || board.footprints.iter().any(|footprint| footprint.layer == layer || silk_layer(&footprint.layer) == layer)
}

/// Whether the board has any hole or slot to map.
fn has_drills(board: &Board) -> bool {
    !board.vias.is_empty() || !board.slots.is_empty() || board.placed_pads().into_iter().any(|pad| pad.drill.is_some())
}

/// Legend entries for the title block: swatch colour, line style and label.
///
/// Only styles the drawing actually uses are listed. The line mark keeps the
/// entries apart when the sheet is printed in black and white. The board size
/// line follows when the document declares an outline.
fn legend_items(board: &Board) -> Vec<(Option<String>, LegendMark, String)> {
    let mut items = Vec::new();
    for layer in board.copper_stack() {
        if layer_has_content(board, layer) {
            items.push((Some(layer_color(board, layer)), LegendMark::Line, format!("{layer} copper")));
        }
    }
    for layer in &board.layers {
        if layer.kind == crate::board::LayerKind::Silkscreen && layer_has_content(board, &layer.id) {
            items.push((Some(layer_color(board, &layer.id)), LegendMark::Line, format!("{} silkscreen", layer.id)));
        }
    }
    if has_drills(board) {
        items.push((Some("#e8e8e8".to_string()), LegendMark::Hole, "Drill hole".to_string()));
    }
    if let Some(outline) = board.outline {
        items.push((Some(layer_color(board, "Edge.Cuts")), LegendMark::Edge, "Edge cuts".to_string()));
        let scale = board.millimetres();
        let width = (outline.to[0] - outline.from[0]) * scale;
        let height = (outline.to[1] - outline.from[1]) * scale;
        items.push((None, LegendMark::Area, format!("Board: {} x {} mm", trim_mm(width), trim_mm(height))));
    }
    items
}

/// One placed legend item: x, row offset, optional colour, line style, label.
type LegendItem = (f64, f64, Option<String>, LegendMark, String);

/// A laid-out legend band: placed items, band height, swatch and font sizes.
struct Legend {
    items: Vec<LegendItem>,
    band: f64,
    size: f64,
    swatch: f64,
}

/// Lays the legend items out left to right, wrapping at the canvas edge.
///
/// The band height fits every row with padding above the first baseline and
/// below the last one, all in the caller's drawing units.
fn layout_legend(board: &Board, width: f64, inset: f64, size: f64, swatch: f64, row_gap: f64) -> Legend {
    let mut items = Vec::new();
    let mut x = inset;
    let mut row = 0.0;
    for (color, mark, label) in legend_items(board) {
        let label_width = label.chars().count() as f64 * size * 0.6;
        let swatch_width = if color.is_some() { swatch + size * 0.4 } else { 0.0 };
        let item_width = swatch_width + label_width;
        if x + item_width > width - inset && x > inset {
            x = inset;
            row += row_gap;
        }
        items.push((x, row, color, mark, label));
        x += item_width + size * 1.6;
    }
    let pad = size * 2.0;
    Legend { items, band: pad + row + pad, size, swatch }
}

/// Formats a millimetre size without a trailing `.0`.
fn trim_mm(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded == rounded.round() { (rounded as i64).to_string() } else { format!("{rounded:.1}") }
}

/// Single-page vector PDF sized to A4 landscape or to the board itself.
///
/// The page uses point coordinates (1 mm = 2.834646 pt). `document-drawing`
/// owns the geometry lowering and the PDF backend, so this writer only places
/// the same outline, tracks, pads and vias the preview draws, plus a title
/// band at the top and a layer legend band at the bottom. The board is scaled
/// to fill the printable area between those bands so a small board stays
/// legible, while a board larger than A4 grows the page instead of shrinking.
/// Renders the board as a one-page PDF and counts the characters outside
/// printable ASCII that the base-14 fonts degrade to `?`.
fn pdf(board: &Board) -> Result<(Vec<u8>, usize), ServiceError> {
    let page = pdf_page(board);
    let layout = Layout {
        scale: page.scale,
        origin_x: page.origin_x,
        origin_y: page.origin_y,
        width: page.width,
        height: page.height,
        y_up: true,
    };
    let mut drawing = build_drawing(board, &layout, None, true, print_color, None);
    draw_pdf_title_block(board, &mut drawing, page.width, page.height, &page.legend);
    let dropped = drawing
        .elements
        .iter()
        .filter_map(|element| match &element.shape {
            Shape::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .flat_map(|text| text.chars())
        .filter(|character| !character.is_ascii())
        .count();
    let bytes = drawing.to_pdf().map_err(|error| crate::invalid(format!("cannot render the PDF preview: {error}")))?;
    Ok((bytes, dropped))
}

/// Planned geometry of one PDF page, in points.
///
/// `board_*` is the scaled drawing box already centred inside the printable
/// region, which excludes the title band at the top, the legend band at the
/// bottom and the page margin on every side. The crate-internal fields are
/// readable by the regression tests that check the fit.
#[allow(dead_code, reason = "the fit fields and helpers are asserted by the crate tests")]
pub(crate) struct PdfPage {
    pub(crate) width: f64,
    pub(crate) height: f64,
    /// Points per board millimetre applied to the drawing.
    pub(crate) scale: f64,
    origin_x: f64,
    origin_y: f64,
    pub(crate) board_width: f64,
    pub(crate) board_height: f64,
    pub(crate) board_left: f64,
    pub(crate) board_bottom: f64,
    pub(crate) legend_band: f64,
    legend: Legend,
}

#[allow(dead_code, reason = "the fit fields and helpers are asserted by the crate tests")]
impl PdfPage {
    /// Width of the area the board may fill, in points.
    pub(crate) fn printable_width(&self) -> f64 {
        (self.width - PDF_MARGIN_PT * 2.0).max(1.0)
    }

    /// Height of the area between the title band and the legend band, in points.
    pub(crate) fn printable_height(&self) -> f64 {
        (self.height - PDF_MARGIN_PT * 2.0 - PDF_TITLE_BAND_PT - self.legend_band).max(1.0)
    }
}

/// Plans the PDF page and the board placement inside it.
///
/// The page stays A4 landscape unless the board does not fit at one point per
/// millimetre, in which case it grows to hold the board plus the margins and
/// both bands. The board is then scaled to fill the printable area and centred
/// in both directions, so a 72 x 52 mm board uses the page instead of a small
/// patch in its middle. The fit uses [`preview_bounds`], so silkscreen text
/// and reference designators stay clear of the title and legend bands.
pub(crate) fn pdf_page(board: &Board) -> PdfPage {
    let (min, max) = preview_bounds(board);
    let width_mm = (max[0] - min[0]).max(1e-6);
    let height_mm = (max[1] - min[1]).max(1e-6);
    let page_width = A4_LANDSCAPE_PT[0].max(width_mm * PT_PER_MM + PDF_MARGIN_PT * 2.0);
    let legend = layout_legend(board, page_width, PDF_MARGIN_PT, 8.0, 6.0, 11.0);
    let page_height = A4_LANDSCAPE_PT[1].max(height_mm * PT_PER_MM + PDF_MARGIN_PT * 2.0 + PDF_TITLE_BAND_PT + legend.band);
    let printable_width = (page_width - PDF_MARGIN_PT * 2.0).max(1.0);
    let printable_height = (page_height - PDF_MARGIN_PT * 2.0 - PDF_TITLE_BAND_PT - legend.band).max(1.0);
    let scale = (printable_width / width_mm).min(printable_height / height_mm).max(PT_PER_MM);
    let board_width = width_mm * scale;
    let board_height = height_mm * scale;
    let board_left = (page_width - board_width) / 2.0;
    let board_bottom = legend.band + PDF_MARGIN_PT + (printable_height - board_height) / 2.0;
    PdfPage {
        width: page_width,
        height: page_height,
        scale,
        origin_x: min[0] - board_left / scale,
        origin_y: max[1] + board_bottom / scale,
        board_width,
        board_height,
        board_left,
        board_bottom,
        legend_band: legend.band,
        legend,
    }
}

/// Places the frame, title block and layer legend on a PDF page.
///
/// Page coordinates are points with Y growing up, so the title sits inside
/// the band reserved at the top and the legend rows descend from the band
/// reserved at the bottom. The legend lists only the styles the sheet uses.
fn draw_pdf_title_block(board: &Board, drawing: &mut Drawing, page_width: f64, page_height: f64, legend: &Legend) {
    let inset = 6.0;
    let frame =
        [[inset, inset], [page_width - inset, inset], [page_width - inset, page_height - inset], [inset, page_height - inset]];
    drawing.push(Element::new(
        Shape::Polygon { points: frame.iter().map(|corner| DrawPoint::new(corner[0], corner[1])).collect() },
        Style::stroke(PRINT_INK, 0.8),
    ));
    let band_bottom = page_height - PDF_TITLE_BAND_PT;
    drawing.push(Element::new(
        Shape::Line { from: DrawPoint::new(inset, band_bottom), to: DrawPoint::new(page_width - inset, band_bottom) },
        Style::stroke(PRINT_INK, 0.6),
    ));
    let title_size = 13.0;
    let title = board_title(board);
    let title_font = fit_font(title, (page_width - inset * 2.0) * 0.6, title_size);
    drawing.push(Element::new(
        Shape::Text { at: DrawPoint::new(inset + 2.0, page_height - 14.0), text: title.to_string(), size: title_font },
        fill_style(print_color(layer_color_value(board, "Edge.Cuts"))),
    ));
    let number = format!("DWG {}", board.id);
    let number_width = number.chars().count() as f64 * 8.0 * 0.6;
    drawing.push(Element::new(
        Shape::Text {
            at: DrawPoint::new(page_width - inset - 2.0 - number_width, page_height - 14.0),
            text: number,
            size: 8.0,
        },
        fill_style(PRINT_INK),
    ));
    drawing.push(Element::new(
        Shape::Text {
            at: DrawPoint::new(inset + 2.0, page_height - 26.0),
            text: "SCALE 1:1 | UNITS mm | COMPOSITE | COPPER + SILKSCREEN".to_string(),
            size: 7.0,
        },
        fill_style(PRINT_INK),
    ));
    for (x, row, color, mark, label) in &legend.items {
        let y = legend.band - legend.size * 2.0 - row;
        let swatch = legend.swatch;
        if let Some(color) = color {
            let ink = print_color(color_from_hex(color));
            match mark {
                LegendMark::Area => {
                    let corners =
                        [[*x, y + 1.0], [x + swatch, y + 1.0], [x + swatch, y - swatch + 1.0], [*x, y - swatch + 1.0]];
                    drawing.push(Element::new(
                        Shape::Polygon { points: corners.iter().map(|corner| DrawPoint::new(corner[0], corner[1])).collect() },
                        fill_style(ink),
                    ));
                }
                LegendMark::Line => {
                    drawing.push(Element::new(
                        Shape::Line {
                            from: DrawPoint::new(*x, y - swatch / 2.0),
                            to: DrawPoint::new(x + swatch, y - swatch / 2.0),
                        },
                        Style::stroke(ink, 2.0),
                    ));
                }
                LegendMark::Hole => {
                    drawing.push(Element::new(
                        Shape::Circle { center: DrawPoint::new(x + swatch / 2.0, y - swatch / 2.0), radius: swatch * 0.32 },
                        Style::stroke(ink, 1.2),
                    ));
                }
                LegendMark::Edge => {
                    for offset in [swatch * 0.8, swatch * 0.2] {
                        drawing.push(Element::new(
                            Shape::Line { from: DrawPoint::new(*x, y - offset), to: DrawPoint::new(x + swatch, y - offset) },
                            Style::stroke(ink, 0.8),
                        ));
                    }
                }
            }
        }
        let text_x = x + if color.is_some() { legend.swatch + legend.size * 0.4 } else { 0.0 };
        drawing.push(Element::new(
            Shape::Text { at: DrawPoint::new(text_x, y), text: label.clone(), size: legend.size },
            fill_style(DRILL_COLOR),
        ));
    }
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
/// itself so it can rotate its `<text>` elements, while the PDF writer takes
/// them here and lowers a non-zero rotation to glyph strokes because its text
/// primitive cannot rotate.
/// `ink` adapts every layer colour it draws, which lets the PDF writer swap
/// pale ink for print-safe dark ink without touching the stored palette.
/// `view` restricts the drawing to one layer: only objects on that layer are
/// lowered, a footprint contributes its copper pads to its own layer and its
/// outline and reference to the matching silkscreen layer, and the Edge.Cuts
/// outline is always drawn. `None` keeps the composite preview behaviour.
fn build_drawing(
    board: &Board,
    layout: &Layout,
    background: Option<Color>,
    with_text_items: bool,
    ink: fn(Color) -> Color,
    view: Option<&str>,
) -> Drawing {
    let layer_ink = |layer: &str| ink(layer_color_value(board, layer));
    let in_view = |layer: &str| view.is_none_or(|view| view == layer);
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
            Style::stroke(layer_ink("Edge.Cuts"), 0.15 * layout.scale),
        ));
    }
    for zone in &board.zones {
        if !in_view(&zone.layer) {
            continue;
        }
        if zone.fill == "grid" {
            let stroke = Style::stroke(layer_ink(&zone.layer), zone.grid_width() * scale * layout.scale);
            for (from, to) in crate::board::grid_lines(&zone.points, zone.grid_spacing()) {
                drawing.push(Element::new(
                    Shape::Line {
                        from: layout.point(from[0] * scale, from[1] * scale),
                        to: layout.point(to[0] * scale, to[1] * scale),
                    },
                    stroke.clone(),
                ));
            }
            continue;
        }
        let points = zone.points.iter().map(|point| layout.point(point[0] * scale, point[1] * scale)).collect();
        let mut style = fill_style(layer_ink(&zone.layer));
        style.opacity = 0.35;
        drawing.push(Element::new(Shape::Polygon { points }, style));
    }
    for track in &board.tracks {
        if !in_view(&track.layer) {
            continue;
        }
        drawing.push(Element::new(
            Shape::Line {
                from: layout.point(track.from[0] * scale, track.from[1] * scale),
                to: layout.point(track.to[0] * scale, track.to[1] * scale),
            },
            Style::stroke(layer_ink(&track.layer), track.width * scale * layout.scale),
        ));
    }
    for footprint in &board.footprints {
        let draw_pads = view.is_none_or(|view| view == footprint.layer);
        let draw_silk = view.is_none_or(|view| view == silk_layer(&footprint.layer));
        if !draw_pads && !draw_silk {
            continue;
        }
        let color = layer_ink(&footprint.layer);
        if draw_pads {
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
        }
        if !draw_silk {
            continue;
        }
        let Some((min, max)) = footprint_silk(footprint)
        else {
            continue;
        };
        let silk = layer_ink(silk_layer(&footprint.layer));
        let corners = [[min[0], min[1]], [max[0], min[1]], [max[0], max[1]], [min[0], max[1]]];
        drawing.push(Element::new(
            Shape::Polygon {
                points: corners.iter().map(|corner| layout.point(corner[0] * scale, corner[1] * scale)).collect(),
            },
            Style::stroke(silk, 0.15 * layout.scale),
        ));
        if !footprint.ref_.is_empty() {
            let at = layout.point(min[0] * scale, (min[1] - 0.6) * scale);
            let size = REFERENCE_SIZE * layout.scale;
            let pad = 0.3 * layout.scale;
            let glyph_width = footprint.ref_.chars().count() as f64 * size * 0.6;
            let knockout = background.unwrap_or(Color::rgb(255, 255, 255));
            let corners = [
                DrawPoint::new(at.x - pad, at.y - size - pad),
                DrawPoint::new(at.x + glyph_width + pad, at.y - size - pad),
                DrawPoint::new(at.x + glyph_width + pad, at.y + pad),
                DrawPoint::new(at.x - pad, at.y + pad),
            ];
            drawing.push(Element::new(Shape::Polygon { points: corners.to_vec() }, fill_style(knockout)));
            drawing.push(Element::new(Shape::Text { at, text: footprint.ref_.clone(), size }, fill_style(silk)));
        }
    }
    for via in &board.vias {
        if view.is_some_and(|view| !board.via_covers(via, view)) {
            continue;
        }
        let via_layer = view
            .filter(|layer| board.layer(layer).is_some_and(|layer| layer.kind == crate::board::LayerKind::Copper))
            .unwrap_or("F.Cu");
        let center = layout.point(via.at[0] * scale, via.at[1] * scale);
        drawing.push(Element::new(
            Shape::Circle { center, radius: via.diameter * scale * layout.scale / 2.0 },
            fill_style(layer_ink(via_layer)),
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
            Style::stroke(layer_ink("F.Cu"), slot.width * scale * layout.scale),
        ));
    }
    if with_text_items {
        for text in &board.texts {
            if !in_view(&text.layer) {
                continue;
            }
            let rotation = text.rotation.rem_euclid(360.0);
            if rotation > 1e-9 && rotation < 360.0 - 1e-9 {
                draw_rotated_text(board, &mut drawing, layout, ink, text);
                continue;
            }
            drawing.push(Element::new(
                Shape::Text {
                    at: layout.point(text.at[0] * scale, text.at[1] * scale),
                    text: text.text.clone(),
                    size: text.size * scale * layout.scale,
                },
                fill_style(layer_ink(&text.layer)),
            ));
        }
    }
    drawing
}

/// Draws one rotated silkscreen text item as 5×7 glyph strokes.
///
/// The PDF text primitive carries no rotation, so a rotated item is lowered
/// to line segments with the same bitmap font the PNG raster uses: every run
/// of lit pixels in a glyph row becomes one segment, rotated about the text
/// anchor exactly as the SVG preview rotates its `<text>` element. Unrotated
/// text stays a searchable text element.
fn draw_rotated_text(
    board: &Board,
    drawing: &mut Drawing,
    layout: &Layout,
    ink: fn(Color) -> Color,
    text: &crate::board::TextItem,
) {
    let scale = board.millimetres();
    let unit = text.size * scale / 7.0;
    let advance = 6.0 * unit;
    let angle = text.rotation.to_radians();
    let (sin, cos) = (angle.sin(), angle.cos());
    let at = [text.at[0] * scale, text.at[1] * scale];
    let point = |x: f64, y: f64| layout.point(at[0] + x * cos + y * sin, at[1] - x * sin + y * cos);
    let stroke = Style::stroke(ink(layer_color_value(board, &text.layer)), unit * layout.scale);
    for (index, character) in text.text.chars().enumerate() {
        let Some(rows) = glyph(character)
        else {
            continue;
        };
        let left = index as f64 * advance;
        for (row, bits) in rows.iter().enumerate() {
            let mut column = 0;
            while column < 5 {
                if bits & (0b10000 >> column) == 0 {
                    column += 1;
                    continue;
                }
                let start = column;
                while column < 5 && bits & (0b10000 >> column) != 0 {
                    column += 1;
                }
                let y = (row as f64 - 7.0) * unit + unit / 2.0;
                drawing.push(Element::new(
                    Shape::Line { from: point(left + start as f64 * unit, y), to: point(left + column as f64 * unit, y) },
                    stroke.clone(),
                ));
            }
        }
    }
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
pub(crate) fn pad_corners(footprint: &crate::board::Footprint, pad: &crate::board::Pad) -> [[f64; 2]; 4] {
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

/// Returns the four rotated corners of one pad, grown by a mask expansion.
fn pad_corners_expanded(footprint: &crate::board::Footprint, pad: &crate::board::Pad, expansion: f64) -> [[f64; 2]; 4] {
    let angle = footprint.rotation.to_radians();
    let (sin, cos) = (angle.sin(), angle.cos());
    let center_x = footprint.x + pad.x * cos - pad.y * sin;
    let center_y = footprint.y + pad.x * sin + pad.y * cos;
    let half_w = pad.w / 2.0 + expansion;
    let half_h = pad.h / 2.0 + expansion;
    [[-half_w, -half_h], [half_w, -half_h], [half_w, half_h], [-half_w, half_h]].map(|[local_x, local_y]| {
        let x = center_x + local_x * cos - local_y * sin;
        let y = center_y + local_x * sin + local_y * cos;
        [x, y]
    })
}

/// Resolves a layer colour, falling back to the default copper tone.
fn layer_color_value(board: &Board, layer: &str) -> Color {
    color_from_hex(&layer_color(board, layer))
}

/// Leaves a layer colour untouched, because the preview canvas is dark.
fn preview_ink(color: Color) -> Color {
    color
}

/// Dark ink that replaces a pale layer colour on white paper.
const PRINT_INK: Color = Color::rgb(58, 58, 58);

/// Perceived brightness at or above which a layer colour is too pale to print.
///
/// `#e6e6e6` (0.90) and `#c4c4c4` (0.77) are over the line, while the edge-cut
/// gold `#d9b44a` (0.70) and both copper tones stay below it.
const PRINT_INK_THRESHOLD: f64 = 0.72;

/// Adapts a layer colour to the white paper of a PDF export.
///
/// The stored palette is chosen for the dark preview canvas, where silkscreen is
/// pale on purpose. On white paper that ink all but disappears, so a pale colour
/// becomes [`PRINT_INK`]; copper, edge cuts and every other colour that already
/// contrasts are returned unchanged. The document palette itself never changes.
pub(crate) fn print_color(color: Color) -> Color {
    let brightness =
        (0.299 * f64::from(color.red()) + 0.587 * f64::from(color.green()) + 0.114 * f64::from(color.blue())) / 255.0;
    if brightness >= PRINT_INK_THRESHOLD { PRINT_INK } else { color }
}

/// Parses a `#rrggbb` colour, falling back to the default copper tone.
fn color_from_hex(text: &str) -> Color {
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

/// Bounds for the fitted preview pages: every drawn object plus the
/// silkscreen text items and the footprint reference designators.
///
/// [`drawing_bounds`] measures only geometry, which lets a rotated label or
/// a long reference run into the title or legend band of a fitted page. This
/// rotates the four corners of every text box about its anchor, so the box a
/// reader sees is the box the fit reserves.
pub(crate) fn preview_bounds(board: &Board) -> ([f64; 2], [f64; 2]) {
    let scale = board.millimetres();
    let (mut min, mut max) = drawing_bounds(board);
    for text in &board.texts {
        let rotation = text.rotation.rem_euclid(360.0);
        let stroked = rotation > 1e-9 && rotation < 360.0 - 1e-9;
        let chars = text.text.chars().count() as f64;
        // A stroked rotation uses the 5×7 font, which advances 6/7 of the
        // cap height per character; the text writers estimate 0.6.
        let width = if stroked { chars * text.size * 6.0 / 7.0 } else { chars * text.size * 0.6 };
        let angle = text.rotation.to_radians();
        let (sin, cos) = (angle.sin(), angle.cos());
        for [x, y] in [[0.0, -text.size], [width, -text.size], [width, text.size], [0.0, text.size]] {
            include(&mut min, &mut max, (text.at[0] + x * cos + y * sin) * scale, (text.at[1] - x * sin + y * cos) * scale);
        }
    }
    for footprint in &board.footprints {
        let Some((silk_min, silk_max)) = footprint_silk(footprint)
        else {
            continue;
        };
        include(&mut min, &mut max, silk_max[0] * scale, silk_max[1] * scale);
        if footprint.ref_.is_empty() {
            continue;
        }
        let width = footprint.ref_.chars().count() as f64 * REFERENCE_SIZE * 0.6;
        let baseline = silk_min[1] - 0.6;
        include(&mut min, &mut max, silk_min[0] * scale, (baseline - REFERENCE_SIZE) * scale);
        include(&mut min, &mut max, (silk_min[0] + width) * scale, baseline * scale);
    }
    (min, max)
}

/// Maps millimetres on a drawing sheet to drawing units.
///
/// SVG keeps millimetres and grows downwards; PDF uses points and grows
/// upwards, so the same sheet description serves both outputs.
struct SheetMap {
    scale: f64,
    height: f64,
    y_up: bool,
}

impl SheetMap {
    fn point(&self, x: f64, y: f64) -> DrawPoint {
        let y = if self.y_up { (self.height - y) * self.scale } else { y * self.scale };
        DrawPoint::new(x * self.scale, y)
    }

    fn size(&self, value: f64) -> f64 {
        value * self.scale
    }
}

/// Draws the sheet frame and title block shared by the new drawing sheets.
fn sheet_chrome(drawing: &mut Drawing, map: &SheetMap, width: f64, height: f64, info: &SheetInfo) {
    let frame = [
        [SHEET_FRAME_INSET, SHEET_FRAME_INSET],
        [width - SHEET_FRAME_INSET, SHEET_FRAME_INSET],
        [width - SHEET_FRAME_INSET, height - SHEET_FRAME_INSET],
        [SHEET_FRAME_INSET, height - SHEET_FRAME_INSET],
    ];
    drawing.push(Element::new(
        Shape::Polygon { points: frame.iter().map(|corner| map.point(corner[0], corner[1])).collect() },
        Style::stroke(PRINT_INK, map.size(0.25)),
    ));
    drawing.push(Element::new(
        Shape::Line {
            from: map.point(SHEET_FRAME_INSET, SHEET_TITLE_BAND),
            to: map.point(width - SHEET_FRAME_INSET, SHEET_TITLE_BAND),
        },
        Style::stroke(PRINT_INK, map.size(0.2)),
    ));
    let number = format!("DWG {}", info.number);
    let number_width = text_width(&number, 2.4);
    let name_size = fit_font(info.name, (width - 14.0 - number_width).max(12.0), 4.2);
    drawing.push(Element::new(
        Shape::Text { at: map.point(4.0, 8.4), text: info.name.to_string(), size: map.size(name_size) },
        fill_style(PRINT_INK),
    ));
    let number_x = (width - 4.0 - number_width).max(4.0 + text_width(info.name, name_size) + 4.0);
    drawing.push(Element::new(
        Shape::Text { at: map.point(number_x, 8.4), text: number, size: map.size(2.4) },
        fill_style(PRINT_INK),
    ));
    let subtitle = format!("SCALE {} · UNITS {} · {}", info.scale, info.units, info.view);
    let subtitle_size = fit_font(&subtitle, width - 8.0, 2.3);
    drawing.push(Element::new(
        Shape::Text { at: map.point(4.0, 13.2), text: subtitle, size: map.size(subtitle_size) },
        fill_style(PRINT_INK),
    ));
}

/// Legend line on a new drawing sheet.
fn sheet_legend(drawing: &mut Drawing, map: &SheetMap, x: f64, y: f64, mark: LegendMark, color: Color, label: &str) {
    match mark {
        LegendMark::Area => {
            let corners = [[x, y - 2.2], [x + 3.2, y - 2.2], [x + 3.2, y], [x, y]];
            drawing.push(Element::new(
                Shape::Polygon { points: corners.iter().map(|corner| map.point(corner[0], corner[1])).collect() },
                Style::both(color, PRINT_INK, map.size(0.15)),
            ));
        }
        LegendMark::Line => {
            drawing.push(Element::new(
                Shape::Line { from: map.point(x, y - 1.1), to: map.point(x + 3.2, y - 1.1) },
                Style::stroke(color, map.size(0.6)),
            ));
        }
        LegendMark::Hole => {
            drawing.push(Element::new(
                Shape::Circle { center: map.point(x + 1.6, y - 1.1), radius: map.size(1.0) },
                Style::stroke(color, map.size(0.35)),
            ));
        }
        LegendMark::Edge => {
            for offset in [0.4, 1.8] {
                drawing.push(Element::new(
                    Shape::Line { from: map.point(x, y - offset), to: map.point(x + 3.2, y - offset) },
                    Style::stroke(color, map.size(0.2)),
                ));
            }
        }
    }
    drawing.push(Element::new(
        Shape::Text { at: map.point(x + 4.2, y), text: label.to_string(), size: map.size(2.5) },
        fill_style(PRINT_INK),
    ));
}

/// Writes the layer cross-section as SVG and PDF and reports both artifacts.
fn stackup_sheet(board: &Board, path: &Path) -> Result<Value, ServiceError> {
    export_directory("stackup", path)?;
    let mut artifacts = Vec::new();
    let mut total = 0u64;
    let svg = stackup_drawing(board, 1.0, false);
    let content = svg.to_svg().map_err(|error| crate::invalid(format!("cannot render the stackup drawing: {error}")))?;
    let content = scale_svg_root(&content, SVG_PREVIEW_PX_PER_MM);
    let bytes = crate::atomic_write(&path.join("Stackup.svg"), content.as_bytes())?;
    total += bytes;
    artifacts.push(json!({ "name": "Stackup.svg", "bytes": bytes }));
    let pdf = stackup_drawing(board, PT_PER_MM, true);
    let bytes = crate::atomic_write(
        &path.join("Stackup.pdf"),
        &pdf.to_pdf().map_err(|error| crate::invalid(format!("cannot render the stackup PDF: {error}")))?,
    )?;
    total += bytes;
    artifacts.push(json!({ "name": "Stackup.pdf", "bytes": bytes }));
    Ok(json!({ "path": path.to_string_lossy(), "format": "stackup", "bytes": total, "artifacts": artifacts }))
}

/// Draws the copper/dielectric cross-section with thickness callouts.
///
/// Every label is measured and fitted to its column, and the notes wrap to the
/// sheet width, so a long declared stackup or a differential-pair summary never
/// runs past the page edge.
fn stackup_drawing(board: &Board, scale: f64, y_up: bool) -> Drawing {
    let copper_layers = board.copper_stack();
    let declared = board.stackup;
    let stackup =
        declared.unwrap_or(crate::board::Stackup { copper_thickness: 0.035, dielectric_height: 0.2, dielectric_constant: 4.2 });
    let copper_total = copper_layers.len() as f64 * stackup.copper_thickness;
    let dielectric_total = copper_layers.len().saturating_sub(1) as f64 * stackup.dielectric_height;
    let board_thickness = copper_total + dielectric_total;

    let width = 190.0;
    let copper_h = 3.2;
    let dielectric_h = (3.2 * stackup.dielectric_height / stackup.copper_thickness.max(1e-6)).clamp(6.0, 14.0);
    let coin_count = copper_layers.len().max(1);
    let diagram_h = coin_count as f64 * copper_h + coin_count.saturating_sub(1) as f64 * dielectric_h;
    let content_top = SHEET_TITLE_BAND + 14.0;

    let mut notes = vec![format!("Total board thickness {:.3} mm · {} copper layers", board_thickness, copper_layers.len())];
    notes.extend(wrap_text(
        &if declared.is_some() {
            "Stackup values as declared in the board file.".to_string()
        }
        else {
            format!(
                "No stackup declared; showing 1 oz copper and {:.3} mm FR-4 with εr {:.1}.",
                stackup.dielectric_height, stackup.dielectric_constant
            )
        },
        width - 16.0,
        2.5,
    ));
    let mut impedance_line = String::from("Impedance: 0.20 mm microstrip -> ");
    match crate::impedance::values(crate::impedance::Structure::Microstrip, 0.2, None, stackup) {
        Ok(values) => impedance_line.push_str(&format!("{:.1} Ω", values.impedance)),
        Err(_) => impedance_line.push_str("n/a"),
    }
    if let Some(pair) = board.differential_pairs.first()
        && let Ok(values) =
            crate::impedance::values(crate::impedance::Structure::DiffMicrostrip, pair.width, Some(pair.gap), stackup)
    {
        impedance_line
            .push_str(&format!(" · DP {} {:.2}/{:.2} mm -> {:.1} Ω", pair.id, pair.width, pair.gap, values.impedance));
    }
    notes.extend(wrap_text(&impedance_line, width - 16.0, 2.5));
    let note_height = notes.len() as f64 * 4.8;
    let height = content_top + diagram_h + 12.0 + note_height + 10.0;
    let map = SheetMap { scale, height, y_up };
    let mut drawing = Drawing::new(Size::new(map.size(width), map.size(height))).with_background(SHEET_PAPER);
    sheet_chrome(
        &mut drawing,
        &map,
        width,
        height,
        &SheetInfo {
            name: "Stackup - layer cross-section",
            number: &board.id,
            scale: "NTS",
            units: "mm",
            view: if declared.is_some() { "DECLARED STACKUP" } else { "ASSUMED STACKUP" },
        },
    );

    let copper_ink = Color::rgb(196, 106, 42);
    let dielectric_ink = Color::rgb(233, 238, 226);
    let bar_left = 58.0;
    let bar_right = 128.0;
    let mut y = content_top;
    for (index, layer) in copper_layers.iter().enumerate() {
        drawing.push(Element::new(
            Shape::Polygon {
                points: vec![
                    map.point(bar_left, y),
                    map.point(bar_right, y),
                    map.point(bar_right, y + copper_h),
                    map.point(bar_left, y + copper_h),
                ],
            },
            Style::both(copper_ink, PRINT_INK, map.size(0.2)),
        ));
        let name_size = fit_font(layer, bar_left - 12.0, 2.8);
        drawing.push(Element::new(
            Shape::Text { at: map.point(6.0, y + copper_h - 0.8), text: layer.to_string(), size: map.size(name_size) },
            fill_style(PRINT_INK),
        ));
        let callout = format!("copper {:.3} mm", stackup.copper_thickness);
        let callout_size = fit_font(&callout, bar_right - bar_left - 5.0, 2.2);
        drawing.push(Element::new(
            Shape::Text { at: map.point(bar_left + 2.0, y + copper_h - 0.8), text: callout, size: map.size(callout_size) },
            fill_style(Color::rgb(40, 20, 5)),
        ));
        drawing.push(Element::new(
            Shape::Text {
                at: map.point(bar_right + 4.0, y + copper_h - 0.8),
                text: format!("{:.3} mm", stackup.copper_thickness),
                size: map.size(2.2),
            },
            fill_style(PRINT_INK),
        ));
        y += copper_h;
        if index + 1 < copper_layers.len() {
            drawing.push(Element::new(
                Shape::Polygon {
                    points: vec![
                        map.point(bar_left, y),
                        map.point(bar_right, y),
                        map.point(bar_right, y + dielectric_h),
                        map.point(bar_left, y + dielectric_h),
                    ],
                },
                Style::both(dielectric_ink, PRINT_INK, map.size(0.2)),
            ));
            drawing.push(Element::new(
                Shape::Text {
                    at: map.point(6.0, y + dielectric_h / 2.0 + 1.0),
                    text: "Dielectric".to_string(),
                    size: map.size(fit_font("Dielectric", bar_left - 12.0, 2.8)),
                },
                fill_style(PRINT_INK),
            ));
            let material = format!("FR-4 · εr {:.1}", stackup.dielectric_constant);
            drawing.push(Element::new(
                Shape::Text {
                    at: map.point(bar_left + 2.0, y + dielectric_h / 2.0 + 1.0),
                    text: material.clone(),
                    size: map.size(fit_font(&material, bar_right - bar_left - 5.0, 2.2)),
                },
                fill_style(PRINT_INK),
            ));
            drawing.push(Element::new(
                Shape::Text {
                    at: map.point(bar_right + 4.0, y + dielectric_h / 2.0 + 1.0),
                    text: format!("{:.3} mm", stackup.dielectric_height),
                    size: map.size(2.2),
                },
                fill_style(PRINT_INK),
            ));
            y += dielectric_h;
        }
    }
    // Dimension line down the right edge with a tick at every layer boundary.
    let mut tick = content_top;
    drawing.push(Element::new(
        Shape::Line { from: map.point(bar_right + 1.0, content_top), to: map.point(bar_right + 1.0, content_top + diagram_h) },
        Style::stroke(PRINT_INK, map.size(0.15)),
    ));
    for index in 0..coin_count {
        drawing.push(Element::new(
            Shape::Line { from: map.point(bar_right + 0.2, tick), to: map.point(bar_right + 1.8, tick) },
            Style::stroke(PRINT_INK, map.size(0.15)),
        ));
        tick += copper_h;
        if index + 1 < coin_count {
            tick += dielectric_h;
        }
    }
    drawing.push(Element::new(
        Shape::Line {
            from: map.point(bar_right + 0.2, content_top + diagram_h),
            to: map.point(bar_right + 1.8, content_top + diagram_h),
        },
        Style::stroke(PRINT_INK, map.size(0.15)),
    ));

    let notes_top = content_top + diagram_h + 9.0;
    for (line, text) in notes.iter().enumerate() {
        drawing.push(Element::new(
            Shape::Text { at: map.point(6.0, notes_top + line as f64 * 4.8), text: text.clone(), size: map.size(2.5) },
            fill_style(PRINT_INK),
        ));
    }
    drawing
}
/// Writes the panelization drawing as SVG and PDF and reports both artifacts.
fn panel_sheet(board: &Board, path: &Path) -> Result<Value, ServiceError> {
    export_directory("panel", path)?;
    let mut artifacts = Vec::new();
    let mut total = 0u64;
    let svg = panel_drawing(board, 1.0, false)?;
    let content = svg.to_svg().map_err(|error| crate::invalid(format!("cannot render the panel drawing: {error}")))?;
    let content = scale_svg_root(&content, SVG_PREVIEW_PX_PER_MM);
    let bytes = crate::atomic_write(&path.join("Panel.svg"), content.as_bytes())?;
    total += bytes;
    artifacts.push(json!({ "name": "Panel.svg", "bytes": bytes }));
    let pdf = panel_drawing(board, PT_PER_MM, true)?;
    let bytes = crate::atomic_write(
        &path.join("Panel.pdf"),
        &pdf.to_pdf().map_err(|error| crate::invalid(format!("cannot render the panel PDF: {error}")))?,
    )?;
    total += bytes;
    artifacts.push(json!({ "name": "Panel.pdf", "bytes": bytes }));
    Ok(json!({ "path": path.to_string_lossy(), "format": "panel", "bytes": total, "artifacts": artifacts }))
}

/// Draws a default 2 x 2 V-CUT panel with rails, tooling holes and dimensions.
///
/// The CLI has no per-export option channel, so the drawing uses the same
/// defaults the panelization report documents: 2 columns by 2 rows, 2 mm gaps,
/// a 5 mm rail, V-CUT seams and four Ø3 mm tooling holes.
fn panel_drawing(board: &Board, scale: f64, y_up: bool) -> Result<Drawing, ServiceError> {
    let report = crate::panel::report(
        board,
        crate::panel::PanelizeRequest {
            columns: 2,
            rows: 2,
            gap_x: Some(2.0),
            gap_y: Some(2.0),
            rail: Some(5.0),
            tab: Some("v-cut".to_string()),
            tab_width: None,
            tab_spacing: None,
            tooling_hole: Some(3.0),
            tooling_hole_inset: Some(2.5),
        },
    )?;
    let panel_from = json_pair(&report["panelOutline"]["from"]);
    let panel_to = json_pair(&report["panelOutline"]["to"]);
    let panel_w = (panel_to[0] - panel_from[0]).max(1.0);
    let panel_h = (panel_to[1] - panel_from[1]).max(1.0);
    let width = 18.0 + panel_w + 44.0;
    let content_top = SHEET_TITLE_BAND + 18.0;
    let height = content_top + panel_h + 50.0;
    let map = SheetMap { scale, height, y_up };
    let mut drawing = Drawing::new(Size::new(map.size(width), map.size(height))).with_background(Color::rgb(255, 255, 255));
    let columns = report["columns"].as_u64().unwrap_or(2);
    let rows = report["rows"].as_u64().unwrap_or(2);
    let view = format!("{columns} x {rows} ARRAY · V-CUT");
    sheet_chrome(
        &mut drawing,
        &map,
        width,
        height,
        &SheetInfo { name: "Panelization - board array", number: &board.id, scale: "1:1", units: "mm", view: &view },
    );
    let left = 12.0;
    let px = |x: f64| left + (x - panel_from[0]);
    let py = |y: f64| content_top + (y - panel_from[1]);

    drawing.push(Element::new(
        Shape::Polygon {
            points: vec![
                map.point(px(panel_from[0]), py(panel_from[1])),
                map.point(px(panel_to[0]), py(panel_from[1])),
                map.point(px(panel_to[0]), py(panel_to[1])),
                map.point(px(panel_from[0]), py(panel_to[1])),
            ],
        },
        Style::stroke(PRINT_INK, map.size(0.35)),
    ));
    if let Some(placements) = report["placements"].as_array() {
        for (index, placement) in placements.iter().enumerate() {
            let from = json_pair(&placement["from"]);
            let to = json_pair(&placement["to"]);
            let corners = [
                map.point(px(from[0]), py(from[1])),
                map.point(px(to[0]), py(from[1])),
                map.point(px(to[0]), py(to[1])),
                map.point(px(from[0]), py(to[1])),
            ];
            drawing.push(Element::new(
                Shape::Polygon { points: corners.to_vec() },
                Style::stroke(layer_color_value(board, "Edge.Cuts"), map.size(0.22)),
            ));
            drawing.push(Element::new(
                Shape::Text {
                    at: map.point(px((from[0] + to[0]) / 2.0) - 2.5, py((from[1] + to[1]) / 2.0) + 1.2),
                    text: format!("{}", index + 1),
                    size: map.size(4.0),
                },
                fill_style(Color::rgb(120, 120, 120)),
            ));
        }
    }
    if let Some(seams) = report["seams"].as_array() {
        for seam in seams {
            let from = json_pair(&seam["from"]);
            let to = json_pair(&seam["to"]);
            drawing.push(Element::new(
                Shape::Line { from: map.point(px(from[0]), py(from[1])), to: map.point(px(to[0]), py(to[1])) },
                Style {
                    stroke: Some(Color::rgb(176, 58, 46)),
                    width: map.size(0.25),
                    dash: Some([map.size(2.0), map.size(1.0)]),
                    ..Style::default()
                },
            ));
        }
    }
    if let Some(tabs) = report["tabs"].as_array() {
        for tab in tabs {
            let x = tab["x"].as_f64().unwrap_or(0.0);
            let y = tab["y"].as_f64().unwrap_or(0.0);
            let w = tab["width"].as_f64().unwrap_or(0.0);
            let h = tab["height"].as_f64().unwrap_or(0.0);
            let corners = [
                map.point(px(x), py(y)),
                map.point(px(x + w), py(y)),
                map.point(px(x + w), py(y + h)),
                map.point(px(x), py(y + h)),
            ];
            drawing.push(Element::new(Shape::Polygon { points: corners.to_vec() }, Style::stroke(PRINT_INK, map.size(0.2))));
        }
    }
    if let Some(holes) = report["toolingHoles"].as_array() {
        for hole in holes {
            let x = hole["x"].as_f64().unwrap_or(0.0);
            let y = hole["y"].as_f64().unwrap_or(0.0);
            let diameter = hole["diameter"].as_f64().unwrap_or(0.0);
            let center = map.point(px(x), py(y));
            let half = diameter.max(0.2) / 2.0;
            drawing
                .push(Element::new(Shape::Circle { center, radius: map.size(half) }, Style::stroke(PRINT_INK, map.size(0.25))));
            drawing.push(Element::new(
                Shape::Line { from: map.point(px(x) - half - 1.0, py(y)), to: map.point(px(x) + half + 1.0, py(y)) },
                Style::stroke(PRINT_INK, map.size(0.12)),
            ));
            drawing.push(Element::new(
                Shape::Line { from: map.point(px(x), py(y) - half - 1.0), to: map.point(px(x), py(y) + half + 1.0) },
                Style::stroke(PRINT_INK, map.size(0.12)),
            ));
        }
    }

    // Overall dimensions and the array pitch.
    let dim_y = py(panel_to[1]) + 10.0;
    for (x, tick) in [(px(panel_from[0]), 0.0), (px(panel_to[0]), 0.0)] {
        drawing.push(Element::new(
            Shape::Line { from: map.point(x, dim_y - 2.0 + tick), to: map.point(x, dim_y + 2.0 + tick) },
            Style::stroke(PRINT_INK, map.size(0.2)),
        ));
    }
    drawing.push(Element::new(
        Shape::Line { from: map.point(px(panel_from[0]), dim_y), to: map.point(px(panel_to[0]), dim_y) },
        Style::stroke(PRINT_INK, map.size(0.2)),
    ));
    drawing.push(Element::new(
        Shape::Text {
            at: map.point(px(panel_from[0]), dim_y - 2.4),
            text: format!("Panel width {:.1} mm", panel_w),
            size: map.size(2.5),
        },
        fill_style(PRINT_INK),
    ));
    let dim_x = px(panel_to[0]) + 9.0;
    drawing.push(Element::new(
        Shape::Line { from: map.point(dim_x, py(panel_from[1])), to: map.point(dim_x, py(panel_to[1])) },
        Style::stroke(PRINT_INK, map.size(0.2)),
    ));
    for y in [py(panel_from[1]), py(panel_to[1])] {
        drawing.push(Element::new(
            Shape::Line { from: map.point(dim_x - 2.0, y), to: map.point(dim_x + 2.0, y) },
            Style::stroke(PRINT_INK, map.size(0.2)),
        ));
    }
    drawing.push(Element::new(
        Shape::Text {
            at: map.point(dim_x + 1.5, py(panel_from[1]) + panel_h / 2.0),
            text: format!("Panel height {:.1} mm", panel_h),
            size: map.size(2.5),
        },
        fill_style(PRINT_INK),
    ));
    if let Some(placements) = report["placements"].as_array()
        && placements.len() > 1
    {
        let first = json_pair(&placements[0]["from"]);
        let second = json_pair(&placements[1]["from"]);
        let pitch_x = second[0] - first[0];
        if pitch_x.abs() > 0.01 {
            let y = py(panel_from[1]) - 5.0;
            drawing.push(Element::new(
                Shape::Line { from: map.point(px(first[0]), y), to: map.point(px(second[0]), y) },
                Style::stroke(PRINT_INK, map.size(0.2)),
            ));
            drawing.push(Element::new(
                Shape::Text {
                    at: map.point(px(first[0]), y - 1.6),
                    text: format!("Array pitch {pitch_x:.1} mm"),
                    size: map.size(2.4),
                },
                fill_style(PRINT_INK),
            ));
        }
        let pitch_y = second[1] - first[1];
        if pitch_y.abs() > 0.01 {
            let x = px(panel_to[0]) + 3.0;
            drawing.push(Element::new(
                Shape::Line { from: map.point(x, py(first[1])), to: map.point(x, py(second[1])) },
                Style::stroke(PRINT_INK, map.size(0.2)),
            ));
        }
    }

    let notes_top = height - 34.0;
    let board_size =
        [report["boardSize"]["width"].as_f64().unwrap_or(0.0), report["boardSize"]["height"].as_f64().unwrap_or(0.0)];
    let notes = [
        format!(
            "{columns} x {rows} array · {} boards · gap {:.1} x {:.1} mm · rail {:.1} mm",
            report["boardCount"].as_u64().unwrap_or(0),
            report["gapX"].as_f64().unwrap_or(0.0),
            report["gapY"].as_f64().unwrap_or(0.0),
            report["rail"].as_f64().unwrap_or(0.0)
        ),
        format!(
            "Board {:.1} x {:.1} mm · V-CUT seams · tooling Ø{:.1} mm x {}",
            board_size[0],
            board_size[1],
            report["toolingHoles"]
                .as_array()
                .and_then(|holes| holes.first())
                .and_then(|hole| hole["diameter"].as_f64())
                .unwrap_or(0.0),
            report["toolingHoleCount"].as_u64().unwrap_or(0)
        ),
        format!(
            "Panel utilization {:.1} % · breakaway {:.1} %",
            report["utilizationPercent"].as_f64().unwrap_or(0.0),
            report["breakaway"]["percent"].as_f64().unwrap_or(0.0)
        ),
    ];
    for (index, note) in notes.iter().enumerate() {
        drawing.push(Element::new(
            Shape::Text { at: map.point(6.0, notes_top + index as f64 * 4.6), text: note.clone(), size: map.size(2.6) },
            fill_style(PRINT_INK),
        ));
    }
    let legend_y = height - 14.0;
    let entries = [
        (
            LegendMark::Edge,
            layer_color_value(board, "Edge.Cuts"),
            format!("Board {:.1} x {:.1} mm", board_size[0], board_size[1]),
        ),
        (LegendMark::Edge, PRINT_INK, "Panel edge".to_string()),
        (LegendMark::Line, Color::rgb(176, 58, 46), "V-CUT seam".to_string()),
        (LegendMark::Hole, PRINT_INK, "Tooling hole".to_string()),
    ];
    let mut x = 6.0;
    for (mark, color, label) in entries {
        sheet_legend(&mut drawing, &map, x, legend_y, mark, color, &label);
        x += 6.0 + label.chars().count() as f64 * 2.5 * 0.6 + 6.0;
    }
    Ok(drawing)
}

/// Reads a two-element JSON array of millimetres.
fn json_pair(value: &Value) -> [f64; 2] {
    [value.get(0).and_then(Value::as_f64).unwrap_or(0.0), value.get(1).and_then(Value::as_f64).unwrap_or(0.0)]
}

/// Pixels per board millimetre used as the root size of the exported SVG.
const SVG_PREVIEW_PX_PER_MM: f64 = 10.0;

/// Rewrites the root `width` and `height` so a standalone file opens at a
/// legible size while the `viewBox` keeps board units.
fn scale_svg_root(content: &str, factor: f64) -> String {
    let Some(start) = content.find("<svg")
    else {
        return content.to_string();
    };
    let Some(offset) = content[start..].find('>')
    else {
        return content.to_string();
    };
    let end = start + offset;
    let mut root = content[start..=end].to_string();
    for attribute in ["width", "height"] {
        root = scale_svg_attribute(&root, attribute, factor);
    }
    format!("{}{}{}", &content[..start], root, &content[end + 1..])
}

fn scale_svg_attribute(tag: &str, attribute: &str, factor: f64) -> String {
    let needle = format!("{attribute}=\"");
    let Some(start) = tag.find(&needle)
    else {
        return tag.to_string();
    };
    let value_start = start + needle.len();
    let Some(offset) = tag[value_start..].find('"')
    else {
        return tag.to_string();
    };
    let value_end = value_start + offset;
    let Ok(value) = tag[value_start..value_end].parse::<f64>()
    else {
        return tag.to_string();
    };
    format!("{}{:.0}{}", &tag[..value_start], value * factor, &tag[value_end..])
}

/// Renders the composite and reports the canvas size the same viewBox used.
pub(crate) fn svg_preview(board: &Board) -> Result<(String, f64, f64), ServiceError> {
    svg_preview_with(board, Some(PREVIEW_BACKGROUND), preview_ink)
}

/// Renders the composite on a chosen paper and ink.
///
/// The interactive studio preview keeps the dark canvas; every exported sheet
/// asks for white paper and print-safe ink so a drawing reads like a
/// manufacturing print.
fn svg_preview_with(
    board: &Board,
    background: Option<Color>,
    ink: fn(Color) -> Color,
) -> Result<(String, f64, f64), ServiceError> {
    let scale = board.millimetres();
    let (min, max) = preview_bounds(board);
    let margin = sheet_padding((max[0] - min[0]).max(max[1] - min[1]).max(1.0));
    let width = (max[0] - min[0]).max(1.0) + margin * 2.0;
    let height = (max[1] - min[1]).max(1.0) + margin * 2.0;
    let layout = Layout { scale: 1.0, origin_x: min[0] - margin, origin_y: min[1] - margin, width, height, y_up: false };
    let drawing = build_drawing(board, &layout, background, false, ink, None);
    let mut content = drawing.to_svg().map_err(|error| crate::invalid(format!("cannot render the SVG preview: {error}")))?;
    content = scale_svg_root(&content, SVG_PREVIEW_PX_PER_MM);
    if !board.texts.is_empty()
        && let Some(index) = content.rfind("</svg>")
    {
        let mut group = String::from("    <g class=\"board-text\">\n");
        let knockout = background.unwrap_or(SHEET_PAPER).to_css();
        for text in &board.texts {
            let x = (text.at[0] * scale - layout.origin_x) * layout.scale;
            let y = (text.at[1] * scale - layout.origin_y) * layout.scale;
            let size = text.size * scale * layout.scale;
            let glyph_width = text.text.chars().count() as f64 * size * 0.6;
            let pad = 0.3;
            let _ = writeln!(
                group,
                "      <rect x=\"{:.3}\" y=\"{:.3}\" width=\"{:.3}\" height=\"{:.3}\" fill=\"{knockout}\" transform=\"rotate({:.3} {x:.3} {y:.3})\"/>",
                x - pad,
                y - size - pad,
                glyph_width + 2.0 * pad,
                size + 2.0 * pad,
                -text.rotation
            );
            let _ = writeln!(
                group,
                "      <text x=\"{x:.3}\" y=\"{y:.3}\" font-family=\"monospace\" font-size=\"{:.3}\" fill=\"{}\" transform=\"rotate({:.3} {x:.3} {y:.3})\">{}</text>",
                size,
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
/// White paper the raster preview is printed on, so the first pixel is white.
const PNG_BACKGROUND: [u8; 3] = [255, 255, 255];
/// Preview outline colour: the same dark gold the vector sheets draw.
const PNG_OUTLINE: [u8; 3] = [138, 109, 31];
/// Front copper colour.
const PNG_FRONT: [u8; 3] = [163, 82, 30];
/// Back copper colour.
const PNG_BACK: [u8; 3] = [31, 95, 139];
/// Silkscreen colour.
const PNG_SILK: [u8; 3] = [24, 24, 24];
/// Drilled-hole colour: paper shows through the hole.
const PNG_DRILL: [u8; 3] = [255, 255, 255];

/// Writes a top-down raster preview as a valid 8-bit RGBA PNG.
///
/// The pixels are encoded in memory with zlib-deflate at best compression and
/// published through the shared atomic writer.
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
    let written = crate::atomic_write(path, &encode_png_compressed(PNG_WIDTH, PNG_HEIGHT, &rgba))?;
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

/// CRC-32 as required by the PNG chunk format.
fn png_crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

/// Writes one PNG chunk: length, type plus payload, and the CRC over both.
fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&png_crc32(&body).to_be_bytes());
}

/// Encodes RGBA8 pixels as PNG bytes with a zlib-deflate IDAT stream at best
/// compression, so a flat preview raster stays in the tens of kilobytes
/// instead of the megabytes a stored-block encoder would emit.
fn encode_png_compressed(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write as _;
    let stride = (width as usize) * 4;
    let mut raw = Vec::with_capacity((height as usize) * (stride + 1));
    for row in rgba.chunks_exact(stride) {
        raw.push(0u8);
        raw.extend_from_slice(row);
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
    encoder.write_all(&raw).expect("in-memory compression cannot fail");
    let idat = encoder.finish().expect("in-memory compression cannot fail");
    let mut out: Vec<u8> = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr);
    png_chunk(&mut out, b"IDAT", &idat);
    png_chunk(&mut out, b"IEND", &[]);
    out
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
