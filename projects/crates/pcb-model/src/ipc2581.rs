//! IPC-2581B XML writer.
//!
//! The document is built from board geometry only. Package definitions come
//! from placed footprints, components carry their reference, part, value,
//! layer, position and rotation, and copper features are grouped per layer.
//!
//! Features also carry the copper attributes the native model holds (`net` on
//! pads and lines, `padDiameter`, `fromLayer` and `toLayer` on holes), so
//! [`crate::ipc2581_import`] restores nets and via spans when the file is read
//! back.

use std::fmt::Write as _;

use crate::board::{Board, Footprint, LayerKind, Pad};

/// Writes one board as an IPC-2581B XML document.
pub(crate) fn write(board: &Board) -> String {
    let scale = board.millimetres();
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<IPC-2581 revision=\"B\" xmlns=\"http://webstds.ipc.org/2581\" generator=\"open-pcb\">\n");
    out.push_str("  <Ecad>\n    <Content>\n");
    out.push_str("      <Step name=\"PCB\">\n");
    out.push_str("        <Datum x=\"0\" y=\"0\"/>\n");

    if let Some(outline) = board.outline {
        out.push_str("        <Profile>\n          <Polyline>\n");
        let corners = [
            [outline.from[0], outline.from[1]],
            [outline.to[0], outline.from[1]],
            [outline.to[0], outline.to[1]],
            [outline.from[0], outline.to[1]],
        ];
        for index in 0..corners.len() {
            let start = corners[index];
            let end = corners[(index + 1) % corners.len()];
            let _ = writeln!(
                out,
                "            <Line startX=\"{}\" startY=\"{}\" endX=\"{}\" endY=\"{}\"/>",
                number(start[0] * scale),
                number(start[1] * scale),
                number(end[0] * scale),
                number(end[1] * scale)
            );
        }
        out.push_str("          </Polyline>\n        </Profile>\n");
    }
    else {
        out.push_str("        <Profile/>\n");
    }

    let mut packages: Vec<String> = Vec::new();
    for footprint in &board.footprints {
        let package = package_name(footprint);
        if packages.iter().any(|existing| existing == &package) {
            continue;
        }
        packages.push(package.clone());
        package_block(&mut out, &package, footprint, scale);
    }

    for footprint in &board.footprints {
        let _ = writeln!(
            out,
            "        <Component ref=\"{}\" part=\"{}\" value=\"{}\" layer=\"{}\" x=\"{}\" y=\"{}\" rotation=\"{}\"/>",
            escape(&footprint.ref_),
            escape(&package_name(footprint)),
            escape(&footprint.value),
            escape(&footprint.layer),
            number(footprint.x * scale),
            number(footprint.y * scale),
            number(footprint.rotation)
        );
    }

    for net in &board.nets {
        let _ = writeln!(out, "        <LogicalNet name=\"{}\"/>", escape(&net.name));
    }

    for layer in &board.layers {
        layer_feature(&mut out, board, layer.id.as_str(), layer.kind, scale);
    }

    out.push_str("      </Step>\n    </Content>\n  </Ecad>\n</IPC-2581>\n");
    out
}

/// Writes one package definition with a pin per pad.
fn package_block(out: &mut String, package: &str, footprint: &Footprint, scale: f64) {
    let _ = writeln!(out, "        <Package name=\"{}\" type=\"SM\">", escape(package));
    for pad in &footprint.pads {
        let _ = writeln!(
            out,
            "          <Pin number=\"{}\" name=\"{}\" type=\"surface\">",
            escape(&pad.number),
            escape(&pad.number)
        );
        let _ = writeln!(out, "            <Location x=\"{}\" y=\"{}\"/>", number(pad.x * scale), number(pad.y * scale));
        out.push_str("            <StandardPrimitive>\n");
        let _ = writeln!(out, "              <Rect width=\"{}\" height=\"{}\"/>", number(pad.w * scale), number(pad.h * scale));
        out.push_str("            </StandardPrimitive>\n          </Pin>\n");
    }
    out.push_str("        </Package>\n");
}

/// Writes the pads, tracks and holes that belong to one layer.
fn layer_feature(out: &mut String, board: &Board, layer: &str, kind: LayerKind, scale: f64) {
    let tracks: Vec<_> = board.tracks.iter().filter(|track| track.layer == layer).collect();
    let pads: Vec<(&Footprint, &Pad)> = board
        .footprints
        .iter()
        .filter(|footprint| footprint.layer == layer)
        .flat_map(|footprint| footprint.pads.iter().map(move |pad| (footprint, pad)))
        .collect();
    let holes = kind == LayerKind::Copper && layer == "F.Cu" && !board.vias.is_empty();
    if tracks.is_empty() && pads.is_empty() && !holes {
        return;
    }

    let _ = writeln!(out, "        <LayerFeature layerRef=\"{}\" kind=\"{}\">", escape(layer), kind_name(kind));
    out.push_str("          <Set>\n");
    for (footprint, pad) in pads {
        let center = pad_center(footprint, pad);
        let net = pad.net.as_deref().map(|net| format!(" net=\"{}\"", escape(net))).unwrap_or_default();
        let _ = writeln!(
            out,
            "            <Pad pinRef=\"{}.{}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"{net}/>",
            escape(&footprint.ref_),
            escape(&pad.number),
            number(center[0] * scale),
            number(center[1] * scale),
            number(pad.w * scale),
            number(pad.h * scale)
        );
    }
    for track in tracks {
        let net = track.net.as_deref().map(|net| format!(" net=\"{}\"", escape(net))).unwrap_or_default();
        let _ = writeln!(
            out,
            "            <Line startX=\"{}\" startY=\"{}\" endX=\"{}\" endY=\"{}\" width=\"{}\"{net}/>",
            number(track.from[0] * scale),
            number(track.from[1] * scale),
            number(track.to[0] * scale),
            number(track.to[1] * scale),
            number(track.width * scale)
        );
    }
    out.push_str("          </Set>\n");
    if holes {
        for via in &board.vias {
            let net = via.net.as_deref().map(|net| format!(" net=\"{}\"", escape(net))).unwrap_or_default();
            let span = match (&via.from_layer, &via.to_layer) {
                (Some(from), Some(to)) => {
                    format!(" fromLayer=\"{}\" toLayer=\"{}\"", escape(from), escape(to))
                }
                _ => String::new(),
            };
            let back = match &via.back_drill {
                Some(back) => format!(
                    " backDrillDepth=\"{}\" backDrillStub=\"{}\"",
                    number(back.depth * scale),
                    number(back.stub * scale)
                ),
                None => String::new(),
            };
            let _ = writeln!(
                out,
                "          <Hole x=\"{}\" y=\"{}\" diameter=\"{}\" padDiameter=\"{}\" plating=\"plated\"{net}{span}{back}/>",
                number(via.at[0] * scale),
                number(via.at[1] * scale),
                number(via.drill * scale),
                number(via.diameter * scale)
            );
        }
    }
    out.push_str("        </LayerFeature>\n");
}

/// Returns the package name of a footprint, with a stable fallback.
fn package_name(footprint: &Footprint) -> String {
    if footprint.footprint.trim().is_empty() { "Package".to_string() } else { footprint.footprint.clone() }
}

/// Lowercase stackup role, readable by the importer and by other tools.
fn kind_name(kind: LayerKind) -> &'static str {
    match kind {
        LayerKind::Copper => "copper",
        LayerKind::Silkscreen => "silkscreen",
        LayerKind::Soldermask => "soldermask",
        LayerKind::Assembly => "assembly",
        LayerKind::Mechanical => "mechanical",
    }
}

/// Rotates one pad offset into board coordinates.
fn pad_center(footprint: &Footprint, pad: &Pad) -> [f64; 2] {
    let angle = footprint.rotation.to_radians();
    let (sin, cos) = (angle.sin(), angle.cos());
    [footprint.x + pad.x * cos - pad.y * sin, footprint.y + pad.x * sin + pad.y * cos]
}

/// Formats millimetres with four decimals and no trailing zeros.
fn number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let value = if value.abs() < 0.00005 { 0.0 } else { value };
    let mut text = format!("{value:.4}");
    if text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if text == "-0" { "0".to_string() } else { text }
}

/// Escapes one XML attribute value, including both quote characters.
fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}
