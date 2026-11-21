//! Deterministic two-layer autorouting.
//!
//! The router builds one minimum spanning tree per net from pad centres and
//! tries a fixed, ordered list of orthogonal candidates for every edge. Nets are
//! routed hardest-first (largest pad bounding box, then pad count, then name)
//! and each edge prefers the layer that matches its dominant direction. When a
//! greedy pass leaves edges unrouted, the router rips up its own copper and
//! retries with the failed nets routed first and the layer preference flipped,
//! for a bounded number of rounds. It never invents connectivity: an edge either
//! produces collision-free copper or is reported as unrouted.

use std::collections::BTreeMap;

use geometry_planar::{
    Point as GeoPoint, Rect, point_segment_distance as geo_point_segment_distance, segment_distance as geo_segment_distance,
};
use serde_json::{Value, json};

use crate::{
    ServiceError, argument,
    board::{Board, LayerKind, RouteOptions, Track, Via},
};

const DEFAULT_WIDTH: f64 = 0.25;
const DEFAULT_CLEARANCE: f64 = 0.2;
const DEFAULT_VIA_DIAMETER: f64 = 0.6;
const DEFAULT_VIA_DRILL: f64 = 0.3;
const DEFAULT_RETRIES: u32 = 2;
const MAX_RETRIES: u32 = 8;
const EPSILON: f64 = 1e-9;
const UNROUTED_REASON: &str = "no collision-free candidate on the configured layers";

#[derive(Debug, Clone)]
struct Pad {
    key: String,
    net: Option<String>,
    at: [f64; 2],
    half: [f64; 2],
    layer: String,
}

#[derive(Debug, Clone)]
struct TrackObstacle {
    net: Option<String>,
    layer: String,
    width: f64,
    from: [f64; 2],
    to: [f64; 2],
}

#[derive(Debug, Clone)]
struct ViaObstacle {
    net: Option<String>,
    at: [f64; 2],
    diameter: f64,
}

#[derive(Debug, Clone)]
struct Settings {
    layers: Vec<String>,
    width: f64,
    clearance: f64,
    via_diameter: f64,
    via_drill: f64,
    net: Option<String>,
    retries: u32,
}

#[derive(Debug, Clone)]
struct Accepted {
    net: String,
    layer: String,
    from: [f64; 2],
    to: [f64; 2],
}

struct Candidate {
    layer: String,
    segments: Vec<([f64; 2], [f64; 2])>,
    vias: Vec<[f64; 2]>,
}

/// One net's pads and the spanning edges the router will try.
#[derive(Debug, Clone)]
struct NetPlan {
    name: String,
    pads: Vec<Pad>,
    edges: Vec<(usize, usize)>,
}

/// Whether one spanning edge produced copper.
#[derive(Debug, Clone)]
struct EdgeResult {
    net: String,
    routed: bool,
}

/// A complete routing attempt; retries replace one attempt with the next.
#[derive(Debug, Clone, Default)]
struct Attempt {
    segments: Vec<Accepted>,
    vias: Vec<(String, [f64; 2])>,
    edges: Vec<EdgeResult>,
    unrouted: Vec<Value>,
    routed: usize,
}

/// Routes every net with at least two pads and returns the routing report.
///
/// The returned boolean is `true` when the board was mutated, so the caller can
/// avoid recording an empty undo step when nothing was routed.
pub(crate) fn run(board: &mut Board, options: RouteOptions) -> Result<(Value, bool), ServiceError> {
    let settings = resolve(&options)?;
    validate_layers(board, &settings.layers)?;
    if let Some(name) = settings.net.as_deref()
        && board.net(name).is_none()
    {
        return Err(argument(format!("net `{name}` is not declared on this board")));
    }

    let all_pads = board
        .placed_pads()
        .into_iter()
        .map(|pad| Pad { key: pad.key, net: pad.net, at: pad.at, half: pad.half, layer: pad.layer.to_string() })
        .collect::<Vec<_>>();
    let mut by_net: BTreeMap<String, Vec<Pad>> = BTreeMap::new();
    for pad in &all_pads {
        if let Some(filter) = settings.net.as_deref()
            && pad.net.as_deref() != Some(filter)
        {
            continue;
        }
        if let Some(net) = pad.net.clone() {
            by_net.entry(net).or_default().push(pad.clone());
        }
    }

    let mut plans: Vec<NetPlan> = Vec::new();
    for (name, mut pads) in by_net {
        if pads.len() < 2 {
            continue;
        }
        pads.sort_by(|left, right| left.key.cmp(&right.key));
        let edges = minimum_spanning_edges(&pads);
        plans.push(NetPlan { name, pads, edges });
    }
    plans.sort_by(|left, right| {
        let (left_diagonal, left_pads) = difficulty(left);
        let (right_diagonal, right_pads) = difficulty(right);
        right_diagonal.total_cmp(&left_diagonal).then(right_pads.cmp(&left_pads)).then_with(|| left.name.cmp(&right.name))
    });

    let base_tracks = board
        .tracks
        .iter()
        .map(|track| TrackObstacle {
            net: track.net.clone(),
            layer: track.layer.clone(),
            width: track.width,
            from: track.from,
            to: track.to,
        })
        .collect::<Vec<_>>();
    let base_vias = board
        .vias
        .iter()
        .map(|via| ViaObstacle { net: via.net.clone(), at: via.at, diameter: via.diameter })
        .collect::<Vec<_>>();
    let outline = board.outline.as_ref().map(|outline| (outline.from, outline.to));

    let mut order = (0..plans.len()).collect::<Vec<_>>();
    let mut current = attempt(board, &settings, &plans, &order, false, &all_pads, &base_tracks, &base_vias, outline);
    let mut best = current.clone();
    let mut retries_by_net = plans.iter().map(|plan| (plan.name.clone(), 0usize)).collect::<BTreeMap<_, _>>();
    let mut rounds = 0u32;
    while !current.unrouted.is_empty() && rounds < settings.retries {
        let failed = order
            .iter()
            .copied()
            .filter(|index| {
                let name = &plans[*index].name;
                current.edges.iter().any(|edge| edge.net == *name && !edge.routed)
            })
            .collect::<Vec<_>>();
        if failed.is_empty() {
            break;
        }
        for index in &failed {
            if let Some(count) = retries_by_net.get_mut(&plans[*index].name) {
                *count += 1;
            }
        }
        rounds += 1;
        let mut retry_order = failed.clone();
        retry_order.extend(order.iter().copied().filter(|index| !failed.contains(index)));
        order = retry_order;
        // Alternate the direction-preferred layer so a retry explores different
        // geometry instead of repeating the same greedy choice.
        current = attempt(board, &settings, &plans, &order, rounds % 2 == 1, &all_pads, &base_tracks, &base_vias, outline);
        if current.routed > best.routed {
            best = current.clone();
        }
    }

    let added_tracks = best.segments.len();
    let added_vias = best.vias.len();
    for segment in &best.segments {
        let id = format!("track-{}", board.next_track);
        board.next_track += 1;
        board.tracks.push(Track {
            id,
            net: Some(segment.net.clone()),
            layer: segment.layer.clone(),
            width: settings.width,
            from: segment.from,
            to: segment.to,
        });
    }
    for (net, at) in &best.vias {
        let id = format!("via-{}", board.next_via);
        board.next_via += 1;
        board.vias.push(Via {
            id,
            net: Some(net.clone()),
            at: *at,
            diameter: settings.via_diameter,
            drill: settings.via_drill,
            from_layer: None,
            to_layer: None,
            back_drill: None,
        });
    }
    let changed = added_tracks + added_vias > 0;
    if changed {
        board.revision += 1;
    }

    let mut totals: BTreeMap<String, (usize, usize)> = plans.iter().map(|plan| (plan.name.clone(), (0, 0))).collect();
    for edge in &best.edges {
        let entry = totals.entry(edge.net.clone()).or_default();
        entry.1 += 1;
        if edge.routed {
            entry.0 += 1;
        }
    }
    let nets = totals
        .iter()
        .map(|(name, (routed_edges, total))| {
            let status = match routed_edges {
                done if done == total => "routed",
                0 => "unrouted",
                _ => "partial",
            };
            json!({
                "net": name,
                "status": status,
                "edges": *total,
                "routed": *routed_edges,
                "unrouted": *total - *routed_edges,
                "retries": retries_by_net.get(name).copied().unwrap_or(0),
            })
        })
        .collect::<Vec<_>>();

    let report = json!({
        "routed": best.routed,
        "unrouted": best.unrouted,
        "tracks": added_tracks,
        "vias": added_vias,
        "retries": rounds,
        "nets": nets,
    });
    Ok((report, changed))
}

/// Runs one full routing pass and returns what it managed to place.
#[allow(clippy::too_many_arguments)]
fn attempt(
    board: &Board,
    settings: &Settings,
    plans: &[NetPlan],
    order: &[usize],
    flip: bool,
    pads: &[Pad],
    base_tracks: &[TrackObstacle],
    base_vias: &[ViaObstacle],
    outline: Option<([f64; 2], [f64; 2])>,
) -> Attempt {
    let mut tracks = base_tracks.to_vec();
    let mut vias = base_vias.to_vec();
    let mut result = Attempt::default();
    for &index in order {
        let plan = &plans[index];
        for &(from, to) in &plan.edges {
            let pad_a = &plan.pads[from];
            let pad_b = &plan.pads[to];
            let chosen = candidates(settings, pad_a.at, pad_b.at, flip).into_iter().find(|candidate| {
                candidate_clear(
                    board,
                    settings,
                    candidate,
                    &plan.name,
                    &pad_a.key,
                    &pad_b.key,
                    &tracks,
                    &vias,
                    pads,
                    &result.segments,
                    outline,
                )
            });
            let Some(candidate) = chosen
            else {
                result.unrouted.push(json!({
                    "net": plan.name,
                    "from": pad_a.key,
                    "to": pad_b.key,
                    "reason": UNROUTED_REASON,
                }));
                result.edges.push(EdgeResult { net: plan.name.clone(), routed: false });
                continue;
            };
            result.routed += 1;
            for (segment_from, segment_to) in &candidate.segments {
                tracks.push(TrackObstacle {
                    net: Some(plan.name.clone()),
                    layer: candidate.layer.clone(),
                    width: settings.width,
                    from: *segment_from,
                    to: *segment_to,
                });
                result.segments.push(Accepted {
                    net: plan.name.clone(),
                    layer: candidate.layer.clone(),
                    from: *segment_from,
                    to: *segment_to,
                });
            }
            for at in &candidate.vias {
                vias.push(ViaObstacle { net: Some(plan.name.clone()), at: *at, diameter: settings.via_diameter });
                result.vias.push((plan.name.clone(), *at));
            }
            result.edges.push(EdgeResult { net: plan.name.clone(), routed: true });
        }
    }
    result
}

/// Pad bounding-box diagonal and pad count, used to route harder nets first.
fn difficulty(plan: &NetPlan) -> (f64, usize) {
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for pad in &plan.pads {
        min[0] = min[0].min(pad.at[0]);
        min[1] = min[1].min(pad.at[1]);
        max[0] = max[0].max(pad.at[0]);
        max[1] = max[1].max(pad.at[1]);
    }
    let diagonal = ((max[0] - min[0]).powi(2) + (max[1] - min[1]).powi(2)).sqrt();
    (diagonal, plan.pads.len())
}

/// Resolves defaults and validates the option values.
fn resolve(options: &RouteOptions) -> Result<Settings, ServiceError> {
    let layers = match options.layers.clone() {
        Some(layers) => layers,
        None => vec!["F.Cu".to_string(), "B.Cu".to_string()],
    };
    if layers.is_empty() {
        return Err(argument("`layers` must name at least one copper layer"));
    }
    if layers.len() > 2 {
        return Err(argument("`layers` must name one or two copper layers"));
    }
    if layers.iter().any(|layer| layer.trim().is_empty()) {
        return Err(argument("a layer name must not be empty"));
    }
    if layers.len() == 2 && layers[0] == layers[1] {
        return Err(argument("`layers` must not repeat a layer"));
    }
    let width = number(options.width, DEFAULT_WIDTH, "width")?;
    let clearance = number(options.clearance, DEFAULT_CLEARANCE, "clearance")?;
    let via_diameter = number(options.via_diameter, DEFAULT_VIA_DIAMETER, "viaDiameter")?;
    let via_drill = number(options.via_drill, DEFAULT_VIA_DRILL, "viaDrill")?;
    if via_drill >= via_diameter {
        return Err(argument("`viaDrill` must be smaller than `viaDiameter`"));
    }
    Ok(Settings {
        layers,
        width,
        clearance,
        via_diameter,
        via_drill,
        net: options.net.clone(),
        retries: options.retries.unwrap_or(DEFAULT_RETRIES).min(MAX_RETRIES),
    })
}

fn number(value: Option<f64>, fallback: f64, field: &str) -> Result<f64, ServiceError> {
    let value = value.unwrap_or(fallback);
    if !value.is_finite() || value <= 0.0 {
        return Err(argument(format!("`{field}` must be a finite number greater than zero")));
    }
    Ok(value)
}

/// Rejects missing, non-copper or locked routing layers.
fn validate_layers(board: &Board, layers: &[String]) -> Result<(), ServiceError> {
    for id in layers {
        let layer = board
            .layers
            .iter()
            .find(|layer| layer.id == *id)
            .ok_or_else(|| argument(format!("layer `{id}` is not declared on this board")))?;
        if layer.kind != LayerKind::Copper {
            return Err(argument(format!("layer `{id}` is not a copper layer")));
        }
        if layer.locked {
            return Err(ServiceError::new("board/layer-locked", format!("layer `{id}` is locked")));
        }
    }
    Ok(())
}

/// Prim's algorithm over pad centres; ties resolve on sorted pad order.
fn minimum_spanning_edges(pads: &[Pad]) -> Vec<(usize, usize)> {
    let mut in_tree = vec![false; pads.len()];
    let mut edges = Vec::with_capacity(pads.len() - 1);
    in_tree[0] = true;
    for _ in 1..pads.len() {
        let mut best: Option<(f64, usize, usize)> = None;
        for left in 0..pads.len() {
            if !in_tree[left] {
                continue;
            }
            for right in 0..pads.len() {
                if in_tree[right] {
                    continue;
                }
                let distance = squared_distance(pads[left].at, pads[right].at);
                let keeps = best.is_none_or(|(current, at_left, at_right)| {
                    distance < current - EPSILON
                        || ((distance - current).abs() <= EPSILON && (left, right) < (at_left, at_right))
                });
                if keeps {
                    best = Some((distance, left, right));
                }
            }
        }
        if let Some((_, left, right)) = best {
            in_tree[right] = true;
            edges.push((left, right));
        }
    }
    edges
}

fn squared_distance(left: [f64; 2], right: [f64; 2]) -> f64 {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    dx * dx + dy * dy
}

/// Candidate order: the direction-preferred layer first, then the other layer
/// with a via at each endpoint. Within a layer the corner that starts along the
/// edge's dominant direction is tried first.
fn candidates(settings: &Settings, from: [f64; 2], to: [f64; 2], flip: bool) -> Vec<Candidate> {
    let horizontal = (to[0] - from[0]).abs() >= (to[1] - from[1]).abs();
    let mut candidates = Vec::new();
    for index in layer_order(settings.layers.len(), horizontal, flip) {
        let orientation = if horizontal { [true, false] } else { [false, true] };
        for horizontal_first in orientation {
            let points = corners(from, to, horizontal_first);
            candidates.push(Candidate {
                layer: settings.layers[index].clone(),
                segments: segments(&points),
                vias: if index == 0 { Vec::new() } else { vec![from, to] },
            });
        }
    }
    candidates
}

/// Horizontal edges prefer the first configured layer, vertical edges the
/// second one; `flip` swaps that preference for retry rounds.
fn layer_order(count: usize, horizontal: bool, flip: bool) -> Vec<usize> {
    if count < 2 {
        return vec![0];
    }
    let base = if horizontal { 0 } else { 1 };
    let first = if flip { 1 - base } else { base };
    vec![first, 1 - first]
}

fn corners(from: [f64; 2], to: [f64; 2], horizontal_first: bool) -> Vec<[f64; 2]> {
    if horizontal_first { vec![from, [to[0], from[1]], to] } else { vec![from, [from[0], to[1]], to] }
}

fn segments(points: &[[f64; 2]]) -> Vec<([f64; 2], [f64; 2])> {
    points
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .filter(|(from, to)| squared_distance(*from, *to) > EPSILON * EPSILON)
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn candidate_clear(
    board: &Board,
    settings: &Settings,
    candidate: &Candidate,
    net: &str,
    from_key: &str,
    to_key: &str,
    tracks: &[TrackObstacle],
    vias: &[ViaObstacle],
    pads: &[Pad],
    accepted: &[Accepted],
    outline: Option<([f64; 2], [f64; 2])>,
) -> bool {
    let radius = settings.width / 2.0;
    for (start, end) in &candidate.segments {
        if let Some((min, max)) = outline
            && (!inside(*start, min, max, radius) || !inside(*end, min, max, radius))
        {
            return false;
        }
        for track in tracks.iter().filter(|track| track.layer == candidate.layer) {
            if track.net.as_deref() == Some(net) && endpoints_match(*start, *end, track.from, track.to) {
                continue;
            }
            let distance = segment_distance(*start, *end, track.from, track.to);
            if distance + EPSILON < settings.clearance + radius + track.width / 2.0 {
                return false;
            }
        }
        for via in vias {
            let distance = point_segment_distance(via.at, *start, *end);
            if distance + EPSILON < settings.clearance + radius + via.diameter / 2.0 {
                return false;
            }
        }
        for pad in pads.iter().filter(|pad| pad.layer == candidate.layer) {
            if pad.key == from_key || pad.key == to_key {
                continue;
            }
            let distance = segment_box_distance(*start, *end, pad.at, pad.half);
            if distance + EPSILON < settings.clearance + radius {
                return false;
            }
        }
        for other in accepted.iter().filter(|other| other.net == net && other.layer == candidate.layer) {
            if endpoints_match(*start, *end, other.from, other.to) {
                continue;
            }
            if segments_cross(*start, *end, other.from, other.to) {
                return false;
            }
        }
    }
    for at in &candidate.vias {
        if let Some((min, max)) = outline
            && !inside(*at, min, max, settings.via_diameter / 2.0)
        {
            return false;
        }
        for track in tracks.iter().filter(|track| copper(board, &track.layer)) {
            if track.net.as_deref() == Some(net) && (near(*at, track.from) || near(*at, track.to)) {
                continue;
            }
            let distance = point_segment_distance(*at, track.from, track.to);
            if distance + EPSILON < settings.clearance + settings.via_diameter / 2.0 + track.width / 2.0 {
                return false;
            }
        }
        for via in vias {
            if via.net.as_deref() == Some(net) && near(*at, via.at) {
                continue;
            }
            if distance(*at, via.at) + EPSILON < settings.clearance + (settings.via_diameter + via.diameter) / 2.0 {
                return false;
            }
        }
        for pad in pads.iter().filter(|pad| copper(board, &pad.layer)) {
            if pad.key == from_key || pad.key == to_key {
                continue;
            }
            let distance = point_box_distance(*at, pad.at, pad.half);
            if distance + EPSILON < settings.clearance + settings.via_diameter / 2.0 {
                return false;
            }
        }
    }
    true
}

fn copper(board: &Board, id: &str) -> bool {
    board.layers.iter().any(|layer| layer.id == id && layer.kind == LayerKind::Copper)
}

fn inside(point: [f64; 2], min: [f64; 2], max: [f64; 2], margin: f64) -> bool {
    Rect::from_points(geo(min), geo(max)).expanded(-margin + EPSILON).contains(geo(point))
}

fn distance(left: [f64; 2], right: [f64; 2]) -> f64 {
    squared_distance(left, right).sqrt()
}

fn endpoints_match(a1: [f64; 2], a2: [f64; 2], b1: [f64; 2], b2: [f64; 2]) -> bool {
    near(a1, b1) || near(a1, b2) || near(a2, b1) || near(a2, b2)
}

fn near(left: [f64; 2], right: [f64; 2]) -> bool {
    distance(left, right) <= 1e-6
}

fn geo(point: [f64; 2]) -> GeoPoint {
    GeoPoint::new(point[0], point[1])
}

fn point_segment_distance(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> f64 {
    if squared_distance(start, end) <= EPSILON {
        return distance(point, start);
    }
    geo_point_segment_distance(geo(point), geo(start), geo(end))
}

fn segment_distance(a1: [f64; 2], a2: [f64; 2], b1: [f64; 2], b2: [f64; 2]) -> f64 {
    geo_segment_distance(geo(a1), geo(a2), geo(b1), geo(b2))
}

fn point_box_distance(point: [f64; 2], centre: [f64; 2], half: [f64; 2]) -> f64 {
    let dx = (point[0] - centre[0]).abs() - half[0];
    let dy = (point[1] - centre[1]).abs() - half[1];
    let dx = dx.max(0.0);
    let dy = dy.max(0.0);
    (dx * dx + dy * dy).sqrt()
}

fn segment_box_distance(start: [f64; 2], end: [f64; 2], centre: [f64; 2], half: [f64; 2]) -> f64 {
    let rect =
        Rect::from_points(geo([centre[0] - half[0], centre[1] - half[1]]), geo([centre[0] + half[0], centre[1] + half[1]]))
            .expanded(EPSILON);
    if rect.contains(geo(start)) || rect.contains(geo(end)) {
        return 0.0;
    }
    let corners = [[rect.min.x, rect.min.y], [rect.max.x, rect.min.y], [rect.max.x, rect.max.y], [rect.min.x, rect.max.y]];
    let mut best = f64::INFINITY;
    for index in 0..4 {
        let edge = (corners[index], corners[(index + 1) % 4]);
        best = best.min(segment_distance(start, end, edge.0, edge.1));
    }
    best
}

/// Proper crossing test; touching endpoints do not count.
fn segments_cross(a1: [f64; 2], a2: [f64; 2], b1: [f64; 2], b2: [f64; 2]) -> bool {
    if endpoints_match(a1, a2, b1, b2) {
        return false;
    }
    let (a1, a2, b1, b2) = (geo(a1), geo(a2), geo(b1), geo(b2));
    let o1 = (a2 - a1).cross(b1 - a1);
    let o2 = (a2 - a1).cross(b2 - a1);
    let o3 = (b2 - b1).cross(a1 - b1);
    let o4 = (b2 - b1).cross(a2 - b1);
    o1 * o2 < 0.0 && o3 * o4 < 0.0
}
