//! Copper pour connectivity audit.
//!
//! Every stored pour polygon is measured against the copper of its own net on
//! the same layer: pads, vias and track segments. A pour with no anchor is an
//! isolated island, and a pour without a declared net is unassigned copper. The
//! report is read-only and lists the anchors so a reviewer can verify the count.

use geometry_planar::{Point as GeoPoint, point_segment_distance};
use serde_json::{Value, json};

use crate::{
    board::{Board, polygon_area},
    round6,
};

/// Builds the copper pour connectivity report of one board.
pub(crate) fn report(board: &Board) -> Value {
    let scale = board.millimetres();
    let factor = scale * scale;
    let pads = board.placed_pads();
    let mut issues: Vec<Value> = Vec::new();
    let mut zones: Vec<Value> = Vec::with_capacity(board.zones.len());
    let mut connected = 0usize;
    let mut isolated = 0usize;
    let mut unassigned = 0usize;
    let mut total_area = 0.0;
    let mut isolated_area = 0.0;

    for zone in &board.zones {
        let area = polygon_area(&zone.points).abs() * factor;
        total_area += area;
        let pad_count = match zone.net.as_deref() {
            Some(net) => pads
                .iter()
                .filter(|pad| pad.net.as_deref() == Some(net) && pad.layer == zone.layer)
                .filter(|pad| box_touches_polygon(pad.at, pad.half, &zone.points))
                .count(),
            None => 0,
        };
        let via_count = match zone.net.as_deref() {
            Some(net) => board
                .vias
                .iter()
                .filter(|via| via.net.as_deref() == Some(net) && board.via_covers(via, &zone.layer))
                .filter(|via| circle_touches_polygon(via.at, via.diameter / 2.0, &zone.points))
                .count(),
            None => 0,
        };
        let track_count = match zone.net.as_deref() {
            Some(net) => board
                .tracks
                .iter()
                .filter(|track| track.net.as_deref() == Some(net) && track.layer == zone.layer)
                .filter(|track| segment_touches_polygon(track.from, track.to, &zone.points))
                .count(),
            None => 0,
        };
        let anchors = pad_count + via_count + track_count;
        let status = match (&zone.net, anchors > 0) {
            (None, _) => "unassigned",
            (Some(_), true) => "connected",
            (Some(_), false) => "isolated",
        };
        match status {
            "connected" => connected += 1,
            "isolated" => {
                isolated += 1;
                isolated_area += area;
                issues.push(issue(
                    "zone/isolated",
                    "warning",
                    &zone.id,
                    &zone.layer,
                    zone.net.as_deref(),
                    format!(
                        "pour `{}` on {} reaches no pad, via or same-net track of net `{}`; it is an isolated island",
                        zone.id,
                        zone.layer,
                        zone.net.as_deref().unwrap_or("")
                    ),
                ));
            }
            _ => {
                unassigned += 1;
                issues.push(issue(
                    "zone/unassigned-net",
                    "warning",
                    &zone.id,
                    &zone.layer,
                    None,
                    format!("pour `{}` on {} has no net; it carries copper outside every net", zone.id, zone.layer),
                ));
            }
        }
        zones.push(json!({
            "id": zone.id,
            "layer": zone.layer,
            "net": zone.net,
            "vertices": zone.points.len(),
            "area": round6(area),
            "perimeter": round6(perimeter(&zone.points) * scale),
            "anchors": { "pads": pad_count, "vias": via_count, "tracks": track_count },
            "status": status,
        }));
    }

    let mut layers: Vec<Value> = Vec::new();
    for layer in board.copper_stack() {
        let entries: Vec<&crate::board::Zone> = board.zones.iter().filter(|zone| zone.layer == layer).collect();
        if entries.is_empty() {
            continue;
        }
        let mut layer_connected = 0usize;
        let mut layer_isolated = 0usize;
        let mut layer_unassigned = 0usize;
        let mut layer_area = 0.0;
        let mut layer_isolated_area = 0.0;
        for zone in entries {
            let area = polygon_area(&zone.points).abs() * factor;
            layer_area += area;
            let status = zones
                .iter()
                .find(|value| value["id"] == zone.id)
                .and_then(|value| value["status"].as_str())
                .unwrap_or("isolated");
            match status {
                "connected" => layer_connected += 1,
                "isolated" => {
                    layer_isolated += 1;
                    layer_isolated_area += area;
                }
                _ => layer_unassigned += 1,
            }
        }
        layers.push(json!({
            "layer": layer,
            "zones": layer_connected + layer_isolated + layer_unassigned,
            "connected": layer_connected,
            "isolated": layer_isolated,
            "unassigned": layer_unassigned,
            "area": round6(layer_area),
            "isolatedArea": round6(layer_isolated_area),
        }));
    }

    json!({
        "report": "open-pcb-zone-audit",
        "revision": board.revision,
        "units": "mm2",
        "status": if issues.is_empty() { "pass" } else { "warn" },
        "zoneCount": zones.len(),
        "connectedCount": connected,
        "isolatedCount": isolated,
        "unassignedCount": unassigned,
        "totalArea": round6(total_area),
        "isolatedArea": round6(isolated_area),
        "layers": layers,
        "zones": zones,
        "issues": issues,
    })
}

/// Polygon perimeter in board units.
fn perimeter(points: &[[f64; 2]]) -> f64 {
    points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let next = points[(index + 1) % points.len()];
            (next[0] - point[0]).hypot(next[1] - point[1])
        })
        .sum()
}

/// Builds one audit finding.
fn issue(code: &str, severity: &str, object_id: &str, layer: &str, net: Option<&str>, message: String) -> Value {
    json!({ "code": code, "severity": severity, "objectId": object_id, "layer": layer, "net": net, "message": message })
}

/// Whether a point lies inside a closed polygon.
fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let [xi, yi] = polygon[current];
        let [xj, yj] = polygon[previous];
        if (yi > point[1]) != (yj > point[1]) {
            let cross = (xj - xi) * (point[1] - yi) / (yj - yi) + xi;
            if point[0] < cross {
                inside = !inside;
            }
        }
        previous = current;
    }
    inside
}

/// Whether a track segment reaches the polygon: an endpoint inside it or a
/// crossing of one of its edges.
fn segment_touches_polygon(from: [f64; 2], to: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    if point_in_polygon(from, polygon) || point_in_polygon(to, polygon) {
        return true;
    }
    polygon.iter().enumerate().any(|(index, point)| segments_intersect(from, to, *point, polygon[(index + 1) % polygon.len()]))
}

/// Whether an axis-aligned pad box touches the polygon.
fn box_touches_polygon(centre: [f64; 2], half: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    if point_in_polygon(centre, polygon) {
        return true;
    }
    let corners = [
        [centre[0] - half[0], centre[1] - half[1]],
        [centre[0] + half[0], centre[1] - half[1]],
        [centre[0] + half[0], centre[1] + half[1]],
        [centre[0] - half[0], centre[1] + half[1]],
    ];
    if corners.iter().any(|corner| point_in_polygon(*corner, polygon)) {
        return true;
    }
    if polygon.iter().any(|point| {
        (point[0] - centre[0]).abs() <= half[0] + f64::EPSILON && (point[1] - centre[1]).abs() <= half[1] + f64::EPSILON
    }) {
        return true;
    }
    corners.iter().enumerate().any(|(index, corner)| {
        let next = corners[(index + 1) % corners.len()];
        polygon
            .iter()
            .enumerate()
            .any(|(edge, point)| segments_intersect(*corner, next, *point, polygon[(edge + 1) % polygon.len()]))
    })
}

/// Whether a via circle touches the polygon.
fn circle_touches_polygon(centre: [f64; 2], radius: f64, polygon: &[[f64; 2]]) -> bool {
    if point_in_polygon(centre, polygon) {
        return true;
    }
    polygon.iter().enumerate().any(|(index, point)| {
        let next = polygon[(index + 1) % polygon.len()];
        point_segment_distance(
            GeoPoint::new(centre[0], centre[1]),
            GeoPoint::new(point[0], point[1]),
            GeoPoint::new(next[0], next[1]),
        ) <= radius + f64::EPSILON
    })
}

/// Whether two closed segments intersect or touch.
fn segments_intersect(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let o1 = orientation(a, b, c);
    let o2 = orientation(a, b, d);
    let o3 = orientation(c, d, a);
    let o4 = orientation(c, d, b);
    if ((o1 > 0.0 && o2 < 0.0) || (o1 < 0.0 && o2 > 0.0)) && ((o3 > 0.0 && o4 < 0.0) || (o3 < 0.0 && o4 > 0.0)) {
        return true;
    }
    (o1 == 0.0 && on_segment(a, b, c))
        || (o2 == 0.0 && on_segment(a, b, d))
        || (o3 == 0.0 && on_segment(c, d, a))
        || (o4 == 0.0 && on_segment(c, d, b))
}

/// Signed area of the triangle `a-b-c`; zero when the points are collinear.
fn orientation(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Whether collinear point `c` lies within the bounding box of `a-b`.
fn on_segment(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> bool {
    c[0] >= a[0].min(b[0]) && c[0] <= a[0].max(b[0]) && c[1] >= a[1].min(b[1]) && c[1] <= a[1].max(b[1])
}
