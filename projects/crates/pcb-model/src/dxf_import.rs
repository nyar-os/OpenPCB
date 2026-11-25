//! DXF board outline reader.
//!
//! The reader parses the minimal AutoCAD R12 DXF the `dxf` export writes: a
//! `HEADER` section declaring `AC1009` and an `ENTITIES` section carrying `LINE`
//! entities in millimetres. Only `Edge.Cuts` lines become the rectangular board
//! outline; every other entity is skipped and reported one by one in the loss
//! ledger, keyed by a stable code from [`LOSS_CODES`]. Geometry the document
//! cannot hold is never approximated: `Edge.Cuts` lines that do not close
//! exactly one axis-aligned rectangle report `dxf/outline-not-rectangular` and
//! leave the board without an outline.

use crate::{
    ServiceError,
    board::{Board, Outline, Units},
    kicad_import::Loss,
};

/// Stable codes the reader may report in its loss ledger.
pub(crate) const LOSS_CODES: &[&str] = &["dxf/construct-not-imported", "dxf/outline-not-rectangular"];

/// A parsed DXF drawing and everything the document did not keep.
pub(crate) struct Imported {
    /// The native board the file maps to.
    pub(crate) board: Board,
    /// Per-item losses in document order.
    pub(crate) losses: Vec<Loss>,
}

/// Parses one DXF document into a board the model can validate.
pub(crate) fn parse(source: &str, id: String) -> Result<Imported, ServiceError> {
    let pairs = pairs(source)?;
    let mut losses = Vec::new();
    let mut lines: Vec<([f64; 2], [f64; 2])> = Vec::new();
    let mut broken = false;
    let mut section = None;
    let mut entities = false;
    let mut index = 0;
    while index < pairs.len() {
        let (code, value) = pairs[index];
        index += 1;
        if code != 0 {
            continue;
        }
        match value {
            "SECTION" => {
                let name = pairs.get(index).filter(|(code, _)| *code == 2).map(|(_, value)| *value);
                match name {
                    Some(name) => {
                        index += 1;
                        section = Some(name);
                        entities |= name == "ENTITIES";
                    }
                    None => return Err(crate::invalid("a DXF `SECTION` has no `2` section name")),
                }
            }
            "ENDSEC" => section = None,
            "EOF" => break,
            kind => {
                if section != Some("ENTITIES") {
                    return Err(crate::invalid(format!("a DXF `{kind}` entity appears outside the ENTITIES section")));
                }
                let mut layer = "0";
                let mut points: [Option<f64>; 4] = [None; 4];
                while index < pairs.len() && pairs[index].0 != 0 {
                    let (code, value) = pairs[index];
                    index += 1;
                    match code {
                        8 => layer = value,
                        10 | 20 | 11 | 21 => {
                            let slot = match code {
                                10 => 0,
                                20 => 1,
                                11 => 2,
                                _ => 3,
                            };
                            points[slot] = value.parse().ok();
                        }
                        _ => {}
                    }
                }
                if kind == "LINE" && layer.eq_ignore_ascii_case("Edge.Cuts") {
                    match points {
                        [Some(x1), Some(y1), Some(x2), Some(y2)] => lines.push(([x1, y1], [x2, y2])),
                        _ => {
                            broken = true;
                            loss(&mut losses, "dxf/outline-not-rectangular", "an Edge.Cuts line has no readable endpoints");
                        }
                    }
                }
                else {
                    loss(
                        &mut losses,
                        "dxf/construct-not-imported",
                        format!("the `{kind}` entity on layer `{layer}` has no board representation"),
                    );
                }
            }
        }
    }
    if !entities {
        return Err(crate::invalid("the DXF has no ENTITIES section"));
    }
    let outline = if broken { None } else { rectangle_from_lines(&lines, &mut losses) };

    let mut board = Board::new(id, None, None)?;
    board.units = Units::Mm;
    board.outline = outline;
    Ok(Imported { board, losses })
}

/// Splits the text into group-code and value pairs, rejecting odd input.
fn pairs(source: &str) -> Result<Vec<(u32, &str)>, ServiceError> {
    let lines: Vec<&str> = source.lines().map(str::trim_end).collect();
    let mut pairs = Vec::with_capacity(lines.len() / 2);
    let mut index = 0;
    while index < lines.len() {
        let Some(code) = lines[index].trim().parse::<u32>().ok()
        else {
            return Err(crate::invalid(format!("`{}` is not a DXF group code", lines[index])));
        };
        let Some(value) = lines.get(index + 1)
        else {
            return Err(crate::invalid(format!("the DXF group code {code} has no value")));
        };
        pairs.push((code, value.trim()));
        index += 2;
    }
    if pairs.first() != Some(&(0, "SECTION")) {
        return Err(crate::invalid("the DXF does not open with a SECTION"));
    }
    Ok(pairs)
}

/// Accepts `Edge.Cuts` lines only when they close exactly one axis-aligned rectangle.
fn rectangle_from_lines(lines: &[([f64; 2], [f64; 2])], losses: &mut Vec<Loss>) -> Option<Outline> {
    const EPSILON: f64 = 0.000_001;
    if lines.is_empty() {
        return None;
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    let mut total = 0.0;
    for (start, end) in lines {
        let axis_aligned = (start[0] - end[0]).abs() <= EPSILON || (start[1] - end[1]).abs() <= EPSILON;
        if !axis_aligned {
            loss(losses, "dxf/outline-not-rectangular", "an Edge.Cuts line is not axis-aligned".to_string());
            return None;
        }
        for point in [start, end] {
            min[0] = min[0].min(point[0]);
            min[1] = min[1].min(point[1]);
            max[0] = max[0].max(point[0]);
            max[1] = max[1].max(point[1]);
        }
        total += (start[0] - end[0]).abs() + (start[1] - end[1]).abs();
    }
    let width = max[0] - min[0];
    let height = max[1] - min[1];
    if width <= EPSILON || height <= EPSILON {
        loss(losses, "dxf/outline-not-rectangular", "the Edge.Cuts lines have no area".to_string());
        return None;
    }
    let perimeter = 2.0 * (width + height);
    if (total - perimeter).abs() > EPSILON {
        loss(losses, "dxf/outline-not-rectangular", "the Edge.Cuts lines do not close exactly one rectangle".to_string());
        return None;
    }
    for (start, end) in lines {
        let horizontal = (start[1] - end[1]).abs() <= EPSILON;
        let on_boundary = if horizontal {
            (start[1] - min[1]).abs() <= EPSILON || (start[1] - max[1]).abs() <= EPSILON
        }
        else {
            (start[0] - min[0]).abs() <= EPSILON || (start[0] - max[0]).abs() <= EPSILON
        };
        if !on_boundary {
            loss(losses, "dxf/outline-not-rectangular", "an Edge.Cuts line is not on the rectangle border".to_string());
            return None;
        }
    }
    Some(Outline { from: min, to: max })
}

fn loss(losses: &mut Vec<Loss>, code: &'static str, message: impl Into<String>) {
    losses.push(Loss { code, message: message.into() });
}
