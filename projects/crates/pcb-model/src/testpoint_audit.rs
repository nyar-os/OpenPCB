//! Test point coverage report.
//!
//! Every declared net reports the pads and vias that can carry a probe and the
//! declared test points that touch it. A net is covered only by an explicit
//! test point on one of its pads or vias; copper geometry never implies a probe.
//! A net with no pad and no via has no physical contact point at all. The report
//! is read-only.

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::{
    board::{Board, ResolvedTestPoint},
    round6,
};

/// Builds the test point coverage report of one board.
///
/// Only declared nets are listed, in declaration order. Probes are listed once
/// each and keep the reference designator and pad number for a pad target; a
/// probe whose net is not declared still appears in the probe list but adds no
/// covered net.
pub(crate) fn report(board: &Board) -> Value {
    let scale = board.millimetres();
    let probes = board.resolved_test_points();
    let placed = board.placed_pads();
    let mut issues: Vec<Value> = Vec::new();
    let mut nets: Vec<Value> = Vec::with_capacity(board.nets.len());
    let mut covered = 0usize;
    let mut uncovered = 0usize;
    let mut unconnected = 0usize;

    for net in &board.nets {
        let pads = placed.iter().filter(|pad| pad.net.as_deref() == Some(net.name.as_str())).count();
        let vias = board.vias.iter().filter(|via| via.net.as_deref() == Some(net.name.as_str())).count();
        let net_probes: Vec<&ResolvedTestPoint> =
            probes.iter().filter(|probe| probe.net.as_deref() == Some(net.name.as_str())).collect();
        let mut layers: Vec<&str> = net_probes.iter().map(|probe| probe.layer.as_str()).collect();
        layers.sort_unstable();
        layers.dedup();
        let connectable = pads + vias > 0;
        let is_covered = connectable && !net_probes.is_empty();
        if !connectable {
            unconnected += 1;
            issues.push(json!({
                "code": "testpoints/net-unconnected",
                "severity": "error",
                "objectId": net.id,
                "message": format!("net `{}` has no pad or via, so it offers no physical test point", net.name),
            }));
        }
        else if !is_covered {
            uncovered += 1;
            issues.push(json!({
                "code": "testpoints/net-not-covered",
                "severity": "warning",
                "objectId": net.id,
                "message": format!("net `{}` has pads and vias but no declared test point", net.name),
            }));
        }
        else {
            covered += 1;
        }
        nets.push(json!({
            "name": net.name,
            "pads": pads,
            "vias": vias,
            "probes": net_probes.len(),
            "layers": layers,
            "covered": is_covered,
        }));
    }

    let connectable = covered + uncovered;
    let percent = (connectable > 0).then(|| round6(covered as f64 / connectable as f64 * 100.0));
    let status = if issues.iter().any(|issue| issue["severity"] == "error") {
        "error"
    }
    else if issues.is_empty() {
        "pass"
    }
    else {
        "warn"
    };
    let probes: Vec<Value> = probes
        .iter()
        .map(|probe| {
            json!({
                "id": probe.id,
                "target": probe.target,
                "kind": probe.kind,
                "net": probe.net,
                "layer": probe.layer,
                "designator": probe.designator,
                "pad": probe.pad,
                "at": [round6(probe.at[0] * scale), round6(probe.at[1] * scale)],
            })
        })
        .collect();
    let declared: HashSet<&str> = board.nets.iter().map(|net| net.name.as_str()).collect();
    let unassigned =
        probes.len() - probes.iter().filter(|probe| probe["net"].as_str().is_some_and(|net| declared.contains(net))).count();
    json!({
        "report": "open-pcb-testpoint-coverage",
        "revision": board.revision,
        "units": "mm",
        "status": status,
        "netCount": nets.len(),
        "coveredCount": covered,
        "uncoveredCount": uncovered,
        "unconnectedCount": unconnected,
        "probeCount": probes.len(),
        "unassignedProbes": unassigned,
        "coveragePercent": percent,
        "nets": nets,
        "probes": probes,
        "issues": issues,
    })
}
