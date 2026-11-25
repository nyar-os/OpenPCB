//! Rule checks run against one open board.
//!
//! Only the checks this build implements appear as `supported` in the
//! capability matrix. Requesting an unimplemented check reports a warning
//! issue instead of a clean pass, so "not run" is never mistaken for "passed".

use std::collections::{BTreeMap, HashMap, HashSet};

use boost_check::{CheckReport, Issue, Severity};
use boost_collection::UnionFind;
use geometry_planar::{
    Point as GeoPoint, Rect, point_segment_distance as geo_point_segment_distance, segment_distance as geo_segment_distance,
};
use serde_json::{Map, Value, json};

use crate::{
    ServiceError,
    board::{Board, Footprint, LayerKind, Pad},
    export::pad_corners,
};

/// Minimum copper width in millimetres.
const MIN_TRACK_WIDTH_MM: f64 = 0.15;
/// Minimum copper-to-copper clearance in millimetres.
pub(crate) const MIN_CLEARANCE_MM: f64 = 0.2;
/// Courtyard expansion around a footprint's pad bounding box, in millimetres.
const COURTYARD_MARGIN_MM: f64 = 0.25;
/// Minimum annular ring on a via or a drilled pad, in millimetres.
const MIN_ANNULAR_RING_MM: f64 = 0.05;
/// Track width below which `track-width` reports an error, in millimetres.
const TRACK_WIDTH_ERROR_MM: f64 = 0.1;
/// Track width below which `track-width` reports a warning, in millimetres.
const TRACK_WIDTH_WARNING_MM: f64 = 0.15;
/// Endpoint matching tolerance for the `unrouted` graph, in millimetres.
const CONNECTIVITY_TOLERANCE_MM: f64 = 0.01;
/// Slack allowed when comparing differential-pair geometry, in millimetres.
const DIFFPAIR_TOLERANCE_MM: f64 = 0.01;
/// Minimum solder-mask web between two openings of different nets, in
/// millimetres.
const MIN_SOLDER_MASK_WEB_MM: f64 = 0.1;
/// Largest allowed routed-length difference inside a naming-inferred
/// differential pair, in millimetres.
const DIFFPAIR_SKEW_MM: f64 = 0.5;

/// Every declared check, in report order.
pub(crate) const CHECKS: &[&str] = &[
    "drc",
    "connectivity",
    "clearance",
    "annular_rings",
    "annular-ring",
    "track-width",
    "unrouted",
    "stackup",
    "outline",
    "silkscreen",
    "courtyard",
    "netlist",
    "hole-to-hole",
    "hole-to-track",
    "hole-to-pad",
    "copper-to-edge",
    "edge-clearance",
    "silkscreen-over-pad",
    "diffpair",
    "dangling-copper",
    "netclass",
    "keepout",
    "via-in-pad",
    "pad-overlap",
    "solder-mask",
    "testpoints",
    "stencil",
];

/// Checks this build can actually run.
pub(crate) const SUPPORTED_CHECKS: &[&str] = &[
    "drc",
    "connectivity",
    "clearance",
    "annular_rings",
    "annular-ring",
    "track-width",
    "unrouted",
    "stackup",
    "outline",
    "silkscreen",
    "courtyard",
    "netlist",
    "hole-to-hole",
    "hole-to-track",
    "hole-to-pad",
    "copper-to-edge",
    "edge-clearance",
    "silkscreen-over-pad",
    "diffpair",
    "dangling-copper",
    "netclass",
    "keepout",
    "via-in-pad",
    "pad-overlap",
    "solder-mask",
    "testpoints",
    "stencil",
];

/// Runs the requested checks and returns an `InspectReport`.
pub(crate) fn run(board: &Board, checks: Option<Vec<String>>) -> Result<Value, ServiceError> {
    let requested = match checks {
        Some(names) if !names.is_empty() => names,
        _ => SUPPORTED_CHECKS.iter().map(|name| name.to_string()).collect(),
    };
    for name in &requested {
        if !CHECKS.contains(&name.as_str()) {
            return Err(crate::argument(format!("unknown check `{name}`")));
        }
    }

    let mut report = CheckReport::new();
    for name in &requested {
        match name.as_str() {
            "drc" => drc(board, &mut report),
            "connectivity" => connectivity(board, &mut report),
            "clearance" => clearance(board, &mut report),
            "annular_rings" => annular_rings(board, &mut report),
            "annular-ring" => annular_ring(board, &mut report),
            "track-width" => track_width(board, &mut report),
            "unrouted" => unrouted(board, &mut report),
            "stackup" => stackup(board, &mut report),
            "outline" => outline(board, &mut report),
            "silkscreen" => silkscreen(board, &mut report),
            "courtyard" => courtyard(board, &mut report),
            "netlist" => netlist(board, &mut report),
            "hole-to-hole" => hole_to_hole(board, &mut report),
            "hole-to-track" => hole_to_track(board, &mut report),
            "hole-to-pad" => hole_to_pad(board, &mut report),
            "copper-to-edge" => copper_to_edge(board, &mut report),
            "edge-clearance" => edge_clearance(board, &mut report),
            "silkscreen-over-pad" => silkscreen_over_pad(board, &mut report),
            "diffpair" => diffpair(board, &mut report),
            "dangling-copper" => dangling_copper(board, &mut report),
            "netclass" => netclass(board, &mut report),
            "keepout" => keepout(board, &mut report),
            "via-in-pad" => via_in_pad(board, &mut report),
            "pad-overlap" => pad_overlap(board, &mut report),
            "solder-mask" => solder_mask(board, &mut report),
            "testpoints" => testpoints(board, &mut report),
            "stencil" => stencil(board, &mut report),
            other => report.push(finding(
                "check/unsupported",
                Severity::Warning,
                board,
                None,
                None,
                format!("`{other}` is declared but not implemented; no result is claimed"),
            )),
        }
    }

    // `stackup` and `outline` both report a missing outline; fold identical
    // findings so one run never lists the same fact twice.
    let mut seen = HashSet::new();
    report.issues.retain(|issue| {
        seen.insert(format!(
            "{}|{}|{}|{}",
            issue.code,
            issue.severity.as_str(),
            issue.details.get("layerId").map_or("", String::as_str),
            issue.details.get("objectId").map_or("", String::as_str)
        ))
    });

    // A document-level severity override replaces the check's own level, then a
    // matching waiver marks the finding. Overrides are applied first so a waiver
    // silences the level the user sees.
    for issue in &mut report.issues {
        if let Some(severity) = board.severity_overrides.get(&issue.code).and_then(|value| severity_of(value)) {
            issue.severity = severity;
        }
        let object = issue.details.get("objectId").map(String::as_str);
        let waiver = board
            .waivers
            .iter()
            .find(|waiver| waiver.code == issue.code && waiver.object.as_deref().is_none_or(|target| Some(target) == object));
        if let Some(waiver) = waiver {
            issue.details.insert("waived".to_string(), "true".to_string());
            issue.details.insert("waiverId".to_string(), waiver.id.clone());
            if let Some(reason) = &waiver.reason {
                issue.details.insert("waiverReason".to_string(), reason.clone());
            }
        }
    }

    let waived = report.issues.iter().filter(|issue| is_waived(issue)).count();
    let active = report.issues.len() - waived;
    let passed = !report.issues.iter().any(|issue| issue.severity.is_error() && !is_waived(issue));
    let issues = report.issues.iter().map(|issue| issue_value(issue, board.revision)).collect::<Vec<_>>();
    Ok(json!({
        "revision": board.revision,
        "checkerVersion": format!("open-pcb-model {}", env!("CARGO_PKG_VERSION")),
        "passed": passed,
        "active": active,
        "waived": waived,
        "issues": issues,
    }))
}

/// Parses a persisted severity name.
fn severity_of(value: &str) -> Option<Severity> {
    match value {
        "info" => Some(Severity::Info),
        "warning" => Some(Severity::Warning),
        "error" => Some(Severity::Error),
        _ => None,
    }
}

/// Whether one finding carries a waiver marker.
fn is_waived(issue: &Issue) -> bool {
    issue.details.get("waived").is_some_and(|value| value == "true")
}

fn finding(
    code: &str,
    severity: Severity,
    _board: &Board,
    layer: Option<&str>,
    object: Option<&str>,
    message: String,
) -> Issue {
    let mut issue = Issue::new(code, severity, message);
    if let Some(layer) = layer {
        issue = issue.with_detail("layerId", layer);
    }
    if let Some(object) = object {
        issue = issue.with_detail("objectId", object);
    }
    issue
}

/// Flattens one issue back into the historical JSON shape.
fn issue_value(issue: &Issue, revision: u64) -> Value {
    let mut value = Map::new();
    value.insert("code".to_string(), Value::String(issue.code.clone()));
    value.insert("severity".to_string(), Value::String(issue.severity.as_str().to_string()));
    value.insert("revision".to_string(), Value::from(revision));
    value.insert("recoverable".to_string(), Value::Bool(!issue.severity.is_error()));
    value.insert("message".to_string(), Value::String(issue.message.clone()));
    for (key, detail) in &issue.details {
        let rendered = if key == "waived" { Value::Bool(detail == "true") } else { Value::String(detail.clone()) };
        value.insert(key.clone(), rendered);
    }
    Value::Object(value)
}

fn drc(board: &Board, issues: &mut CheckReport) {
    let mut references: HashMap<&str, usize> = HashMap::new();
    for footprint in &board.footprints {
        *references.entry(footprint.ref_.as_str()).or_default() += 1;
    }
    for footprint in &board.footprints {
        if references.get(footprint.ref_.as_str()).copied().unwrap_or(0) > 1 {
            issues.push(finding(
                "drc/duplicate-reference",
                Severity::Error,
                board,
                Some(&footprint.layer),
                Some(&footprint.id),
                format!("reference `{}` is used by more than one footprint", footprint.ref_),
            ));
        }
        if footprint.pads.is_empty() {
            issues.push(finding(
                "drc/footprint-no-pads",
                Severity::Warning,
                board,
                Some(&footprint.layer),
                Some(&footprint.id),
                format!("footprint `{}` has no pads", footprint.ref_),
            ));
        }
        if board.layer(&footprint.layer).is_none() {
            issues.push(finding(
                "drc/footprint-layer-missing",
                Severity::Error,
                board,
                Some(&footprint.layer),
                Some(&footprint.id),
                format!("footprint `{}` is placed on missing layer `{}`", footprint.ref_, footprint.layer),
            ));
        }
    }

    for track in &board.tracks {
        match board.layer(&track.layer) {
            None => issues.push(finding(
                "drc/track-layer-missing",
                Severity::Error,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!("track `{}` references missing layer `{}`", track.id, track.layer),
            )),
            Some(layer) if layer.kind != LayerKind::Copper && layer.kind != LayerKind::Silkscreen => issues.push(finding(
                "drc/track-layer-not-copper",
                Severity::Error,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!("track `{}` sits on `{}`, which is not a copper layer", track.id, track.layer),
            )),
            Some(_) => {}
        }
        if track.width * board.millimetres() < MIN_TRACK_WIDTH_MM {
            issues.push(finding(
                "drc/track-width-too-small",
                Severity::Error,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!(
                    "track `{}` is {} mm wide; the minimum is {MIN_TRACK_WIDTH_MM} mm",
                    track.id,
                    track.width * board.millimetres()
                ),
            ));
        }
    }

    for via in &board.vias {
        if via.drill * board.millimetres() <= 0.0 {
            issues.push(finding(
                "drc/via-drill-invalid",
                Severity::Error,
                board,
                None,
                Some(&via.id),
                format!("via `{}` has a non-positive drill", via.id),
            ));
        }
        if let Some(back) = via.back_drill {
            if back.depth <= 0.0 || back.stub < 0.0 {
                issues.push(finding(
                    "drc/back-drill-invalid",
                    Severity::Error,
                    board,
                    None,
                    Some(&via.id),
                    format!("via `{}` has an invalid back drill (depth {}, stub {})", via.id, back.depth, back.stub),
                ));
            }
            if board.via_kind(via) != "through" {
                issues.push(finding(
                    "drc/back-drill-invalid",
                    Severity::Error,
                    board,
                    None,
                    Some(&via.id),
                    format!("via `{}` is not a through via; back drilling is only defined on a through via", via.id),
                ));
            }
        }
    }
}

fn connectivity(board: &Board, issues: &mut CheckReport) {
    let mut declared = HashSet::new();
    for net in &board.nets {
        declared.insert(net.name.as_str());
    }
    for track in &board.tracks {
        if let Some(net) = track.net.as_deref()
            && !declared.contains(net)
        {
            issues.push(finding(
                "drc/track-net-undeclared",
                Severity::Error,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!("track `{}` references undeclared net `{net}`", track.id),
            ));
        }
    }
    for via in &board.vias {
        if let Some(net) = via.net.as_deref()
            && !declared.contains(net)
        {
            issues.push(finding(
                "drc/via-net-undeclared",
                Severity::Error,
                board,
                None,
                Some(&via.id),
                format!("via `{}` references undeclared net `{net}`", via.id),
            ));
        }
    }
}

fn clearance(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    for (index, track) in board.tracks.iter().enumerate() {
        if board.layer(&track.layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
            continue;
        }
        for other in board.tracks.iter().skip(index + 1) {
            if other.layer != track.layer {
                continue;
            }
            if board.layer(&other.layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
                continue;
            }
            let centres = segment_distance(track.from, track.to, other.from, other.to);
            if track.net == other.net
                && (shares_endpoint(track, other) || centres <= 1e-6 || (track.teardrop && other.teardrop))
            {
                // Two segments of one net that meet or cross are connected; a
                // shared endpoint or a T-junction is not a clearance violation.
                // Teardrop chords of one net are generated fillet copper whose
                // strokes merge by design, so they are connected as well.
                continue;
            }
            let gap = (centres - (track.width + other.width) / 2.0) * scale;
            let required = required_clearance(board, track.net.as_deref(), other.net.as_deref());
            if gap < required {
                issues.push(finding(
                    "drc/clearance-violation",
                    Severity::Error,
                    board,
                    Some(&track.layer),
                    Some(&track.id),
                    if gap < 0.0 {
                        format!(
                            "tracks `{}` and `{}` overlap by {:.3} mm on {}; the minimum clearance is {required} mm",
                            track.id, other.id, -gap, track.layer
                        )
                    }
                    else {
                        format!(
                            "tracks `{}` and `{}` are {:.3} mm apart on {}; the minimum clearance is {required} mm",
                            track.id, other.id, gap, track.layer
                        )
                    },
                ));
            }
        }
    }
}

/// Flags every copper object that enters a declared keepout region.
///
/// Tracks, vias, copper pour regions and pads are tested against each keepout
/// polygon on a matching layer. A keepout on `*` covers every copper layer; one
/// on a named layer covers only that layer.
fn keepout(board: &Board, issues: &mut CheckReport) {
    for region in &board.keepouts {
        for track in &board.tracks {
            if board.layer(&track.layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
                continue;
            }
            if !board.keepout_covers(region, &track.layer) {
                continue;
            }
            if segment_in_polygon(track.from, track.to, &region.points) {
                issues.push(finding(
                    "drc/keepout-violation",
                    Severity::Error,
                    board,
                    Some(&track.layer),
                    Some(&track.id),
                    format!("track `{}` enters keepout `{}` on {}", track.id, region.id, track.layer),
                ));
            }
        }
        for via in &board.vias {
            if region.layer != "*" && !board.via_covers(via, &region.layer) {
                continue;
            }
            if point_in_polygon(via.at, &region.points) {
                let layer = (region.layer != "*").then_some(region.layer.as_str());
                issues.push(finding(
                    "drc/keepout-violation",
                    Severity::Error,
                    board,
                    layer,
                    Some(&via.id),
                    format!("via `{}` sits inside keepout `{}`", via.id, region.id),
                ));
            }
        }
        for zone in &board.zones {
            if !board.keepout_covers(region, &zone.layer) {
                continue;
            }
            if polygons_overlap(&zone.points, &region.points) {
                issues.push(finding(
                    "drc/keepout-violation",
                    Severity::Error,
                    board,
                    Some(&zone.layer),
                    Some(&zone.id),
                    format!("zone `{}` overlaps keepout `{}`", zone.id, region.id),
                ));
            }
        }
        for pad in board.placed_pads() {
            if !board.keepout_covers(region, pad.layer) {
                continue;
            }
            if point_in_polygon(pad.at, &region.points) {
                issues.push(finding(
                    "drc/keepout-violation",
                    Severity::Error,
                    board,
                    Some(pad.layer),
                    Some(&pad.key),
                    format!("pad `{}` sits inside keepout `{}`", pad.key, region.id),
                ));
            }
        }
    }
}

/// Checks vias whose centre lands inside a pad they share copper with.
///
/// A via-in-pad needs plugging or filling before assembly, so the finding is a
/// warning rather than an electrical error. The check reports the pad key and
/// the via id so a review can decide whether the via is intentional. Only vias
/// that reach the pad's layer are compared.
fn via_in_pad(board: &Board, issues: &mut CheckReport) {
    let pads = board.placed_pads();
    for via in &board.vias {
        for pad in &pads {
            if !board.via_covers(via, pad.layer) {
                continue;
            }
            if (via.at[0] - pad.at[0]).abs() > pad.half[0] || (via.at[1] - pad.at[1]).abs() > pad.half[1] {
                continue;
            }
            let mut issue = finding(
                "via-in-pad/via-in-pad",
                Severity::Warning,
                board,
                Some(pad.layer),
                Some(&via.id),
                format!("via `{}` sits inside pad `{}` and needs fill or plugging", via.id, pad.key),
            );
            issue = issue.with_detail("pad", pad.key.clone());
            issues.push(issue);
        }
    }
}

/// Checks that pads of different nets do not overlap on one layer.
///
/// Two pads on the same layer overlap when their rotated rectangles share
/// copper, not when they merely touch: a fine-pitch land pattern keeps adjacent
/// pads tangent and stays clean. Each pair is reported once, in footprint-id and
/// pad-number order, so the same board always lists the same findings. A pad
/// without a net and a pad with one count as different nets.
fn pad_overlap(board: &Board, issues: &mut CheckReport) {
    let mut pads: Vec<(&Footprint, &Pad)> =
        board.footprints.iter().flat_map(|footprint| footprint.pads.iter().map(move |pad| (footprint, pad))).collect();
    pads.sort_by(|(left, left_pad), (right, right_pad)| {
        left.id.cmp(&right.id).then_with(|| left_pad.number.cmp(&right_pad.number))
    });
    for (index, (footprint, pad)) in pads.iter().enumerate() {
        for (other_footprint, other_pad) in pads.iter().skip(index + 1) {
            if footprint.layer != other_footprint.layer || pad.net == other_pad.net {
                continue;
            }
            if !convex_overlap(&pad_corners(footprint, pad), &pad_corners(other_footprint, other_pad)) {
                continue;
            }
            let object = format!("{}:{}+{}:{}", footprint.id, pad.number, other_footprint.id, other_pad.number);
            issues.push(finding(
                "drc/pad-overlap",
                Severity::Error,
                board,
                Some(&footprint.layer),
                Some(&object),
                format!(
                    "pad `{}` of `{}` overlaps pad `{}` of `{}` on {}",
                    pad.number, footprint.ref_, other_pad.number, other_footprint.ref_, footprint.layer
                ),
            ));
        }
    }
}

/// Checks the solder-mask apertures of outer-layer pads.
///
/// The mask writer opens a rectangle over every outer-layer pad, grown by the
/// document's solder-mask rule per side; vias stay tented and inner copper
/// opens no mask. Two openings of different nets that overlap or touch leave
/// no mask between the pads, so solder can bridge them; an opening pair that
/// stays apart by less than [`MIN_SOLDER_MASK_WEB_MM`] leaves a web too thin to
/// survive fabrication. Pads of the same net are skipped, mirroring
/// `pad_overlap`. Each pair is reported once, in pad-key order, on the pad's
/// outer copper layer.
fn solder_mask(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    if scale <= 0.0 {
        return;
    }
    let stack = board.copper_stack();
    let Some(&front) = stack.first()
    else {
        return;
    };
    let back = *stack.last().unwrap_or(&front);
    let expansion = board.rules.solder_mask_expansion / scale;
    let mut pads: Vec<_> = board.placed_pads().into_iter().filter(|pad| pad.layer == front || pad.layer == back).collect();
    pads.sort_by(|left, right| left.key.cmp(&right.key));
    for (index, pad) in pads.iter().enumerate() {
        for other in pads.iter().skip(index + 1) {
            if pad.layer != other.layer || pad.net == other.net {
                continue;
            }
            let overlap_x = pad.half[0] + other.half[0] + 2.0 * expansion - (pad.at[0] - other.at[0]).abs();
            let overlap_y = pad.half[1] + other.half[1] + 2.0 * expansion - (pad.at[1] - other.at[1]).abs();
            let object = format!("{}+{}", pad.key, other.key);
            if overlap_x >= -1e-9 && overlap_y >= -1e-9 {
                issues.push(finding(
                    "drc/solder-mask-bridge",
                    Severity::Error,
                    board,
                    Some(pad.layer),
                    Some(&object),
                    format!(
                        "solder-mask openings of pad `{}` and pad `{}` overlap on {}; no mask separates the two nets",
                        pad.key, other.key, pad.layer
                    ),
                ));
                continue;
            }
            let gap = (overlap_x.min(0.0).powi(2) + overlap_y.min(0.0).powi(2)).sqrt() * scale;
            // The rule is a minimum: a web that only differs from it by rounding
            // is accepted, so a nominal 0.1 mm web never warns.
            if gap + 1e-9 < MIN_SOLDER_MASK_WEB_MM && gap > 0.0 {
                issues.push(finding(
                    "drc/solder-mask-sliver",
                    Severity::Warning,
                    board,
                    Some(pad.layer),
                    Some(&object),
                    format!(
                        "solder-mask web between pad `{}` and pad `{}` is {gap:.3} mm on {}; the minimum web is {MIN_SOLDER_MASK_WEB_MM:.3} mm",
                        pad.key, other.key, pad.layer
                    ),
                ));
            }
        }
    }
}

/// Checks the stencil apertures implied by the solder-paste windows of
/// outer-layer pads.
///
/// The paste writer opens one rectangular window over every outer-layer pad,
/// sized exactly to the copper pad; vias stay tented and inner copper opens no
/// paste. Each window is measured in millimetres against the document's stencil
/// foil thickness. The area ratio is the opening area divided by the opening
/// perimeter times the thickness; the aspect ratio is the opening's short side
/// divided by the thickness. A ratio below its configured minimum is reported
/// as a warning, because a small opening fills and releases paste unreliably.
/// Findings are emitted in pad-key order, and a ratio exactly at the minimum is
/// accepted.
fn stencil(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    let thickness = board.rules.stencil_thickness;
    if scale <= 0.0 || thickness <= 0.0 {
        return;
    }
    let min_area = board.rules.stencil_area_ratio;
    let min_aspect = board.rules.stencil_aspect_ratio;
    let stack = board.copper_stack();
    let Some(front) = stack.first().copied()
    else {
        return;
    };
    let Some(back) = stack.last().copied()
    else {
        return;
    };

    let mut windows: Vec<(String, &str, f64, f64)> = Vec::new();
    for footprint in &board.footprints {
        if footprint.layer != front && footprint.layer != back {
            continue;
        }
        for pad in &footprint.pads {
            if pad.w <= 0.0 || pad.h <= 0.0 {
                continue;
            }
            windows.push((format!("{}:{}", footprint.id, pad.number), footprint.layer.as_str(), pad.w * scale, pad.h * scale));
        }
    }
    windows.sort_by(|left, right| left.0.cmp(&right.0));

    for (key, layer, width, height) in windows {
        let area_ratio = (width * height) / (2.0 * (width + height) * thickness);
        let aspect_ratio = width.min(height) / thickness;
        if area_ratio + 1e-9 < min_area {
            let mut issue = finding(
                "drc/stencil-area-ratio",
                Severity::Warning,
                board,
                Some(layer),
                Some(&key),
                format!(
                    "stencil opening of pad `{key}` is {width:.3} x {height:.3} mm with an area ratio of {area_ratio:.3}; the minimum is {min_area:.3}"
                ),
            );
            issue = issue
                .with_detail("openingWidth", format!("{width:.3}"))
                .with_detail("openingHeight", format!("{height:.3}"))
                .with_detail("areaRatio", format!("{area_ratio:.3}"))
                .with_detail("stencilThickness", format!("{thickness:.3}"));
            issues.push(issue);
        }
        if aspect_ratio + 1e-9 < min_aspect {
            let mut issue = finding(
                "drc/stencil-aspect-ratio",
                Severity::Warning,
                board,
                Some(layer),
                Some(&key),
                format!(
                    "stencil opening of pad `{key}` is {width:.3} x {height:.3} mm with an aspect ratio of {aspect_ratio:.3}; the minimum is {min_aspect:.3}"
                ),
            );
            issue = issue
                .with_detail("openingWidth", format!("{width:.3}"))
                .with_detail("openingHeight", format!("{height:.3}"))
                .with_detail("aspectRatio", format!("{aspect_ratio:.3}"))
                .with_detail("stencilThickness", format!("{thickness:.3}"));
            issues.push(issue);
        }
    }
}

/// Whether two convex polygons share a positive area, using the separating-axis test.
///
/// A projection that only meets at an endpoint means the shapes touch without
/// overlapping, which is not a pad overlap.
fn convex_overlap(left: &[[f64; 2]], right: &[[f64; 2]]) -> bool {
    for polygon in [left, right] {
        for (start, end) in polygon_edges(polygon) {
            let axis = [start[1] - end[1], end[0] - start[0]];
            let (left_min, left_max) = projection(left, axis);
            let (right_min, right_max) = projection(right, axis);
            if left_max.min(right_max) - left_min.max(right_min) <= 1e-9 {
                return false;
            }
        }
    }
    true
}

/// Signed extent of one polygon along a separating axis.
fn projection(polygon: &[[f64; 2]], axis: [f64; 2]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for point in polygon {
        let value = point[0] * axis[0] + point[1] * axis[1];
        min = min.min(value);
        max = max.max(value);
    }
    (min, max)
}

/// Checks that every declared net offers a declared probe point.
///
/// A net is covered only by an explicit test point: the document stores test
/// points as pad or via targets, so no probe is inferred from copper geometry.
/// A net with neither a pad nor a via has no physical contact point at all and
/// is reported as an error; a connectable net with no declared test point is a
/// warning that names the net.
fn testpoints(board: &Board, issues: &mut CheckReport) {
    let mut covered = HashSet::new();
    for point in &board.test_points {
        if let Some(net) = board.resolve_test_point(&point.target).and_then(|resolved| resolved.net) {
            covered.insert(net);
        }
    }

    let mut pads: HashMap<&str, usize> = HashMap::new();
    let placed = board.placed_pads();
    for pad in &placed {
        if let Some(net) = pad.net.as_deref() {
            *pads.entry(net).or_default() += 1;
        }
    }
    let mut vias: HashMap<&str, usize> = HashMap::new();
    for via in &board.vias {
        if let Some(net) = via.net.as_deref() {
            *vias.entry(net).or_default() += 1;
        }
    }

    for net in &board.nets {
        let pad_count = pads.get(net.name.as_str()).copied().unwrap_or(0);
        let via_count = vias.get(net.name.as_str()).copied().unwrap_or(0);
        if pad_count == 0 && via_count == 0 {
            issues.push(finding(
                "testpoints/net-unconnected",
                Severity::Error,
                board,
                None,
                Some(&net.id),
                format!("net `{}` has no pad or via, so it offers no physical test point", net.name),
            ));
            continue;
        }
        if covered.contains(&net.name) {
            continue;
        }
        issues.push(finding(
            "testpoints/net-not-covered",
            Severity::Warning,
            board,
            None,
            Some(&net.id),
            format!("net `{}` has pads and vias but no declared test point", net.name),
        ));
    }
}

/// Whether a point falls inside a closed polygon, using the even-odd rule.
fn point_in_polygon(point: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = polygon.len() - 1;
    for current in 0..polygon.len() {
        let (a, b) = (polygon[current], polygon[previous]);
        if (a[1] > point[1]) != (b[1] > point[1]) && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0] {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

/// Whether a segment enters a polygon, including touching an edge.
fn segment_in_polygon(from: [f64; 2], to: [f64; 2], polygon: &[[f64; 2]]) -> bool {
    if point_in_polygon(from, polygon) || point_in_polygon(to, polygon) {
        return true;
    }
    polygon_edges(polygon).any(|(start, end)| segment_distance(from, to, start, end) <= 1e-9)
}

/// Whether two closed polygons share any area or touch.
fn polygons_overlap(left: &[[f64; 2]], right: &[[f64; 2]]) -> bool {
    left.iter().any(|point| point_in_polygon(*point, right))
        || right.iter().any(|point| point_in_polygon(*point, left))
        || polygon_edges(left).any(|(a, b)| polygon_edges(right).any(|(c, d)| segment_distance(a, b, c, d) <= 1e-9))
}

/// Iterates the ordered edges of a closed polygon.
fn polygon_edges(polygon: &[[f64; 2]]) -> impl Iterator<Item = ([f64; 2], [f64; 2])> + '_ {
    polygon.iter().enumerate().map(move |(index, point)| (*point, polygon[(index + 1) % polygon.len()]))
}

/// Clearance two nets need, taking the strictest class value.
fn required_clearance(board: &Board, left: Option<&str>, right: Option<&str>) -> f64 {
    let mut required = MIN_CLEARANCE_MM;
    for net in [left, right].into_iter().flatten() {
        if let Some(clearance) = board.class_of_net(net).and_then(|class| class.clearance) {
            required = required.max(clearance);
        }
    }
    required
}

fn annular_rings(board: &Board, issues: &mut CheckReport) {
    for via in &board.vias {
        if via.drill <= 0.0 || !via.diameter.is_finite() || !via.drill.is_finite() {
            continue;
        }
        if via.diameter <= via.drill {
            issues.push(finding(
                "drc/via-annular-ring-invalid",
                Severity::Error,
                board,
                None,
                Some(&via.id),
                format!("via `{}` has diameter {} and drill {}; no annular ring is left", via.id, via.diameter, via.drill),
            ));
        }
    }
}

/// Reports vias and drilled pads whose copper ring is thinner than
/// [`MIN_ANNULAR_RING_MM`].
fn annular_ring(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    for via in &board.vias {
        if !via.diameter.is_finite() || !via.drill.is_finite() {
            continue;
        }
        let ring = (via.diameter - via.drill) / 2.0 * scale;
        if ring < MIN_ANNULAR_RING_MM {
            issues.push(finding(
                "drc/annular-ring-too-small",
                Severity::Error,
                board,
                None,
                Some(&via.id),
                format!("via `{}` has an annular ring of {ring:.3} mm; the minimum is {MIN_ANNULAR_RING_MM} mm", via.id),
            ));
        }
    }
    for footprint in &board.footprints {
        for pad in &footprint.pads {
            let Some(drill) = pad.drill
            else {
                continue;
            };
            if !drill.is_finite() || !pad.w.is_finite() || !pad.h.is_finite() {
                continue;
            }
            let ring = (pad.w.min(pad.h) - drill) / 2.0 * scale;
            if ring < MIN_ANNULAR_RING_MM {
                issues.push(finding(
                    "drc/annular-ring-too-small",
                    Severity::Error,
                    board,
                    Some(&footprint.layer),
                    Some(&footprint.id),
                    format!(
                        "pad `{}` of `{}` has an annular ring of {ring:.3} mm; the minimum is {MIN_ANNULAR_RING_MM} mm",
                        pad.number, footprint.ref_
                    ),
                ));
            }
        }
    }
}

/// Reports copper tracks narrower than the warning and error thresholds.
///
/// The document model stores no board minimum width, so the fixed thresholds
/// above are the only source of truth.
fn track_width(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    for track in &board.tracks {
        if board.layer(&track.layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
            continue;
        }
        let width = track.width * scale;
        if width < TRACK_WIDTH_ERROR_MM {
            issues.push(finding(
                "drc/track-width-too-small",
                Severity::Error,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!("track `{}` is {width:.3} mm wide; the minimum is {TRACK_WIDTH_ERROR_MM} mm", track.id),
            ));
        }
        else if width < TRACK_WIDTH_WARNING_MM {
            issues.push(finding(
                "drc/track-width-narrow",
                Severity::Warning,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!(
                    "track `{}` is {width:.3} mm wide; narrower than the {TRACK_WIDTH_WARNING_MM} mm recommendation",
                    track.id
                ),
            ));
        }
    }
}

/// Enforces the width and via targets declared by net classes.
///
/// Only tracks and vias whose net belongs to a class are checked; a class with
/// no targets sets nothing. Damage in the document itself, such as an
/// undeclared net, is reported as an error too.
fn netclass(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    for class in &board.net_classes {
        for net in &class.nets {
            if board.net(net).is_none() {
                issues.push(finding(
                    "netclass/undeclared-net",
                    Severity::Error,
                    board,
                    None,
                    None,
                    format!("net class `{}` references undeclared net `{net}`", class.name),
                ));
            }
        }
    }
    for track in &board.tracks {
        if board.layer(&track.layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
            continue;
        }
        let Some(net) = track.net.as_deref()
        else {
            continue;
        };
        let Some(class) = board.class_of_net(net)
        else {
            continue;
        };
        if let Some(width) = class.width {
            let actual = track.width * scale;
            if actual + 1e-9 < width {
                issues.push(finding(
                    "netclass/track-too-narrow",
                    Severity::Error,
                    board,
                    Some(&track.layer),
                    Some(&track.id),
                    format!(
                        "track `{}` on net `{net}` is {actual:.3} mm wide; net class `{}` requires {width} mm",
                        track.id, class.name
                    ),
                ));
            }
        }
    }
    for via in &board.vias {
        let Some(net) = via.net.as_deref()
        else {
            continue;
        };
        let Some(class) = board.class_of_net(net)
        else {
            continue;
        };
        let diameter = via.diameter * scale;
        if let Some(target) = class.via_diameter
            && diameter + 1e-9 < target
        {
            issues.push(finding(
                "netclass/via-diameter-too-small",
                Severity::Error,
                board,
                None,
                Some(&via.id),
                format!(
                    "via `{}` on net `{net}` is {diameter:.3} mm wide; net class `{}` requires {target} mm",
                    via.id, class.name
                ),
            ));
        }
        let drill = via.drill * scale;
        if let Some(target) = class.via_drill
            && drill + 1e-9 < target
        {
            issues.push(finding(
                "netclass/via-drill-too-small",
                Severity::Error,
                board,
                None,
                Some(&via.id),
                format!(
                    "via `{}` on net `{net}` drills {drill:.3} mm; net class `{}` requires {target} mm",
                    via.id, class.name
                ),
            ));
        }
    }
}

/// Reports nets whose pads are not joined by tracks and vias.
///
/// The graph connects a track to a pad when an endpoint touches the pad centre
/// on the pad layer, to a via at a shared endpoint when the via reaches the
/// track layer, and to another track of the same net that shares a same-layer
/// endpoint. A via joins a pad only when it reaches the pad layer.
fn unrouted(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    if scale <= 0.0 {
        return;
    }
    let tolerance = CONNECTIVITY_TOLERANCE_MM / scale;
    let placed = board.placed_pads();
    for net in &board.nets {
        let pads = placed.iter().filter(|pad| pad.net.as_deref() == Some(net.name.as_str())).collect::<Vec<_>>();
        if pads.len() < 2 {
            continue;
        }
        let tracks = board.tracks.iter().filter(|track| track.net.as_deref() == Some(net.name.as_str())).collect::<Vec<_>>();
        let vias = board.vias.iter().filter(|via| via.net.as_deref() == Some(net.name.as_str())).collect::<Vec<_>>();
        let track_base = pads.len();
        let via_base = track_base + tracks.len();
        let mut groups = UnionFind::new(via_base + vias.len());

        for (index, track) in tracks.iter().enumerate() {
            let node = track_base + index;
            for (pad_index, pad) in pads.iter().enumerate() {
                if pad.layer == track.layer.as_str()
                    && (near(track.from, pad.at, tolerance) || near(track.to, pad.at, tolerance))
                {
                    groups.union(node, pad_index);
                }
            }
            for (via_index, via) in vias.iter().enumerate() {
                if board.via_covers(via, &track.layer)
                    && (near(track.from, via.at, tolerance) || near(track.to, via.at, tolerance))
                {
                    groups.union(node, via_base + via_index);
                }
            }
            for (other_index, other) in tracks.iter().enumerate().skip(index + 1) {
                if other.layer != track.layer {
                    continue;
                }
                let joined = near(track.from, other.from, tolerance)
                    || near(track.from, other.to, tolerance)
                    || near(track.to, other.from, tolerance)
                    || near(track.to, other.to, tolerance);
                if joined {
                    groups.union(node, track_base + other_index);
                }
            }
        }
        for (via_index, via) in vias.iter().enumerate() {
            for (pad_index, pad) in pads.iter().enumerate() {
                if board.via_covers(via, pad.layer) && near(via.at, pad.at, tolerance) {
                    groups.union(via_base + via_index, pad_index);
                }
            }
        }

        let mut roots = std::collections::BTreeSet::new();
        for pad_index in 0..pads.len() {
            roots.insert(groups.find(pad_index));
        }
        if roots.len() > 1 {
            issues.push(finding(
                "drc/unrouted",
                Severity::Warning,
                board,
                None,
                Some(&net.id),
                format!("net `{}` has {} disconnected pad groups", net.name, roots.len()),
            ));
        }
    }
}

fn near(left: [f64; 2], right: [f64; 2], tolerance: f64) -> bool {
    let dx = left[0] - right[0];
    let dy = left[1] - right[1];
    dx * dx + dy * dy <= tolerance * tolerance
}

fn stackup(board: &Board, issues: &mut CheckReport) {
    for required in ["F.Cu", "B.Cu"] {
        if board.layer(required).is_none() {
            issues.push(finding(
                "drc/stackup-missing-layer",
                Severity::Error,
                board,
                Some(required),
                None,
                format!("the stackup is missing required copper layer `{required}`"),
            ));
        }
    }
    if board.outline.is_none() {
        issues.push(missing_outline(board));
    }
}

fn missing_outline(board: &Board) -> Issue {
    finding(
        "drc/outline-missing",
        Severity::Error,
        board,
        None,
        None,
        "the board has no outline; set a rectangular outline before fabrication".to_string(),
    )
}

fn outline(board: &Board, issues: &mut CheckReport) {
    let Some(outline) = board.outline
    else {
        issues.push(missing_outline(board));
        return;
    };
    if outline.to[0] <= outline.from[0] || outline.to[1] <= outline.from[1] {
        issues.push(finding(
            "drc/outline-invalid",
            Severity::Error,
            board,
            None,
            None,
            "the outline must have a positive width and height".to_string(),
        ));
        return;
    }
    let board_rect = Rect::from_points(geo(outline.from), geo(outline.to));

    for footprint in &board.footprints {
        let angle = footprint.rotation.to_radians();
        let (sin, cos) = (angle.sin(), angle.cos());
        let mut left = footprint.x;
        let mut right = footprint.x;
        let mut bottom = footprint.y;
        let mut top = footprint.y;
        for pad in &footprint.pads {
            let center_x = footprint.x + pad.x * cos - pad.y * sin;
            let center_y = footprint.y + pad.x * sin + pad.y * cos;
            let half_w = (cos.abs() * pad.w + sin.abs() * pad.h) / 2.0;
            let half_h = (sin.abs() * pad.w + cos.abs() * pad.h) / 2.0;
            left = left.min(center_x - half_w);
            right = right.max(center_x + half_w);
            bottom = bottom.min(center_y - half_h);
            top = top.max(center_y + half_h);
        }
        if !board_rect.contains(geo([left, bottom])) || !board_rect.contains(geo([right, top])) {
            issues.push(finding(
                "drc/footprint-outside-outline",
                Severity::Warning,
                board,
                Some(&footprint.layer),
                Some(&footprint.id),
                format!("footprint `{}` lies outside the board outline", footprint.ref_),
            ));
        }
    }

    for track in &board.tracks {
        let track_rect = Rect::from_points(geo(track.from), geo(track.to));
        if track_rect.max.x < board_rect.min.x
            || track_rect.min.x > board_rect.max.x
            || track_rect.max.y < board_rect.min.y
            || track_rect.min.y > board_rect.max.y
        {
            issues.push(finding(
                "drc/track-outside-outline",
                Severity::Warning,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!("track `{}` lies outside the board outline", track.id),
            ));
        }
    }

    for via in &board.vias {
        if !board_rect.contains(geo(via.at)) {
            issues.push(finding(
                "drc/via-outside-outline",
                Severity::Warning,
                board,
                None,
                Some(&via.id),
                format!("via `{}` lies outside the board outline", via.id),
            ));
        }
    }
}

/// Reports whether every footprint reference can be represented on silkscreen
/// and whether every pad stays inside the board outline.
///
/// A footprint reference is satisfied by a silkscreen text item whose content
/// matches the designator; without one the warning names the missing legend.
/// Pad placement is still checked against real coordinates.
fn silkscreen(board: &Board, issues: &mut CheckReport) {
    for footprint in &board.footprints {
        let layer = silkscreen_layer(board, &footprint.layer);
        if let Some(layer) = layer
            && board.texts.iter().any(|item| item.text == footprint.ref_ && item.layer == layer)
        {
            continue;
        }
        issues.push(finding(
            "silkscreen/no-reference-text",
            Severity::Warning,
            board,
            layer,
            Some(&footprint.id),
            format!(
                "footprint `{}` has no silkscreen reference text; add a `{}` legend or keep the reference off the legend",
                footprint.ref_, footprint.ref_
            ),
        ));
    }

    let Some(outline) = board.outline
    else {
        issues.push(finding(
            "silkscreen/outline-missing",
            Severity::Warning,
            board,
            None,
            None,
            "the board has no outline; pad placement was not checked against it".to_string(),
        ));
        return;
    };
    let board_rect = Rect::from_points(geo(outline.from), geo(outline.to));

    for footprint in &board.footprints {
        let angle = footprint.rotation.to_radians();
        let (sin, cos) = (angle.sin(), angle.cos());
        for pad in &footprint.pads {
            let center_x = footprint.x + pad.x * cos - pad.y * sin;
            let center_y = footprint.y + pad.x * sin + pad.y * cos;
            let half_w = (cos.abs() * pad.w + sin.abs() * pad.h) / 2.0;
            let half_h = (sin.abs() * pad.w + cos.abs() * pad.h) / 2.0;
            if !board_rect.contains(geo([center_x - half_w, center_y - half_h]))
                || !board_rect.contains(geo([center_x + half_w, center_y + half_h]))
            {
                issues.push(finding(
                    "silkscreen/pad-outside-outline",
                    Severity::Error,
                    board,
                    Some(&footprint.layer),
                    Some(&footprint.id),
                    format!("pad `{}` of `{}` is not fully inside the board outline", pad.number, footprint.ref_),
                ));
                break;
            }
        }
    }
}

/// Compares one courtyard per footprint, expanded from its pad bounding box.
///
/// Two footprints whose courtyards intersect are an error. A courtyard that
/// leaves the board outline is a warning. The outline is required for both
/// findings; without it the check reports that geometry was skipped.
fn courtyard(board: &Board, issues: &mut CheckReport) {
    let Some(outline) = board.outline
    else {
        issues.push(finding(
            "courtyard/outline-missing",
            Severity::Warning,
            board,
            None,
            None,
            "the board has no outline; courtyard geometry was not checked".to_string(),
        ));
        return;
    };

    let margin = COURTYARD_MARGIN_MM / board.millimetres();
    let board_rect = Rect::from_points(geo(outline.from), geo(outline.to));
    let mut areas: Vec<(&Footprint, Rect)> = Vec::new();
    for footprint in &board.footprints {
        let Some(bounds) = pad_bounds(footprint)
        else {
            continue;
        };
        areas.push((
            footprint,
            Rect::from_points(geo([bounds[0] - margin, bounds[1] - margin]), geo([bounds[2] + margin, bounds[3] + margin])),
        ));
    }

    for (index, (footprint, area)) in areas.iter().enumerate() {
        if !board_rect.contains(area.min) || !board_rect.contains(area.max) {
            issues.push(finding(
                "courtyard/pad-outside-board",
                Severity::Warning,
                board,
                Some(&footprint.layer),
                Some(&footprint.id),
                format!("the courtyard of `{}` extends past the board outline", footprint.ref_),
            ));
        }
        for (other, second) in areas.iter().skip(index + 1) {
            if area.min.x < second.max.x && second.min.x < area.max.x && area.min.y < second.max.y && second.min.y < area.max.y
            {
                issues.push(finding(
                    "courtyard/overlap",
                    Severity::Error,
                    board,
                    Some(&footprint.layer),
                    Some(&footprint.id),
                    format!("the courtyard of `{}` overlaps the courtyard of `{}`", footprint.ref_, other.ref_),
                ));
            }
        }
    }
}

/// Returns the silkscreen layer a footprint is drawn on, preferring its side.
fn silkscreen_layer<'a>(board: &'a Board, footprint_layer: &str) -> Option<&'a str> {
    let preferred = if footprint_layer.starts_with("B.") { "B.SilkS" } else { "F.SilkS" };
    board
        .layer(preferred)
        .or_else(|| board.layers.iter().find(|layer| layer.kind == LayerKind::Silkscreen))
        .map(|layer| layer.id.as_str())
}

/// Returns the rotated pad bounding box of one footprint in board units.
fn pad_bounds(footprint: &Footprint) -> Option<[f64; 4]> {
    if footprint.pads.is_empty() {
        return None;
    }
    let angle = footprint.rotation.to_radians();
    let (sin, cos) = (angle.sin(), angle.cos());
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::INFINITY;
    let mut top = f64::NEG_INFINITY;
    for pad in &footprint.pads {
        let center_x = footprint.x + pad.x * cos - pad.y * sin;
        let center_y = footprint.y + pad.x * sin + pad.y * cos;
        let half_w = (cos.abs() * pad.w + sin.abs() * pad.h) / 2.0;
        let half_h = (sin.abs() * pad.w + cos.abs() * pad.h) / 2.0;
        left = left.min(center_x - half_w);
        right = right.max(center_x + half_w);
        bottom = bottom.min(center_y - half_h);
        top = top.max(center_y + half_h);
    }
    Some([left, bottom, right, top])
}

fn netlist(board: &Board, issues: &mut CheckReport) {
    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for net in &board.nets {
        if !ids.insert(net.id.as_str()) {
            issues.push(finding(
                "drc/duplicate-net-id",
                Severity::Error,
                board,
                None,
                Some(&net.id),
                format!("net id `{}` is declared more than once", net.id),
            ));
        }
        if !names.insert(net.name.as_str()) {
            issues.push(finding(
                "drc/duplicate-net-name",
                Severity::Error,
                board,
                None,
                Some(&net.id),
                format!("net name `{}` is declared more than once", net.name),
            ));
        }
    }
    for footprint in &board.footprints {
        for pad in &footprint.pads {
            if let Some(net) = pad.net.as_deref()
                && !names.contains(net)
            {
                issues.push(finding(
                    "drc/pad-net-undeclared",
                    Severity::Error,
                    board,
                    Some(&footprint.layer),
                    Some(&footprint.id),
                    format!("pad `{}` of `{}` references undeclared net `{net}`", pad.number, footprint.ref_),
                ));
            }
        }
    }
}

/// One physical drill hole: a via or a drilled pad, in document order.
struct HoleSite {
    /// Via id, or `{footprint.id}.{pad.number}` for a drilled pad.
    id: String,
    /// Declared net name, when assigned.
    net: Option<String>,
    /// Hole centre in board coordinates.
    at: [f64; 2],
    /// Finished drill diameter in board units.
    drill: f64,
    /// Pad key `footprint-id:pad-number` of the owning pad, `None` for a via.
    pad: Option<String>,
}

/// Collects every physical drill hole in document order.
///
/// Every via comes first, then every pad with a finite positive drill. A pad
/// without a drill is not a hole. A pad hole id is
/// `{footprint.id}.{pad.number}`.
fn hole_sites(board: &Board) -> Vec<HoleSite> {
    let mut holes: Vec<HoleSite> = board
        .vias
        .iter()
        .map(|via| HoleSite { id: via.id.clone(), net: via.net.clone(), at: via.at, drill: via.drill, pad: None })
        .collect();
    let placed = board.placed_pads();
    for footprint in &board.footprints {
        for pad in &footprint.pads {
            let Some(drill) = pad.drill.filter(|drill| drill.is_finite() && *drill > 0.0)
            else {
                continue;
            };
            let key = format!("{}:{}", footprint.id, pad.number);
            let Some(site) = placed.iter().find(|site| site.key == key)
            else {
                continue;
            };
            holes.push(HoleSite {
                id: format!("{}.{}", footprint.id, pad.number),
                net: pad.net.clone(),
                at: site.at,
                drill,
                pad: Some(key),
            });
        }
    }
    holes
}

/// Checks the gap between drill holes of different nets.
fn hole_to_hole(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    let rule = board.rules.hole_clearance;
    let holes = hole_sites(board);

    for (index, first) in holes.iter().enumerate() {
        for second in holes.iter().skip(index + 1) {
            if first.net == second.net {
                continue;
            }
            let centres = ((second.at[0] - first.at[0]).powi(2) + (second.at[1] - first.at[1]).powi(2)).sqrt() * scale;
            let gap = centres - (first.drill + second.drill) / 2.0 * scale;
            let gap_text = (gap * 1000.0).round() / 1000.0;
            if gap < rule {
                let object = format!("{}+{}", first.id, second.id);
                issues.push(finding(
                    "drc/hole-to-hole",
                    Severity::Error,
                    board,
                    None,
                    Some(&object),
                    format!(
                        "drill holes `{}` and `{}` are {gap_text} mm apart; the rule requires {rule} mm",
                        first.id, second.id
                    ),
                ));
            }
        }
    }
}

/// Checks copper clearance between every drill hole and every track.
///
/// The hole set matches `hole-to-hole`: every via, then every pad with a finite
/// positive drill. Each hole is measured against the nearest point of each
/// track segment, not its infinite line. A hole and a track on the same net are
/// not compared, including two unassigned objects. Every track is tested against
/// every hole regardless of layer, so each drill hole is treated as a
/// through-hole feature. Blind and buried via spans are not used to limit which
/// tracks participate, and the check makes no per-layer filtering.
fn hole_to_track(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    for hole in hole_sites(board) {
        for track in &board.tracks {
            if hole.net == track.net {
                continue;
            }
            let centres = point_segment_distance(hole.at, track.from, track.to) * scale;
            let gap = centres - (hole.drill + track.width) / 2.0 * scale;
            let rule = required_clearance(board, hole.net.as_deref(), track.net.as_deref());
            if gap < rule {
                let object = format!("{}+{}", hole.id, track.id);
                issues.push(finding(
                    "drc/hole-to-track",
                    Severity::Error,
                    board,
                    Some(&track.layer),
                    Some(&object),
                    format!(
                        "drill hole `{}` is {gap:.3} mm from track `{}`; the clearance rule requires {rule} mm",
                        hole.id, track.id
                    ),
                ));
            }
        }
    }
}

/// Checks copper clearance between every drill hole and every other pad.
///
/// The hole set matches `hole-to-hole`: every via, then every pad with a finite
/// positive drill. A hole is measured to the nearest point of every pad box,
/// not to its own drill: the pad that owns a pad hole is skipped by identity,
/// and a hole and a pad on the same net are not compared, including two
/// unassigned objects. Every pad is tested regardless of the layer the hole or
/// the pad sits on, so each drill hole is treated as a through-hole feature and
/// the check makes no per-layer filtering.
fn hole_to_pad(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    let holes = hole_sites(board);
    let pads = board.placed_pads();
    for hole in &holes {
        for pad in &pads {
            if hole.pad.as_deref() == Some(pad.key.as_str()) || hole.net == pad.net {
                continue;
            }
            let centres = point_box_distance(hole.at, pad.at, pad.half) * scale;
            let gap = centres - hole.drill / 2.0 * scale;
            let rule = required_clearance(board, hole.net.as_deref(), pad.net.as_deref());
            if gap < rule {
                let object = format!("{}+{}", hole.id, pad.key);
                issues.push(finding(
                    "drc/hole-to-pad",
                    Severity::Error,
                    board,
                    Some(pad.layer),
                    Some(&object),
                    format!(
                        "drill hole `{}` is {gap:.3} mm from pad `{}`; the clearance rule requires {rule} mm",
                        hole.id, pad.key
                    ),
                ));
            }
        }
    }
}

/// Checks copper-to-board-edge spacing for tracks, pads and vias.
///
/// A board without a usable rectangular outline reports nothing here; the
/// `outline` and `stackup` checks already report the missing outline, and this
/// check does not invent geometry to measure against.
fn copper_to_edge(board: &Board, issues: &mut CheckReport) {
    let Some(outline) = board.outline
    else {
        return;
    };
    if outline.to[0] <= outline.from[0] || outline.to[1] <= outline.from[1] {
        return;
    }
    let scale = board.millimetres();
    let rule = board.rules.copper_to_edge;
    let border = Rect::from_points(geo(outline.from), geo(outline.to));

    for track in &board.tracks {
        if !board.layer(&track.layer).is_some_and(|layer| layer.kind == LayerKind::Copper) {
            continue;
        }
        let gap = segment_to_rect_border(track.from, track.to, border) * scale - track.width * scale / 2.0;
        if gap < rule {
            issues.push(finding(
                "drc/copper-to-edge",
                Severity::Error,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!("track `{}` is {gap} mm from the board edge; the rule requires {rule} mm", track.id),
            ));
        }
    }

    for pad in board.placed_pads() {
        if !board.layer(pad.layer).is_some_and(|layer| layer.kind == LayerKind::Copper) {
            continue;
        }
        let area = Rect::from_points(
            geo([pad.at[0] - pad.half[0], pad.at[1] - pad.half[1]]),
            geo([pad.at[0] + pad.half[0], pad.at[1] + pad.half[1]]),
        );
        let gap = rect_border_distance(area, border) * scale;
        if gap < rule {
            issues.push(finding(
                "drc/copper-to-edge",
                Severity::Error,
                board,
                Some(pad.layer),
                Some(&pad.key),
                format!("pad `{}` is {gap} mm from the board edge; the rule requires {rule} mm", pad.key),
            ));
        }
    }

    for via in &board.vias {
        let gap = point_to_rect_border(via.at, border) * scale - via.diameter * scale / 2.0;
        if gap < rule {
            issues.push(finding(
                "drc/copper-to-edge",
                Severity::Error,
                board,
                None,
                Some(&via.id),
                format!("via `{}` is {gap} mm from the board edge; the rule requires {rule} mm", via.id),
            ));
        }
    }
}

/// Checks component bodies and drill holes against the board edge.
///
/// The model stores a rectangular outline, so closure and self-intersection are
/// guaranteed by construction; this check adds the two spacing rules the
/// rectangular model can measure. A component is represented by its rotated pad
/// bounding box (or its placement point when it has no pads). A drill hole is
/// represented by the finished drill of a via. Boards without a usable outline
/// report nothing here because `outline` and `stackup` already report it.
fn edge_clearance(board: &Board, issues: &mut CheckReport) {
    let Some(outline) = board.outline
    else {
        return;
    };
    if outline.to[0] <= outline.from[0] || outline.to[1] <= outline.from[1] {
        return;
    }
    let scale = board.millimetres();
    let border = Rect::from_points(geo(outline.from), geo(outline.to));
    let component_rule = board.rules.component_to_edge;
    for footprint in &board.footprints {
        let gap = match pad_bounds(footprint) {
            Some([left, bottom, right, top]) => {
                rect_border_distance(Rect::from_points(geo([left, bottom]), geo([right, top])), border) * scale
            }
            None => point_to_rect_border([footprint.x, footprint.y], border) * scale,
        };
        if gap < component_rule {
            issues.push(finding(
                "drc/component-to-edge",
                Severity::Error,
                board,
                Some(&footprint.layer),
                Some(&footprint.id),
                format!(
                    "footprint `{}` is {gap:.3} mm from the board edge; the rule requires {component_rule} mm",
                    footprint.ref_
                ),
            ));
        }
    }

    let hole_rule = board.rules.hole_to_edge;
    for via in &board.vias {
        let gap = point_to_rect_border(via.at, border) * scale - via.drill * scale / 2.0;
        if gap < hole_rule {
            issues.push(finding(
                "drc/hole-to-edge",
                Severity::Error,
                board,
                None,
                Some(&via.id),
                format!("via `{}` drill is {gap:.3} mm from the board edge; the rule requires {hole_rule} mm", via.id),
            ));
        }
    }
}

/// Checks silkscreen tracks and text against pads on the same side of the board.
///
/// A silkscreen track is legend geometry: its stroked centreline must not enter a
/// pad rectangle, and a stroke that only touches the pad edge is contact rather
/// than overlap. A text item stores no glyph outlines, so its legend is the box
/// the preview writers use: one `0.6 × size` advance per character to the right
/// of the anchor and one `size` above and below it. Both rules report every
/// overlapping pair once, in document order, and only when the silk layer and the
/// pad share a side (`F.` or `B.`).
fn silkscreen_over_pad(board: &Board, issues: &mut CheckReport) {
    let pads = board.placed_pads();
    for track in &board.tracks {
        let Some(side) = silkscreen_side(board, &track.layer)
        else {
            continue;
        };
        for pad in &pads {
            if !copper_on_side(board, pad.layer, side) {
                continue;
            }
            if segment_box_distance(track.from, track.to, pad.at, pad.half) < track.width / 2.0 {
                let object = format!("{}>{}", track.id, pad.key);
                issues.push(finding(
                    "drc/silkscreen-over-pad",
                    Severity::Error,
                    board,
                    Some(&track.layer),
                    Some(&object),
                    format!("silkscreen track `{}` overlaps pad `{}` on the same side", track.id, pad.key),
                ));
            }
        }
    }

    for text in &board.texts {
        let Some(side) = silkscreen_side(board, &text.layer)
        else {
            continue;
        };
        let bounds = text_bounds(text);
        for footprint in &board.footprints {
            if !copper_on_side(board, &footprint.layer, side) {
                continue;
            }
            for pad in &footprint.pads {
                if !convex_overlap(&bounds, &pad_corners(footprint, pad)) {
                    continue;
                }
                let key = format!("{}:{}", footprint.id, pad.number);
                let object = format!("{}>{}", text.id, key);
                issues.push(finding(
                    "drc/silkscreen-over-pad",
                    Severity::Error,
                    board,
                    Some(&text.layer),
                    Some(&object),
                    format!("silkscreen text `{}` overlaps pad `{}` on the same side", text.id, key),
                ));
            }
        }
    }
}

/// Side of the board a silkscreen layer belongs to, or `None` for other layers.
fn silkscreen_side(board: &Board, layer: &str) -> Option<&'static str> {
    board.layer(layer).is_some_and(|layer| layer.kind == LayerKind::Silkscreen).then_some(if layer.starts_with('B') {
        "B."
    }
    else {
        "F."
    })
}

/// Whether a layer is copper on the given side of the board.
fn copper_on_side(board: &Board, layer: &str, side: &str) -> bool {
    layer.starts_with(side) && board.layer(layer).is_some_and(|layer| layer.kind == LayerKind::Copper)
}

/// Axis-aligned legend box of one silkscreen text item, in board units.
///
/// The model stores an anchor, a glyph size and a stroke thickness rather than
/// glyph outlines. This is the box the preview writers use to fit and draw the
/// legend: the text advances `0.6 × size` per character from its anchor and
/// spans one `size` above and below.
fn text_bounds(text: &crate::board::TextItem) -> [[f64; 2]; 4] {
    let right = text.at[0] + text.text.chars().count() as f64 * text.size * 0.6;
    let bottom = text.at[1] - text.size;
    let top = text.at[1] + text.size;
    [[text.at[0], bottom], [right, bottom], [right, top], [text.at[0], top]]
}

/// Compares each declared differential pair against its routing targets, then
/// infers further pairs from net names.
///
/// Length is the sum of the routed copper segments of one net, straight-line
/// per segment; via barrels are not counted. The spacing is the smallest
/// edge-to-edge gap between a positive and a negative segment that share a
/// layer. Both halves must be routed and share at least one layer before a
/// measurement is claimed; otherwise the pair is reported as unrouted.
///
/// Nets that no declared pair covers are grouped by a shared base name with a
/// `_P`/`_N` or `+`/`-` suffix. Both halves must be routed before their skew is
/// measured against [`DIFFPAIR_SKEW_MM`]; a half without a counterpart is
/// reported as unpaired.
fn diffpair(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    if scale <= 0.0 {
        return;
    }
    let mut declared = HashSet::new();
    for pair in &board.differential_pairs {
        declared.insert(pair.positive.as_str());
        declared.insert(pair.negative.as_str());
        let positive: Vec<_> =
            board.tracks.iter().filter(|track| track.net.as_deref() == Some(pair.positive.as_str())).collect();
        let negative: Vec<_> =
            board.tracks.iter().filter(|track| track.net.as_deref() == Some(pair.negative.as_str())).collect();
        if positive.is_empty() || negative.is_empty() {
            let missing = if positive.is_empty() { &pair.positive } else { &pair.negative };
            issues.push(finding(
                "drc/diffpair-unrouted",
                Severity::Warning,
                board,
                None,
                Some(&pair.id),
                format!("differential pair `{}`/`{}` has no copper on net `{missing}`", pair.positive, pair.negative),
            ));
            continue;
        }

        let positive_length = pair_length(&positive) * scale;
        let negative_length = pair_length(&negative) * scale;
        let skew = (positive_length - negative_length).abs();
        let max_skew = pair.max_skew * scale;
        if skew > max_skew + DIFFPAIR_TOLERANCE_MM {
            issues.push(finding(
                "drc/diffpair-length-mismatch",
                Severity::Error,
                board,
                None,
                Some(&pair.id),
                format!(
                    "differential pair `{}`/`{}` differs by {:.3} mm ({:.3} mm versus {:.3} mm); the limit is {:.3} mm",
                    pair.positive, pair.negative, skew, positive_length, negative_length, max_skew
                ),
            ));
        }

        let mut minimum: Option<f64> = None;
        for first in &positive {
            if board.layer(&first.layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
                continue;
            }
            for second in &negative {
                if second.layer != first.layer {
                    continue;
                }
                let centres = segment_distance(first.from, first.to, second.from, second.to);
                let gap = (centres - (first.width + second.width) / 2.0) * scale;
                minimum = Some(minimum.map_or(gap, |current| current.min(gap)));
            }
        }
        let target_gap = pair.gap * scale;
        match minimum {
            Some(gap) if gap < target_gap - DIFFPAIR_TOLERANCE_MM => issues.push(finding(
                "drc/diffpair-spacing",
                Severity::Error,
                board,
                None,
                Some(&pair.id),
                format!(
                    "differential pair `{}`/`{}` closes to {:.3} mm; the target gap is {:.3} mm",
                    pair.positive, pair.negative, gap, target_gap
                ),
            )),
            Some(_) => {}
            None => issues.push(finding(
                "drc/diffpair-unrouted",
                Severity::Warning,
                board,
                None,
                Some(&pair.id),
                format!(
                    "differential pair `{}`/`{}` has no pair of segments on a shared copper layer",
                    pair.positive, pair.negative
                ),
            )),
        }

        let target_width = pair.width * scale;
        if positive
            .iter()
            .chain(negative.iter())
            .any(|track| (track.width * scale - target_width).abs() > DIFFPAIR_TOLERANCE_MM)
        {
            issues.push(finding(
                "drc/diffpair-width",
                Severity::Warning,
                board,
                None,
                Some(&pair.id),
                format!(
                    "differential pair `{}`/`{}` has a track width that differs from the {:.3} mm target",
                    pair.positive, pair.negative, target_width
                ),
            ));
        }
    }

    diffpair_by_name(board, &declared, scale, issues);
}

/// Polarity of one half of a naming-inferred differential pair.
#[derive(Clone, Copy)]
enum PairPolarity {
    /// `_P` or `+` suffix.
    Positive,
    /// `_N` or `-` suffix.
    Negative,
}

/// Splits one net name into its pair base name and polarity.
fn diffpair_member(name: &str) -> Option<(&str, PairPolarity)> {
    if let Some(base) = name.strip_suffix("_P")
        && !base.is_empty()
    {
        return Some((base, PairPolarity::Positive));
    }
    if let Some(base) = name.strip_suffix("_N")
        && !base.is_empty()
    {
        return Some((base, PairPolarity::Negative));
    }
    if let Some(base) = name.strip_suffix('+')
        && !base.is_empty()
    {
        return Some((base, PairPolarity::Positive));
    }
    if let Some(base) = name.strip_suffix('-')
        && !base.is_empty()
    {
        return Some((base, PairPolarity::Negative));
    }
    None
}

/// Groups naming-inferred pairs by base name, in sorted order.
#[derive(Default)]
struct NamedPair<'a> {
    positive: Option<&'a str>,
    negative: Option<&'a str>,
}

/// Reports skew and unpaired halves of pairs inferred from net names.
///
/// Nets already covered by a declared pair are skipped so one board never
/// reports the same pair twice. Groups are visited in base-name order, which
/// keeps the report deterministic.
fn diffpair_by_name(board: &Board, declared: &HashSet<&str>, scale: f64, issues: &mut CheckReport) {
    let mut groups: BTreeMap<&str, NamedPair<'_>> = BTreeMap::new();
    for net in &board.nets {
        let name = net.name.as_str();
        if declared.contains(name) {
            continue;
        }
        let Some((base, polarity)) = diffpair_member(name)
        else {
            continue;
        };
        let group = groups.entry(base).or_default();
        match polarity {
            PairPolarity::Positive => group.positive = group.positive.or(Some(name)),
            PairPolarity::Negative => group.negative = group.negative.or(Some(name)),
        }
    }

    for (base, group) in &groups {
        match (group.positive, group.negative) {
            (Some(positive), Some(negative)) => {
                let positive_length = net_track_length(board, positive, scale);
                let negative_length = net_track_length(board, negative, scale);
                if positive_length <= 0.0 || negative_length <= 0.0 {
                    for (name, length) in [(positive, positive_length), (negative, negative_length)] {
                        if length <= 0.0 {
                            issues.push(finding(
                                "drc/diffpair-unpaired",
                                Severity::Warning,
                                board,
                                None,
                                Some(name),
                                format!("differential pair net `{name}` has no routed track to pair on `{base}`"),
                            ));
                        }
                    }
                    continue;
                }
                let skew = (positive_length - negative_length).abs();
                if skew > DIFFPAIR_SKEW_MM {
                    issues.push(finding(
                        "drc/diffpair-skew",
                        Severity::Warning,
                        board,
                        None,
                        Some(*base),
                        format!(
                            "differential pair `{base}` is skewed by {skew:.3} mm ({positive} {positive_length:.3} mm versus {negative} {negative_length:.3} mm); the limit is {DIFFPAIR_SKEW_MM:.3} mm"
                        ),
                    ));
                }
            }
            (Some(only), None) | (None, Some(only)) => issues.push(finding(
                "drc/diffpair-unpaired",
                Severity::Warning,
                board,
                None,
                Some(only),
                format!("differential pair net `{only}` has no counterpart on `{base}`"),
            )),
            (None, None) => {}
        }
    }
}

/// Total routed length of one net's segments, in board units.
fn pair_length(tracks: &[&crate::board::Track]) -> f64 {
    tracks
        .iter()
        .map(|track| {
            let dx = track.to[0] - track.from[0];
            let dy = track.to[1] - track.from[1];
            (dx * dx + dy * dy).sqrt()
        })
        .sum()
}

/// Total routed length of one named net, in millimetres.
fn net_track_length(board: &Board, net: &str, scale: f64) -> f64 {
    board
        .tracks
        .iter()
        .filter(|track| track.net.as_deref() == Some(net))
        .map(|track| (track.to[0] - track.from[0]).hypot(track.to[1] - track.from[1]) * scale)
        .sum()
}

/// Reports copper track ends that touch no pad, via or same-net track.
///
/// A track endpoint is connected when it reaches a pad of the same net on its
/// layer, a via of the same net whose span covers the layer, an endpoint of
/// another same-net segment on the same layer, or lies on such a segment. A
/// track found dangling at either end is reported once.
fn dangling_copper(board: &Board, issues: &mut CheckReport) {
    let scale = board.millimetres();
    if scale <= 0.0 {
        return;
    }
    let tolerance = CONNECTIVITY_TOLERANCE_MM / scale;
    let pads = board.placed_pads();
    for track in &board.tracks {
        let Some(net) = track.net.as_deref()
        else {
            continue;
        };
        if board.layer(&track.layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
            continue;
        }
        let mut dangling = false;
        for point in [track.from, track.to] {
            if track_point_connected(board, &pads, net, &track.layer, point, track, tolerance) {
                continue;
            }
            dangling = true;
            break;
        }
        if dangling {
            issues.push(finding(
                "drc/dangling-copper",
                Severity::Warning,
                board,
                Some(&track.layer),
                Some(&track.id),
                format!("track `{}` has a loose end that reaches no pad, via or same-net track", track.id),
            ));
        }
    }
}

/// Whether one point on a track reaches other copper of the same net.
fn track_point_connected(
    board: &Board,
    pads: &[crate::board::PlacedPad<'_>],
    net: &str,
    layer: &str,
    point: [f64; 2],
    current: &crate::board::Track,
    tolerance: f64,
) -> bool {
    for pad in pads {
        if pad.net.as_deref() != Some(net) || pad.layer != layer {
            continue;
        }
        if point_box_distance(point, pad.at, pad.half) <= tolerance {
            return true;
        }
    }
    for via in &board.vias {
        if via.net.as_deref() != Some(net) || !board.via_covers(via, layer) {
            continue;
        }
        if near(point, via.at, tolerance) {
            return true;
        }
    }
    for other in &board.tracks {
        if other.id == current.id || other.net.as_deref() != Some(net) || other.layer != layer {
            continue;
        }
        if near(point, other.from, tolerance) || near(point, other.to, tolerance) {
            return true;
        }
        if point_segment_distance(point, other.from, other.to) <= tolerance {
            return true;
        }
    }
    false
}

/// Distance from a point to an axis-aligned pad box; zero inside the box.
fn point_box_distance(point: [f64; 2], centre: [f64; 2], half: [f64; 2]) -> f64 {
    let dx = ((point[0] - centre[0]).abs() - half[0]).max(0.0);
    let dy = ((point[1] - centre[1]).abs() - half[1]).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Distance from a point to a segment.
fn point_segment_distance(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> f64 {
    geo_point_segment_distance(geo(point), geo(start), geo(end))
}

/// Distance from a point to the border of a rectangle; zero on or outside it.
fn point_to_rect_border(point: [f64; 2], rect: Rect) -> f64 {
    if !rect.contains(geo(point)) {
        return 0.0;
    }
    (point[0] - rect.min.x).min(rect.max.x - point[0]).min(point[1] - rect.min.y).min(rect.max.y - point[1]).max(0.0)
}

/// Distance from a segment to the border of a rectangle.
fn segment_to_rect_border(start: [f64; 2], end: [f64; 2], rect: Rect) -> f64 {
    rect_edges(rect)
        .into_iter()
        .map(|(first, second)| segment_distance(start, end, first, second))
        .fold(f64::INFINITY, f64::min)
}

/// Distance between an inner rectangle and the border of an outer one.
fn rect_border_distance(inner: Rect, outer: Rect) -> f64 {
    let gap_x = (outer.min.x - inner.max.x).max(inner.min.x - outer.max.x).max(0.0);
    let gap_y = (outer.min.y - inner.max.y).max(inner.min.y - outer.max.y).max(0.0);
    if gap_x > 0.0 || gap_y > 0.0 {
        return gap_x.hypot(gap_y);
    }
    if outer.contains(inner.min) && outer.contains(inner.max) {
        return (inner.min.x - outer.min.x)
            .min(outer.max.x - inner.max.x)
            .min(inner.min.y - outer.min.y)
            .min(outer.max.y - inner.max.y);
    }
    0.0
}

/// Distance from a segment to an axis-aligned pad box.
fn segment_box_distance(start: [f64; 2], end: [f64; 2], centre: [f64; 2], half: [f64; 2]) -> f64 {
    let box_rect =
        Rect::from_points(geo([centre[0] - half[0], centre[1] - half[1]]), geo([centre[0] + half[0], centre[1] + half[1]]));
    if box_rect.contains(geo(start)) || box_rect.contains(geo(end)) {
        return 0.0;
    }
    rect_edges(box_rect)
        .into_iter()
        .map(|(first, second)| segment_distance(start, end, first, second))
        .fold(f64::INFINITY, f64::min)
}

/// The four border segments of a rectangle, in clockwise order.
fn rect_edges(rect: Rect) -> [([f64; 2], [f64; 2]); 4] {
    let left = rect.min.x;
    let right = rect.max.x;
    let bottom = rect.min.y;
    let top = rect.max.y;
    [
        ([left, bottom], [right, bottom]),
        ([right, bottom], [right, top]),
        ([right, top], [left, top]),
        ([left, top], [left, bottom]),
    ]
}

fn shares_endpoint(first: &crate::board::Track, second: &crate::board::Track) -> bool {
    let same = |left: [f64; 2], right: [f64; 2]| left[0] == right[0] && left[1] == right[1];
    same(first.from, second.from) || same(first.from, second.to) || same(first.to, second.from) || same(first.to, second.to)
}

fn geo(point: [f64; 2]) -> GeoPoint {
    GeoPoint::new(point[0], point[1])
}

fn segment_distance(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> f64 {
    geo_segment_distance(geo(a), geo(b), geo(c), geo(d))
}
