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
