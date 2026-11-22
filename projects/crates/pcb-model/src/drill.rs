//! Drill tool and hole statistics.
//!
//! The report groups every drilled feature by finished tool diameter, so a
//! fabrication quote can be checked against the Excellon file without parsing
//! its tool table. Vias, drilled pads and plated routed slots share the tool
//! grouping; `holes` still lists vias only, and pad drills are listed separately
//! in `padHoles`.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::{
    board::{Board, Slot, Via},
    round6,
};

/// Builds the drill tool, hole and slot report of one board.
pub(crate) fn report(board: &Board) -> Value {
    let scale = board.millimetres();
    let mut tools: BTreeMap<String, (f64, usize, usize, usize)> = BTreeMap::new();
    for via in &board.vias {
        let diameter = round6(via.drill * scale);
        let entry = tools.entry(format!("{diameter:.3}")).or_insert((diameter, 0, 0, 0));
        entry.1 += 1;
    }
    for slot in &board.slots {
        let width = round6(slot.width * scale);
        let entry = tools.entry(format!("{width:.3}")).or_insert((width, 0, 0, 0));
        entry.2 += 1;
    }
    let pads = board.placed_pads();
    for pad in &pads {
        let Some(drill) = pad.drill
        else {
            continue;
        };
        let diameter = round6(drill * scale);
        let entry = tools.entry(format!("{diameter:.3}")).or_insert((diameter, 0, 0, 0));
        entry.3 += 1;
    }

    let numbers: BTreeMap<String, u32> = tools.keys().enumerate().map(|(index, key)| (key.clone(), index as u32 + 1)).collect();
    let tool_number = |diameter: f64| numbers.get(&format!("{diameter:.3}")).copied().unwrap_or(0);

    let mut holes = Vec::with_capacity(board.vias.len());
    let mut through = 0;
    let mut blind = 0;
    let mut buried = 0;
    let mut micro = 0;
    let mut back_drilled = 0;
    for via in &board.vias {
        let diameter = round6(via.drill * scale);
        match board.via_kind(via) {
            "micro" => micro += 1,
            "blind" => blind += 1,
            "buried" => buried += 1,
            _ => through += 1,
        }
        if via.back_drill.is_some() {
            back_drilled += 1;
        }
        holes.push(hole_value(board, via, scale, diameter, tool_number(diameter)));
    }

    let mut slots = Vec::with_capacity(board.slots.len());
    for slot in &board.slots {
        let width = round6(slot.width * scale);
        slots.push(slot_value(slot, scale, width, tool_number(width)));
    }

    let mut pad_holes = Vec::new();
    for pad in &pads {
        let Some(drill) = pad.drill
        else {
            continue;
        };
        let diameter = round6(drill * scale);
        pad_holes.push(json!({
            "id": pad.key,
            "net": pad.net,
            "tool": tool_number(diameter),
            "at": [round6(pad.at[0] * scale), round6(pad.at[1] * scale)],
            "diameter": diameter,
            "plated": true,
        }));
    }

    let tools: Vec<Value> = tools
        .values()
        .enumerate()
        .map(|(index, (diameter, hole_count, slot_count, pad_count))| {
            let mut value = json!({
                "tool": index as u32 + 1,
                "diameter": round6(*diameter),
                "holes": hole_count,
                "slots": slot_count,
            });
            if *pad_count > 0
                && let Some(object) = value.as_object_mut()
            {
                object.insert("padHoles".to_string(), json!(pad_count));
            }
            value
        })
        .collect();

    json!({
        "revision": board.revision,
        "units": "mm",
        "tools": tools,
        "holeCount": holes.len(),
        "slotCount": slots.len(),
        "padHoleCount": pad_holes.len(),
        "backDrilled": back_drilled,
        "vias": { "through": through, "blind": blind, "buried": buried, "micro": micro },
        "holes": holes,
        "padHoles": pad_holes,
        "slots": slots,
    })
}

/// One via hit in the hole list.
fn hole_value(board: &Board, via: &Via, scale: f64, diameter: f64, tool: u32) -> Value {
    let (from, to) = board.via_endpoints(via);
    let mut value = json!({
        "id": via.id,
        "net": via.net,
        "tool": tool,
        "at": [round6(via.at[0] * scale), round6(via.at[1] * scale)],
        "diameter": diameter,
        "kind": board.via_kind(via),
        "backDrilled": via.back_drill.is_some(),
    });
    if let (Some(from), Some(to)) = (from, to)
        && let Some(object) = value.as_object_mut()
    {
        object.insert("fromLayer".to_string(), Value::String(from));
        object.insert("toLayer".to_string(), Value::String(to));
    }
    value
}

/// One plated routed slot in the slot list.
fn slot_value(slot: &Slot, scale: f64, width: f64, tool: u32) -> Value {
    let from = [slot.from[0] * scale, slot.from[1] * scale];
    let to = [slot.to[0] * scale, slot.to[1] * scale];
    json!({
        "id": slot.id,
        "net": slot.net,
        "tool": tool,
        "from": [round6(from[0]), round6(from[1])],
        "to": [round6(to[0]), round6(to[1])],
        "width": width,
        "length": round6((to[0] - from[0]).hypot(to[1] - from[1])),
    })
}
