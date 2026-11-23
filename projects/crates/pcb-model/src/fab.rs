//! Fabrication file manifest.
//!
//! The manifest mirrors the Gerber writer's file set without writing anything:
//! one copper file per copper layer, front and back silkscreen, mask and paste
//! files, and the profile. It also counts the drill tools and the assembly rows
//! the other writers emit. Cross-checks flag the conditions that make the
//! Gerber writer fail or turn out a file with no source data.

use std::collections::{BTreeSet, HashMap};

use serde_json::{Value, json};

use crate::{
    board::{Board, LayerKind},
    round6,
};

/// One file the Gerber writer plans to emit.
struct PlannedFile {
    name: String,
    function: String,
    kind: &'static str,
    layer: Option<String>,
    objects: usize,
}

/// Builds the fabrication manifest and consistency report of one board.
pub(crate) fn manifest(board: &Board) -> Value {
    let scale = board.millimetres();
    let stack = board.copper_stack();
    let mut issues: Vec<Value> = Vec::new();
    let mut files: Vec<PlannedFile> = Vec::new();

    for (index, layer) in stack.iter().enumerate() {
        let layer = *layer;
        let tracks = board.tracks.iter().filter(|track| track.layer == layer).count();
        let pads: usize =
            board.footprints.iter().filter(|footprint| footprint.layer == layer).map(|footprint| footprint.pads.len()).sum();
        let vias = board.vias.iter().filter(|via| board.via_covers(via, layer)).count();
        let zones = board.zones.iter().filter(|zone| zone.layer == layer).count();
        let objects = tracks + pads + vias + zones;
        if objects == 0 {
            issues.push(issue(
                "fab/empty-copper-layer",
                "warning",
                layer,
                format!("copper layer `{layer}` has no tracks, pads, vias or pours; its Gerber file is written empty"),
            ));
        }
        let position = if index == 0 {
            "Top"
        }
        else if index + 1 == stack.len() {
            "Bot"
        }
        else {
            "Inr"
        };
        files.push(PlannedFile {
            name: gerber_name(layer),
            function: format!("Copper,L{},{}", index + 1, position),
            kind: "copper",
            layer: Some(layer.to_string()),
            objects,
        });
    }

    for (layer_id, side) in [("F.SilkS", "Top"), ("B.SilkS", "Bot")] {
        if board.layer(layer_id).is_none_or(|layer| layer.kind != LayerKind::Silkscreen) {
            issues.push(issue(
                "fab/missing-silkscreen-layer",
                "warning",
                layer_id,
                format!("silkscreen layer `{layer_id}` is not declared; its Gerber file is written empty"),
            ));
        }
        let objects = board.tracks.iter().filter(|track| track.layer == layer_id).count();
        files.push(PlannedFile {
            name: gerber_name(layer_id),
            function: format!("Legend,{side}"),
            kind: "silkscreen",
            layer: Some(layer_id.to_string()),
            objects,
        });
    }

    for (copper_layer, side) in [("F.Cu", "Top"), ("B.Cu", "Bot")] {
        if !stack.contains(&copper_layer) {
            issues.push(issue(
                "fab/orphan-mask-layer",
                "warning",
                copper_layer,
                format!(
                    "copper layer `{copper_layer}` is not in the stack; its mask file is written without an opening source"
                ),
            ));
        }
        let pads: usize = board
            .footprints
            .iter()
            .filter(|footprint| footprint.layer == copper_layer)
            .map(|footprint| footprint.pads.len())
            .sum();
        files.push(PlannedFile {
            name: format!("{}_Mask.gbr", if side == "Top" { "F" } else { "B" }),
            function: format!("Soldermask,{side}"),
            kind: "soldermask",
            layer: Some(copper_layer.to_string()),
            objects: pads,
        });
    }

    for (copper_layer, side) in [("F.Cu", "Top"), ("B.Cu", "Bot")] {
        if !stack.contains(&copper_layer) {
            issues.push(issue(
                "fab/orphan-paste-layer",
                "warning",
                copper_layer,
                format!(
                    "copper layer `{copper_layer}` is not in the stack; its paste file is written without an opening source"
                ),
            ));
        }
        let pads: usize = board
            .footprints
            .iter()
            .filter(|footprint| footprint.layer == copper_layer)
            .map(|footprint| footprint.pads.len())
            .sum();
        files.push(PlannedFile {
            name: format!("{}_Paste.gbr", if side == "Top" { "F" } else { "B" }),
            function: format!("Paste,{side}"),
            kind: "paste",
            layer: Some(copper_layer.to_string()),
            objects: pads,
        });
    }

    if board.outline.is_some() {
        files.push(PlannedFile {
            name: "Edge_Cuts.gbr".to_string(),
            function: "Profile,NP".to_string(),
            kind: "profile",
            layer: None,
            objects: 4,
        });
    }
    else {
        issues.push(issue(
            "fab/no-outline",
            "error",
            "",
            "the board has no outline; Gerber export rejects the board before writing any file".to_string(),
        ));
    }

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for file in &files {
        *counts.entry(file.name.as_str()).or_default() += 1;
    }
    let mut collisions: Vec<(&str, usize)> = counts.into_iter().filter(|(_, count)| *count > 1).collect();
    collisions.sort_unstable_by(|left, right| left.0.cmp(right.0));
    for (name, count) in collisions {
        issues.push(issue(
            "fab/file-name-collision",
            "error",
            name,
            format!("{count} planned Gerber files map to `{name}`; the writer would overwrite its own output"),
        ));
    }

    let mut diameters: BTreeSet<String> = BTreeSet::new();
    for via in &board.vias {
        diameters.insert(format!("{:.3}", round6(via.drill * scale)));
    }
    for slot in &board.slots {
        diameters.insert(format!("{:.3}", round6(slot.width * scale)));
    }
    let pads = board.placed_pads();
    let mut pad_holes = 0usize;
    for pad in &pads {
        if let Some(drill) = pad.drill {
            diameters.insert(format!("{:.3}", round6(drill * scale)));
            pad_holes += 1;
        }
    }
    if diameters.is_empty() {
        issues.push(issue(
            "fab/no-drill-data",
            "warning",
            "",
            "the board stores no via, slot or pad drill; the Excellon file is written with no tools".to_string(),
        ));
    }
    let groups: BTreeSet<(String, String)> =
        board.footprints.iter().map(|footprint| (footprint.value.clone(), footprint.footprint.clone())).collect();
    if board.footprints.is_empty() {
        issues.push(issue(
            "fab/no-placements",
            "warning",
            "",
            "the board has no footprints; the pick-and-place and BOM files are written with no rows".to_string(),
        ));
    }

    let status = status_of(&issues);
    let files: Vec<Value> = files
        .iter()
        .map(|file| {
            json!({
                "name": file.name,
                "function": file.function,
                "kind": file.kind,
                "layer": file.layer,
                "objects": file.objects,
            })
        })
        .collect();
    json!({
        "report": "open-pcb-fab-manifest",
        "revision": board.revision,
        "units": "mm",
        "status": status,
        "outline": board.outline.is_some(),
        "fileCount": files.len(),
        "copperLayers": stack,
        "files": files,
        "drill": {
            "tools": diameters.len(),
            "holes": board.vias.len(),
            "slots": board.slots.len(),
            "padHoles": pad_holes,
        },
        "assembly": {
            "placements": board.footprints.len(),
            "bomGroups": groups.len(),
        },
        "issues": issues,
    })
}

/// One Gerber file name for a layer id, with `.` replaced by `_`.
fn gerber_name(layer: &str) -> String {
    format!("{}.gbr", layer.replace('.', "_"))
}

/// Builds one manifest finding.
fn issue(code: &str, severity: &str, object_id: &str, message: String) -> Value {
    json!({ "code": code, "severity": severity, "objectId": object_id, "message": message })
}

/// Overall status derived from the findings.
fn status_of(issues: &[Value]) -> &'static str {
    if issues.iter().any(|issue| issue["severity"] == "error") {
        "error"
    }
    else if issues.is_empty() {
        "pass"
    }
    else {
        "warn"
    }
}
