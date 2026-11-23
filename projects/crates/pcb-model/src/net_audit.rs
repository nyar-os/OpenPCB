//! Net class, width and clearance audit.
//!
//! Every declared net reports the class it belongs to, that class's targets and
//! the copper actually routed on it. Clearance is measured between track
//! segments of two different declared nets on a shared copper layer, as the
//! edge-to-edge gap in millimetres; the required value is the strictest class
//! clearance of the pair, or the baseline the other checks use. The report is
//! read-only and never edits the board.

use geometry_planar::{Point as GeoPoint, segment_distance};
use serde_json::{Value, json};

use crate::{
    board::{Board, LayerKind, Track},
    inspect::MIN_CLEARANCE_MM,
    round6,
};

/// Builds the net class, width and clearance audit of one board.
///
/// Only declared nets are listed, in declaration order. Tracks on a layer that
/// is not copper are ignored, and copper whose net name is not declared is not
/// measured. Pads, vias and pours take no part in the clearance measurement.
pub(crate) fn report(board: &Board) -> Value {
    let scale = board.millimetres();
    let copper = |track: &Track| board.layer(&track.layer).is_some_and(|layer| layer.kind == LayerKind::Copper);
    let net_tracks: Vec<Vec<&Track>> = board
        .nets
        .iter()
        .map(|net| {
            board.tracks.iter().filter(|track| track.net.as_deref() == Some(net.name.as_str()) && copper(track)).collect()
        })
        .collect();
    let mut pads: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for pad in board.placed_pads() {
        if let Some(net) = pad.net.as_deref() {
            *pads.entry(net.to_string()).or_default() += 1;
        }
    }

    let mut issues: Vec<Value> = Vec::new();
    let mut pairs: Vec<Value> = Vec::with_capacity(board.nets.len());
    let mut closest: Vec<Option<(usize, f64, f64)>> = vec![None; board.nets.len()];

    for left in 0..board.nets.len() {
        for right in (left + 1)..board.nets.len() {
            let Some((actual, layer, left_track, right_track)) = pair_clearance(&net_tracks[left], &net_tracks[right], scale)
            else {
                continue;
            };
            let required = required_clearance(board, &board.nets[left].name, &board.nets[right].name, scale);
            let ok = actual + 1e-9 >= required;
            remember(&mut closest[left], right, actual, required);
            remember(&mut closest[right], left, actual, required);
            if !ok {
                issues.push(json!({
                    "code": "net/clearance-below-class",
                    "severity": "error",
                    "objectId": left_track,
                    "net": board.nets[left].name,
                    "other": board.nets[right].name,
                    "layer": layer,
                    "message": format!(
                        "tracks `{left_track}` ({}) and `{right_track}` ({}) are {actual:.3} mm apart on {layer}; {required:.3} mm is required",
                        board.nets[left].name, board.nets[right].name
                    ),
                }));
            }
            pairs.push(json!({
                "a": board.nets[left].name,
                "b": board.nets[right].name,
                "actual": actual,
                "required": required,
                "ok": ok,
                "layer": layer,
                "aTrack": left_track,
                "bTrack": right_track,
            }));
        }
    }

    let mut nets: Vec<Value> = Vec::with_capacity(board.nets.len());
    for (index, net) in board.nets.iter().enumerate() {
        let class = board.class_of_net(&net.name);
        let targets = json!({
            "width": class.and_then(|class| class.width).map(|width| round6(width * scale)),
            "clearance": class.and_then(|class| class.clearance).map(|clearance| round6(clearance * scale)),
            "viaDiameter": class.and_then(|class| class.via_diameter).map(|diameter| round6(diameter * scale)),
            "viaDrill": class.and_then(|class| class.via_drill).map(|drill| round6(drill * scale)),
        });
        let widths: Vec<f64> = net_tracks[index].iter().map(|track| track.width * scale).collect();
        let width_min = widths.iter().copied().reduce(f64::min).map(round6);
        let width_max = widths.iter().copied().reduce(f64::max).map(round6);
        let target_width = targets["width"].as_f64();
        let narrow = target_width.map_or(0, |target| widths.iter().filter(|width| **width + 1e-9 < target).count());
        if let (Some(target), true) = (target_width, narrow > 0) {
            let worst = round6(widths.iter().copied().reduce(f64::min).unwrap_or(target));
            issues.push(json!({
                "code": "net/width-below-class",
                "severity": "error",
                "objectId": net.id,
                "net": net.name,
                "other": Value::Null,
                "layer": Value::Null,
                "message": format!(
                    "net `{}` routes {narrow} track(s) narrower than the {target:.3} mm class width; the narrowest is {worst:.3} mm",
                    net.name
                ),
            }));
        }
        let width_status = match (target_width, widths.is_empty(), narrow) {
            (Some(_), false, 0) => "pass",
            (Some(_), false, _) => "below",
            _ => "none",
        };
        let vias: Vec<&crate::board::Via> =
            board.vias.iter().filter(|via| via.net.as_deref() == Some(net.name.as_str())).collect();
        let diameters: Vec<f64> = vias.iter().map(|via| via.diameter * scale).collect();
        let drills: Vec<f64> = vias.iter().map(|via| via.drill * scale).collect();
        let clearance = closest[index].map(|(partner, actual, required)| {
            json!({
                "to": board.nets[partner].name,
                "actual": actual,
                "required": required,
                "ok": actual + 1e-9 >= required,
            })
        });
        nets.push(json!({
            "name": net.name,
            "class": class.map(|class| class.name.clone()),
            "targets": targets,
            "tracks": net_tracks[index].len(),
            "width": width_min.zip(width_max).map(|(min, max)| json!({ "min": min, "max": max })),
            "vias": vias.len(),
            "viaDiameter": diameters.iter().copied().reduce(f64::min).map(round6),
            "viaDrill": drills.iter().copied().reduce(f64::min).map(round6),
            "pads": pads.get(net.name.as_str()).copied().unwrap_or(0),
            "widthStatus": width_status,
            "clearance": clearance,
        }));
    }

    let status = if issues.iter().any(|issue| issue["severity"] == "error") {
        "error"
    }
    else if issues.is_empty() {
        "pass"
    }
    else {
        "warn"
    };
    json!({
        "report": "open-pcb-net-audit",
        "revision": board.revision,
        "units": "mm",
        "status": status,
        "netCount": nets.len(),
        "classCount": board.net_classes.len(),
        "pairCount": pairs.len(),
        "nets": nets,
        "pairs": pairs,
        "issues": issues,
    })
}

/// Keeps the narrowest measured pair that touches one net.
fn remember(slot: &mut Option<(usize, f64, f64)>, partner: usize, actual: f64, required: f64) {
    match *slot {
        Some((_, current, _)) if current <= actual => {}
        _ => *slot = Some((partner, actual, required)),
    }
}

/// Narrowest edge-to-edge gap between two track sets on a shared copper layer.
///
/// Returns the rounded gap in millimetres, the layer and the two track ids.
fn pair_clearance<'a>(left: &[&'a Track], right: &[&'a Track], scale: f64) -> Option<(f64, &'a str, &'a str, &'a str)> {
    let mut best: Option<(f64, &'a str, &'a str, &'a str)> = None;
    for first in left {
        for second in right {
            if first.layer != second.layer {
                continue;
            }
            let centres = segment_distance(
                GeoPoint::new(first.from[0], first.from[1]),
                GeoPoint::new(first.to[0], first.to[1]),
                GeoPoint::new(second.from[0], second.from[1]),
                GeoPoint::new(second.to[0], second.to[1]),
            );
            let gap = (centres - (first.width + second.width) / 2.0) * scale;
            if best.is_none_or(|(current, _, _, _)| gap < current) {
                best = Some((gap, first.layer.as_str(), first.id.as_str(), second.id.as_str()));
            }
        }
    }
    best.map(|(gap, layer, first, second)| (round6(gap), layer, first, second))
}

/// Clearance two nets need, taking the strictest class value.
fn required_clearance(board: &Board, left: &str, right: &str, scale: f64) -> f64 {
    let mut required = MIN_CLEARANCE_MM;
    for net in [left, right] {
        if let Some(clearance) = board.class_of_net(net).and_then(|class| class.clearance) {
            required = required.max(clearance * scale);
        }
    }
    round6(required)
}
