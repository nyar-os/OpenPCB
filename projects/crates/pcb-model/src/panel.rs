//! Panelization report for one rectangular board.
//!
//! A panel repeats the board across a grid with optional gaps and edge rails.
//! The report gives the panel outline, every placement and the area
//! utilization. Internal seams are scored when the tab style is `v-cut`; with
//! `mouse-bite` the report also places tab rectangles over each gap. Both are
//! geometry placeholders: no fabrication file is written.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{ServiceError, board::Board, round6};

/// Panel tab styles this build can report.
const TAB_STYLES: &[&str] = &["v-cut", "mouse-bite", "none"];

/// Largest board count one report may describe.
const MAX_BOARDS: u32 = 1024;

/// Default tab width, in millimetres.
const DEFAULT_TAB_WIDTH_MM: f64 = 5.0;

/// Default tab spacing along a seam, in millimetres.
const DEFAULT_TAB_SPACING_MM: f64 = 20.0;

/// Request of [`crate::panelize`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelizeRequest {
    /// Boards across one row; at least one.
    pub columns: u32,
    /// Rows of boards; at least one.
    pub rows: u32,
    /// Gap between neighbouring columns, in millimetres.
    #[serde(default)]
    pub gap_x: Option<f64>,
    /// Gap between neighbouring rows, in millimetres.
    #[serde(default)]
    pub gap_y: Option<f64>,
    /// Width of the border that surrounds the array, in millimetres.
    #[serde(default)]
    pub rail: Option<f64>,
    /// Tab style: `v-cut` (default), `mouse-bite` or `none`.
    #[serde(default)]
    pub tab: Option<String>,
    /// Length of one mouse-bite tab along its seam, in millimetres.
    #[serde(default)]
    pub tab_width: Option<f64>,
    /// Distance between tab centres along a seam, in millimetres.
    #[serde(default)]
    pub tab_spacing: Option<f64>,
    /// Finished diameter of the four rail tooling holes, in millimetres.
    #[serde(default)]
    pub tooling_hole: Option<f64>,
    /// Distance from the panel edge to a tooling-hole centre, in millimetres.
    #[serde(default)]
    pub tooling_hole_inset: Option<f64>,
}

/// Builds the panelization report of one board.
pub(crate) fn report(board: &Board, request: PanelizeRequest) -> Result<Value, ServiceError> {
    let tab = request.tab.as_deref().map(str::trim).filter(|value| !value.is_empty()).unwrap_or("v-cut");
    if !TAB_STYLES.contains(&tab) {
        return Err(crate::argument(format!("`{tab}` is not a tab style; use `v-cut`, `mouse-bite` or `none`")));
    }
    let count = request
        .columns
        .checked_mul(request.rows)
        .filter(|count| *count > 0 && *count <= MAX_BOARDS)
        .ok_or_else(|| crate::argument(format!("`columns` and `rows` must describe between 1 and {MAX_BOARDS} boards")))?;
    let gap_x = length(request.gap_x.unwrap_or(0.0), "gapX")?;
    let gap_y = length(request.gap_y.unwrap_or(0.0), "gapY")?;
    let rail = length(request.rail.unwrap_or(0.0), "rail")?;
    let tab_width = length(request.tab_width.unwrap_or(DEFAULT_TAB_WIDTH_MM), "tabWidth")?;
    let tab_spacing = length(request.tab_spacing.unwrap_or(DEFAULT_TAB_SPACING_MM), "tabSpacing")?;
    let tooling_hole = match request.tooling_hole {
        Some(value) => {
            let diameter = length(value, "toolingHole")?;
            if diameter <= 0.0 {
                return Err(crate::argument("`toolingHole` must be greater than zero"));
            }
            Some(diameter)
        }
        None => None,
    };
    let tooling_inset = request.tooling_hole_inset.map(|value| length(value, "toolingHoleInset")).transpose()?;

    let Some(outline) = board.outline
    else {
        return Err(crate::argument("panelization needs a board outline; set one first"));
    };
    let scale = board.millimetres();
    let board_width = (outline.to[0] - outline.from[0]) * scale;
    let board_height = (outline.to[1] - outline.from[1]) * scale;
    let (columns, rows) = (request.columns, request.rows);

    if tab == "mouse-bite" {
        if tab_width <= 0.0 {
            return Err(crate::argument("`tabWidth` must be greater than zero"));
        }
        if tab_spacing <= 0.0 {
            return Err(crate::argument("`tabSpacing` must be greater than zero"));
        }
        if tab_width > tab_spacing {
            return Err(crate::argument("`tabWidth` must not exceed `tabSpacing`"));
        }
        if (columns > 1 && gap_x <= 0.0) || (rows > 1 && gap_y <= 0.0) {
            return Err(crate::argument("mouse-bite tabs need a positive `gapX` and `gapY` where boards meet"));
        }
    }

    let panel_width = columns as f64 * board_width + (columns - 1) as f64 * gap_x + rail * 2.0;
    let panel_height = rows as f64 * board_height + (rows - 1) as f64 * gap_y + rail * 2.0;
    let panel_from = [outline.from[0] * scale - rail, outline.from[1] * scale - rail];
    let panel_to = [panel_from[0] + panel_width, panel_from[1] + panel_height];

    let tooling_inset = match tooling_hole {
        Some(diameter) => {
            if rail <= 0.0 {
                return Err(crate::argument("tooling holes need a positive `rail` to sit in"));
            }
            let radius = diameter / 2.0;
            let inset = tooling_inset.unwrap_or(rail / 2.0);
            if inset - radius < 0.0 || inset + radius > rail {
                return Err(crate::argument(format!(
                    "a {diameter} mm tooling hole must fit inside the {rail} mm rail; the inset allows {radius} to {} mm",
                    rail - radius
                )));
            }
            Some(inset)
        }
        None => None,
    };
    let tooling_holes: Vec<Value> = match tooling_inset {
        Some(inset) => [
            ("bottom-left", panel_from[0] + inset, panel_from[1] + inset),
            ("bottom-right", panel_to[0] - inset, panel_from[1] + inset),
            ("top-right", panel_to[0] - inset, panel_to[1] - inset),
            ("top-left", panel_from[0] + inset, panel_to[1] - inset),
        ]
        .iter()
        .map(|(corner, x, y)| {
            json!({
                "corner": corner,
                "x": round6(*x),
                "y": round6(*y),
                "diameter": round6(tooling_hole.unwrap_or(0.0)),
            })
        })
        .collect(),
        None => Vec::new(),
    };

    let mut placements = Vec::with_capacity(count as usize);
    for row in 0..rows {
        for column in 0..columns {
            let from = [
                panel_from[0] + rail + column as f64 * (board_width + gap_x),
                panel_from[1] + rail + row as f64 * (board_height + gap_y),
            ];
            placements.push(json!({
                "column": column,
                "row": row,
                "from": [round6(from[0]), round6(from[1])],
                "to": [round6(from[0] + board_width), round6(from[1] + board_height)],
            }));
        }
    }

    let mut seams = Vec::new();
    for column in 0..columns.saturating_sub(1) {
        let at = panel_from[0] + rail + (column + 1) as f64 * board_width + (column as f64 + 0.5) * gap_x;
        seams.push(json!({
            "orientation": "vertical",
            "at": round6(at),
            "from": [round6(at), round6(panel_from[1])],
            "to": [round6(at), round6(panel_to[1])],
            "length": round6(panel_height),
        }));
    }
    for row in 0..rows.saturating_sub(1) {
        let at = panel_from[1] + rail + (row + 1) as f64 * board_height + (row as f64 + 0.5) * gap_y;
        seams.push(json!({
            "orientation": "horizontal",
            "at": round6(at),
            "from": [round6(panel_from[0]), round6(at)],
            "to": [round6(panel_to[0]), round6(at)],
            "length": round6(panel_width),
        }));
    }

    let mut tabs = Vec::new();
    if tab == "mouse-bite" {
        for seam in &seams {
            let vertical = seam["orientation"] == "vertical";
            let at = seam["at"].as_f64().unwrap_or(0.0);
            let length = seam["length"].as_f64().unwrap_or(0.0);
            let slots = (length / tab_spacing).floor().max(1.0);
            let step = length / slots;
            for index in 0..slots as u32 {
                let centre = panel_from[1] + step * (index as f64 + 0.5);
                if vertical {
                    tabs.push(json!({
                        "orientation": "vertical",
                        "x": round6(at - gap_x / 2.0),
                        "y": round6(centre - tab_width / 2.0),
                        "width": round6(gap_x),
                        "height": round6(tab_width),
                    }));
                }
                else {
                    let centre = panel_from[0] + step * (index as f64 + 0.5);
                    tabs.push(json!({
                        "orientation": "horizontal",
                        "x": round6(centre - tab_width / 2.0),
                        "y": round6(at - gap_y / 2.0),
                        "width": round6(tab_width),
                        "height": round6(gap_y),
                    }));
                }
            }
        }
    }

    let board_area = board_width * board_height;
    let panel_area = panel_width * panel_height;
    let content_area = (columns as f64 * board_width + (columns - 1) as f64 * gap_x)
        * (rows as f64 * board_height + (rows - 1) as f64 * gap_y);
    let board_total = board_area * count as f64;
    let breakaway_area = panel_area - board_total;
    let utilization = if panel_area > 0.0 { board_total / panel_area * 100.0 } else { 0.0 };
    let breakaway_percent = if panel_area > 0.0 { breakaway_area / panel_area * 100.0 } else { 0.0 };
    let tooling_count = tooling_holes.len();
    Ok(json!({
        "revision": board.revision,
        "units": "mm",
        "columns": columns,
        "rows": rows,
        "boardCount": count,
        "gapX": round6(gap_x),
        "gapY": round6(gap_y),
        "rail": round6(rail),
        "boardSize": { "width": round6(board_width), "height": round6(board_height) },
        "panelSize": { "width": round6(panel_width), "height": round6(panel_height) },
        "panelOutline": {
            "from": [round6(panel_from[0]), round6(panel_from[1])],
            "to": [round6(panel_to[0]), round6(panel_to[1])],
        },
        "boardArea": round6(board_area),
        "panelArea": round6(panel_area),
        "utilizationPercent": round6(utilization),
        "breakaway": {
            "area": round6(breakaway_area),
            "percent": round6(breakaway_percent),
            "railArea": round6(panel_area - content_area),
            "gapArea": round6(content_area - board_total),
        },
        "toolingHoles": tooling_holes,
        "toolingHoleCount": tooling_count,
        "tab": tab,
        "seams": seams,
        "tabs": tabs,
        "tabCount": tabs.len(),
        "placements": placements,
    }))
}

/// Rejects a negative or non-finite length.
fn length(value: f64, field: &str) -> Result<f64, ServiceError> {
    if !value.is_finite() {
        return Err(crate::argument(format!("`{field}` must be a finite number")));
    }
    if value < 0.0 {
        return Err(crate::argument(format!("`{field}` must not be negative")));
    }
    Ok(value)
}
