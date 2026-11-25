//! Copper area and balance report.
//!
//! The report sums the drawn copper of every feature per copper layer: track
//! length times width, via and pad footprints and pour polygon area. Features can
//! overlap, so the total is a feature-area sum rather than a union area. With a
//! rectangular outline the report also divides each layer by the board area, so
//! an uneven copper distribution shows up as a coverage imbalance.

use std::f64::consts::PI;

use serde_json::{Value, json};

use crate::{
    board::{Board, LayerKind, polygon_area},
    round6,
};

/// Builds the copper pour area report of one board.
///
/// Every zone contributes its enclosed polygon area in square millimetres and
/// its perimeter in millimetres. Layers are listed in stack order; the report
/// carries the same total the balance report sees for zones.
pub(crate) fn zones(board: &Board) -> Value {
    let scale = board.millimetres();
    let factor = scale * scale;
    let mut per_layer: Vec<Value> = Vec::new();
    let mut total = 0.0;
    for layer in board.layers.iter().filter(|layer| layer.kind == LayerKind::Copper) {
        let entries: Vec<&crate::board::Zone> = board.zones.iter().filter(|zone| zone.layer == layer.id).collect();
        if entries.is_empty() {
            continue;
        }
        let area = round6(entries.iter().map(|zone| polygon_area(&zone.points).abs() * factor).sum());
        total += area;
        per_layer.push(json!({ "layer": layer.id, "zones": entries.len(), "area": area }));
    }
    let zones = board
        .zones
        .iter()
        .map(|zone| {
            let perimeter = zone
                .points
                .iter()
                .enumerate()
                .map(|(index, point)| {
                    let next = zone.points[(index + 1) % zone.points.len()];
                    (next[0] - point[0]).hypot(next[1] - point[1]) * scale
                })
                .sum::<f64>();
            json!({
                "id": zone.id,
                "layer": zone.layer,
                "net": zone.net,
                "vertices": zone.points.len(),
                "fill": zone.fill,
                "gridSpacing": (zone.fill == "grid").then(|| round6(zone.grid_spacing() * scale)),
                "gridWidth": (zone.fill == "grid").then(|| round6(zone.grid_width() * scale)),
                "area": round6(polygon_area(&zone.points).abs() * factor),
                "perimeter": round6(perimeter),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "revision": board.revision,
        "units": "mm2",
        "count": zones.len(),
        "totalArea": round6(total),
        "layers": per_layer,
        "zones": zones,
    })
}

/// Builds the routed net-length report of one board.
///
/// Only nets declared in the document are listed, in declaration order. A net
/// with no copper reports zeros. Copper without a declared net is not reported.
/// Tracks are straight segments, so each length is the exact sum of its segment
/// lengths in document units; vias are counted, not measured. The `ranking`
/// array orders the same nets by length, longest first; equal rounded lengths
/// fall back to the net name so the order is deterministic.
pub(crate) fn net_lengths(board: &Board) -> Value {
    let mut nets: Vec<Value> = Vec::with_capacity(board.nets.len());
    let mut ranking: Vec<(f64, &str)> = Vec::with_capacity(board.nets.len());
    let mut total_tracks = 0usize;
    let mut total_length = 0.0;
    let mut total_vias = 0usize;
    for net in &board.nets {
        let tracks: Vec<&crate::board::Track> =
            board.tracks.iter().filter(|track| track.net.as_deref() == Some(net.name.as_str())).collect();
        let vias = board.vias.iter().filter(|via| via.net.as_deref() == Some(net.name.as_str())).count();
        let length: f64 = tracks.iter().map(|track| (track.to[0] - track.from[0]).hypot(track.to[1] - track.from[1])).sum();
        total_tracks += tracks.len();
        total_length += length;
        total_vias += vias;
        let length = round6(length);
        ranking.push((length, net.name.as_str()));
        nets.push(json!({
            "name": net.name,
            "tracks": tracks.len(),
            "length": length,
            "vias": vias,
        }));
    }
    ranking.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    let ranking: Vec<Value> = ranking.iter().map(|(length, name)| json!({ "name": name, "length": length })).collect();
    json!({
        "report": "open-pcb-net-lengths",
        "revision": board.revision,
        "nets": nets,
        "ranking": ranking,
        "totals": {
            "tracks": total_tracks,
            "length": round6(total_length),
            "vias": total_vias,
        },
    })
}

/// Builds the routed length audit of one board.
///
/// Every declared net is measured in millimetres: `trackLength` sums the
/// straight track segments, `viaLength` sums the via barrels spanning the
/// dielectric between the copper layers each via touches, and `totalLength`
/// adds the two. Barrels need a declared stackup; without one `viaLength` and
/// `totalLength` come back null and the track length stands alone. Declared
/// differential pairs compare their halves: `deviation` is the absolute
/// difference of the measured lengths — the total when barrels are measurable,
/// otherwise the track length — and `target` is the pair's `maxSkew`; `status`
/// is `pass` while the deviation stays within the target.
pub(crate) fn length_audit(board: &Board) -> Value {
    let scale = board.millimetres();
    let dielectric = board.stackup.as_ref().map(|stackup| stackup.dielectric_height);
    let mut totals: Vec<(&str, f64, Option<f64>)> = Vec::with_capacity(board.nets.len());
    let mut nets: Vec<Value> = Vec::with_capacity(board.nets.len());
    let (mut total_tracks, mut total_vias) = (0.0, 0.0);
    for net in &board.nets {
        let track_length = board
            .tracks
            .iter()
            .filter(|track| track.net.as_deref() == Some(net.name.as_str()))
            .map(|track| (track.to[0] - track.from[0]).hypot(track.to[1] - track.from[1]))
            .sum::<f64>()
            * scale;
        let via_length = dielectric.map(|height| {
            board
                .vias
                .iter()
                .filter(|via| via.net.as_deref() == Some(net.name.as_str()))
                .filter_map(|via| board.via_span(via))
                .map(|(low, high)| (high - low) as f64 * height)
                .sum::<f64>()
        });
        total_tracks += track_length;
        total_vias += via_length.unwrap_or_default();
        let track_length = round6(track_length);
        let via_length = via_length.map(round6);
        let total_length = via_length.map(|vias| round6(track_length + vias));
        nets.push(json!({
            "name": net.name,
            "trackLength": track_length,
            "viaLength": via_length,
            "totalLength": total_length,
        }));
        totals.push((net.name.as_str(), track_length, total_length));
    }
    let measured = |name: &str| totals.iter().find(|(net, ..)| *net == name).map(|(_, track, total)| total.unwrap_or(*track));
    let pairs: Vec<Value> = board
        .differential_pairs
        .iter()
        .map(|pair| {
            let positive = measured(&pair.positive).unwrap_or_default();
            let negative = measured(&pair.negative).unwrap_or_default();
            let deviation = round6((positive - negative).abs());
            let target = round6(pair.max_skew * scale);
            json!({
                "id": pair.id,
                "positive": pair.positive,
                "negative": pair.negative,
                "positiveLength": round6(positive),
                "negativeLength": round6(negative),
                "deviation": deviation,
                "target": target,
                "status": if deviation <= target + 1e-9 { "pass" } else { "fail" },
            })
        })
        .collect();
    json!({
        "report": "open-pcb-length-audit",
        "revision": board.revision,
        "units": "mm",
        "barrelsMeasured": dielectric.is_some(),
        "nets": nets,
        "pairs": pairs,
        "totals": {
            "trackLength": round6(total_tracks),
            "viaLength": dielectric.map(|_| round6(total_vias)),
            "totalLength": dielectric.map(|_| round6(total_tracks + total_vias)),
        },
    })
}

/// Builds the copper area and balance report of one board.
pub(crate) fn report(board: &Board) -> Value {
    let scale = board.millimetres();
    let factor = scale * scale;
    let board_area = board.outline.map(|outline| {
        polygon_area(&[outline.from, [outline.to[0], outline.from[1]], outline.to, [outline.from[0], outline.to[1]]]).abs()
            * factor
    });
    let pads = board.placed_pads();

    let mut layers = Vec::new();
    let mut total = 0.0;
    let mut coverages: Vec<f64> = Vec::new();
    for layer in board.layers.iter().filter(|layer| layer.kind == LayerKind::Copper) {
        let tracks: f64 = board
            .tracks
            .iter()
            .filter(|track| track.layer == layer.id)
            .map(|track| (track.to[0] - track.from[0]).hypot(track.to[1] - track.from[1]) * track.width * factor)
            .sum();
        let vias: f64 = board
            .vias
            .iter()
            .filter(|via| board.via_covers(via, &layer.id))
            .map(|via| PI * (via.diameter / 2.0_f64).powi(2) * factor)
            .sum();
        let pad_area: f64 =
            pads.iter().filter(|pad| pad.layer == layer.id).map(|pad| pad.half[0] * 2.0 * pad.half[1] * 2.0 * factor).sum();
        let zones: f64 = board
            .zones
            .iter()
            .filter(|zone| zone.layer == layer.id)
            .map(|zone| polygon_area(&zone.points).abs() * factor)
            .sum();
        let area = tracks + vias + pad_area + zones;
        total += area;
        let coverage = board_area.filter(|area| *area > 0.0).map(|board_area| area / board_area * 100.0);
        if let Some(coverage) = coverage {
            coverages.push(coverage);
        }
        layers.push(json!({
            "layer": layer.id,
            "area": round6(area),
            "tracks": round6(tracks),
            "vias": round6(vias),
            "pads": round6(pad_area),
            "zones": round6(zones),
            "coveragePercent": coverage.map(round6),
        }));
    }

    let min = coverages.iter().copied().reduce(f64::min);
    let max = coverages.iter().copied().reduce(f64::max);
    json!({
        "revision": board.revision,
        "units": "mm2",
        "boardArea": board_area.map(round6),
        "totalArea": round6(total),
        "layers": layers,
        "minCoveragePercent": min.map(round6),
        "maxCoveragePercent": max.map(round6),
        "imbalancePercent": min.zip(max).map(|(min, max)| round6(max - min)),
    })
}
