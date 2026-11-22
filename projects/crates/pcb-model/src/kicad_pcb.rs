//! KiCad 7 `kicad_pcb` writer.
//!
//! The writer emits a deterministic S-expression from geometry the document
//! already carries. Declared nets are numbered in document order and copper
//! objects reference those numbers; an unassigned object references `net 0`.
//! Footprints keep their reference, value and pads.

use std::fmt::Write as _;

use crate::board::{Board, Footprint, LayerKind, Pad};

/// Writes one board as a KiCad 7 `kicad_pcb` document.
pub(crate) fn write(board: &Board) -> String {
    let scale = board.millimetres();
    let mut out = String::from("(kicad_pcb (version 20221018) (generator open-pcb)\n");
    out.push_str("  (general (thickness 1.6))\n");
    out.push_str("  (paper \"A4\")\n");
    out.push_str("  (layers\n");
    for (index, layer) in board.layers.iter().enumerate() {
        let kind = match layer.kind {
            LayerKind::Copper => "signal",
            _ => "user",
        };
        let name = kicad_layer_name(board, &layer.id);
        let _ = writeln!(out, "    ({} \"{}\" {kind})", layer_number(&name, index), sexpr(&name));
    }
    out.push_str("  )\n");
    out.push_str("  (setup\n    (pad_to_mask_clearance 0)\n  )\n");
    for net in &board.nets {
        let _ = writeln!(out, "  (net {} \"{}\")", net_number(board, Some(&net.name)), sexpr(&net.name));
    }

    if let Some(outline) = board.outline {
        let _ = writeln!(
            out,
            "  (gr_rect (start {}) (end {}) (layer \"Edge.Cuts\") (width 0.1))",
            point(outline.from, scale),
            point(outline.to, scale)
        );
    }

    for text in &board.texts {
        let _ = writeln!(
            out,
            "  (gr_text \"{}\" (at {} {}) (layer \"{}\") (effects (font (size {} {}) (thickness {}))))",
            sexpr(&text.text),
            point(text.at, scale),
            mm(text.rotation),
            sexpr(&kicad_layer_name(board, &text.layer)),
            mm(text.size * scale),
            mm(text.size * scale),
            mm(text.thickness * scale)
        );
    }

    for track in &board.tracks {
        let _ = writeln!(
            out,
            "  (segment (start {}) (end {}) (width {}) (layer \"{}\") (net {}))",
            point(track.from, scale),
            point(track.to, scale),
            mm(track.width * scale),
            sexpr(&kicad_layer_name(board, &track.layer)),
            net_number(board, track.net.as_deref())
        );
    }

    for via in &board.vias {
        let from = via.from_layer.as_deref().unwrap_or("F.Cu");
        let to = via.to_layer.as_deref().unwrap_or("B.Cu");
        let _ = writeln!(
            out,
            "  (via (at {}) (size {}) (drill {}) (layers \"{}\" \"{}\") (net {}))",
            point(via.at, scale),
            mm(via.diameter * scale),
            mm(via.drill * scale),
            sexpr(&kicad_layer_name(board, from)),
            sexpr(&kicad_layer_name(board, to)),
            net_number(board, via.net.as_deref())
        );
    }

    for footprint in &board.footprints {
        footprint_block(&mut out, footprint, board, scale);
    }

    out.push_str(")\n");
    out
}

/// Writes one footprint with its reference, value and pads.
fn footprint_block(out: &mut String, footprint: &Footprint, board: &Board, scale: f64) {
    let package = if footprint.footprint.trim().is_empty() { "Package" } else { footprint.footprint.as_str() };
    let _ = writeln!(out, "  (footprint \"{}\" (layer \"{}\")", sexpr(package), sexpr(&footprint.layer));
    let _ = writeln!(out, "    (at {} {})", point([footprint.x, footprint.y], scale), mm(footprint.rotation));
    let _ = writeln!(
        out,
        "    (property \"Reference\" \"{}\" (layer \"{}\"))",
        sexpr(&footprint.ref_),
        reference_layer(&footprint.layer)
    );
    let _ =
        writeln!(out, "    (property \"Value\" \"{}\" (layer \"{}\"))", sexpr(&footprint.value), value_layer(&footprint.layer));
    for pad in &footprint.pads {
        pad_block(out, pad, &footprint.layer, board, scale);
    }
    out.push_str("  )\n");
}

/// Writes one surface-mount rectangular pad relative to its footprint.
fn pad_block(out: &mut String, pad: &Pad, footprint_layer: &str, board: &Board, scale: f64) {
    let net = match pad.net.as_deref() {
        Some(name) => format!(" (net {} \"{}\")", net_number(board, Some(name)), sexpr(name)),
        None => String::new(),
    };
    let _ = writeln!(
        out,
        "    (pad \"{}\" smd rect (at {}) (size {} {}) (layers {}){net})",
        sexpr(&pad.number),
        point([pad.x, pad.y], scale),
        mm(pad.w * scale),
        mm(pad.h * scale),
        pad_layers(footprint_layer)
    );
}

/// Stable KiCad net number of a declared net; unassigned and unknown names are
/// `net 0`.
fn net_number(board: &Board, name: Option<&str>) -> u32 {
    let Some(name) = name
    else {
        return 0;
    };
    board.nets.iter().position(|net| net.name == name).map(|index| index as u32 + 1).unwrap_or(0)
}

/// Maps a board layer id to its KiCad name.
///
/// KiCad only accepts `F.Cu`, `B.Cu` and `In1.Cu`..`In30.Cu` as copper names;
/// a user-added inner layer keeps its document position but is written under
/// the corresponding KiCad name so the file is valid and imports back.
fn kicad_layer_name(board: &Board, id: &str) -> String {
    if id == "F.Cu" || id == "B.Cu" {
        return id.to_string();
    }
    let mut inner = 0_u32;
    for layer in &board.layers {
        if layer.kind != LayerKind::Copper || layer.id == "F.Cu" || layer.id == "B.Cu" {
            continue;
        }
        inner += 1;
        if layer.id == id {
            return format!("In{inner}.Cu");
        }
    }
    id.to_string()
}

/// Maps a board layer id to the stable KiCad layer number.
fn layer_number(id: &str, index: usize) -> u32 {
    match id {
        "F.Cu" => 0,
        "B.Cu" => 31,
        "F.SilkS" => 35,
        "B.SilkS" => 36,
        "F.Paste" => 37,
        "B.Paste" => 38,
        "F.Mask" => 39,
        "B.Mask" => 40,
        "Edge.Cuts" => 44,
        "F.Fab" => 48,
        "B.Fab" => 49,
        _ => match inner_layer_number(id) {
            Some(number) => number,
            None => 100 + index as u32,
        },
    }
}

/// Number of `In<N>.Cu`, when it is a layer KiCad can address.
fn inner_layer_number(id: &str) -> Option<u32> {
    let rest = id.strip_prefix("In")?.strip_suffix(".Cu")?;
    match rest.parse::<u32>() {
        Ok(number) if (1..=30).contains(&number) => Some(number),
        _ => None,
    }
}

fn reference_layer(footprint_layer: &str) -> &'static str {
    if footprint_layer.starts_with("B.") { "B.SilkS" } else { "F.SilkS" }
}

fn value_layer(footprint_layer: &str) -> &'static str {
    if footprint_layer.starts_with("B.") { "B.Fab" } else { "F.Fab" }
}

fn pad_layers(footprint_layer: &str) -> &'static str {
    if footprint_layer.starts_with("B.") { "\"B.Cu\" \"B.Paste\" \"B.Mask\"" } else { "\"F.Cu\" \"F.Paste\" \"F.Mask\"" }
}

/// Formats one point as `x y` in millimetres.
fn point(value: [f64; 2], scale: f64) -> String {
    format!("{} {}", mm(value[0] * scale), mm(value[1] * scale))
}

/// Formats millimetres with four decimals and no trailing zeros.
fn mm(value: f64) -> String {
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

/// Escapes one S-expression string literal.
fn sexpr(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
