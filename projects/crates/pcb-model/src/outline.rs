//! Polygonal board outline geometry.
//!
//! The stored document outline stays rectangular. This report validates a
//! requested closed polygon of straight edges before it is committed elsewhere,
//! measures it in millimetres and offsets it outward or inward. An offset that
//! collapses a corner or makes the outline self-intersect is rejected instead of
//! being approximated.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::{ServiceError, board::Board, round6};

/// Largest vertex count one report accepts.
const MAX_VERTICES: usize = 4096;

/// Request of [`crate::outline_geometry`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineGeometryRequest {
    /// Closed polygon vertices in board units; omitted uses the stored
    /// rectangular outline.
    #[serde(default)]
    pub points: Option<Vec<[f64; 2]>>,
    /// Outward offset in board units; negative moves inward. Omitted skips it.
    #[serde(default)]
    pub offset: Option<f64>,
}

/// Builds the outline geometry report of one board.
pub(crate) fn report(board: &Board, request: OutlineGeometryRequest) -> Result<Value, ServiceError> {
    let (points, source) = match request.points {
        Some(points) => (points, "request"),
        None => {
            let Some(outline) = board.outline
            else {
                return Err(crate::argument(
                    "this board has no outline; pass polygon `points` or set a rectangular outline first",
                ));
            };
            (
                vec![
                    [outline.from[0], outline.from[1]],
                    [outline.to[0], outline.from[1]],
                    [outline.to[0], outline.to[1]],
                    [outline.from[0], outline.to[1]],
                ],
                "outline",
            )
        }
    };
    validate(&points)?;

    let scale = board.millimetres();
    let area = polygon_area(&points);
    let signed = signed_area(&points);
    let total_perimeter = perimeter(&points);
    let offset = match request.offset {
        Some(distance) => {
            if !distance.is_finite() {
                return Err(crate::argument("`offset` must be a finite number"));
            }
            if distance == 0.0 {
                None
            }
            else {
                let shifted = offset_polygon(&points, distance)?;
                Some(json!({
                    "distance": round6(distance * scale),
                    "vertexCount": shifted.len(),
                    "area": round6(polygon_area(&shifted) * scale * scale),
                    "perimeter": round6(perimeter(&shifted) * scale),
                    "points": shifted
                        .iter()
                        .map(|point| [round6(point[0] * scale), round6(point[1] * scale)])
                        .collect::<Vec<_>>(),
                }))
            }
        }
        None => None,
    };
    Ok(json!({
        "revision": board.revision,
        "units": "mm",
        "source": source,
        "vertexCount": points.len(),
        "orientation": if signed > 0.0 { "counter-clockwise" } else { "clockwise" },
        "area": round6(area * scale * scale),
        "perimeter": round6(total_perimeter * scale),
        "points": points
            .iter()
            .map(|point| [round6(point[0] * scale), round6(point[1] * scale)])
            .collect::<Vec<_>>(),
        "offset": offset,
    }))
}

/// Rejects a polygon the report cannot measure.
fn validate(points: &[[f64; 2]]) -> Result<(), ServiceError> {
    if points.len() < 3 {
        return Err(crate::argument("an outline polygon needs at least three vertices"));
    }
    if points.len() > MAX_VERTICES {
        return Err(crate::argument(format!("an outline polygon accepts at most {MAX_VERTICES} vertices")));
    }
    for point in points {
        if !point[0].is_finite() || !point[1].is_finite() {
            return Err(crate::argument("outline vertices must be finite numbers"));
        }
    }
    for left in 0..points.len() {
        for right in (left + 1)..points.len() {
            if same_point(points[left], points[right]) {
                return Err(crate::argument("the outline polygon repeats a vertex"));
            }
        }
    }
    if polygon_area(points) <= 1e-9 {
        return Err(crate::argument("the outline polygon encloses no area"));
    }
    let count = points.len();
    for left in 0..count {
        for right in (left + 1)..count {
            if right == left + 1 || (left == 0 && right == count - 1) {
                continue;
            }
            if segments_cross(points[left], points[(left + 1) % count], points[right], points[(right + 1) % count]) {
                return Err(crate::argument("the outline polygon self-intersects"));
            }
        }
    }
    Ok(())
}

/// Moves every edge outward by `distance` and joins them at the corners.
fn offset_polygon(points: &[[f64; 2]], distance: f64) -> Result<Vec<[f64; 2]>, ServiceError> {
    let direction = if signed_area(points) >= 0.0 { 1.0 } else { -1.0 };
    let count = points.len();
    let mut shifted = Vec::with_capacity(count);
    for index in 0..count {
        let previous = points[(index + count - 1) % count];
        let current = points[index];
        let next = points[(index + 1) % count];
        let before = outward_normal(previous, current, direction);
        let after = outward_normal(current, next, direction);
        let p1 = [previous[0] + before[0] * distance, previous[1] + before[1] * distance];
        let p2 = [current[0] + before[0] * distance, current[1] + before[1] * distance];
        let p3 = [current[0] + after[0] * distance, current[1] + after[1] * distance];
        let p4 = [next[0] + after[0] * distance, next[1] + after[1] * distance];
        let first = [p2[0] - p1[0], p2[1] - p1[1]];
        let second = [p4[0] - p3[0], p4[1] - p3[1]];
        let denominator = first[0] * second[1] - first[1] * second[0];
        let product = (first[0].hypot(first[1]) * second[0].hypot(second[1])).max(1e-12);
        let corner = if denominator.abs() <= 1e-12 * product {
            p2
        }
        else {
            let ratio = ((p3[0] - p1[0]) * second[1] - (p3[1] - p1[1]) * second[0]) / denominator;
            [p1[0] + first[0] * ratio, p1[1] + first[1] * ratio]
        };
        let miter = (corner[0] - current[0]).hypot(corner[1] - current[1]);
        if !corner[0].is_finite() || !corner[1].is_finite() || miter > distance.abs() * 100.0 {
            return Err(crate::argument("the offset is too large for a corner of the outline"));
        }
        shifted.push(corner);
    }
    validate(&shifted).map_err(|_| crate::argument("the offset produces a self-intersecting or empty outline"))?;
    if signed_area(points).signum() != signed_area(&shifted).signum() {
        return Err(crate::argument("the offset turns the outline inside out"));
    }
    // An offset boundary must sit at least `distance` away from the original
    // boundary. Line intersections can cross past the centre and still form a
    // simple polygon, so this distance test rejects that case.
    let required = distance.abs() * 0.995;
    for corner in &shifted {
        for (index, start) in points.iter().enumerate() {
            let end = points[(index + 1) % count];
            if point_segment_distance(*corner, *start, end) < required {
                return Err(crate::argument("the offset is too large for the outline"));
            }
        }
    }
    Ok(shifted)
}

/// Outward unit normal of one edge, following the polygon winding.
fn outward_normal(from: [f64; 2], to: [f64; 2], direction: f64) -> [f64; 2] {
    let edge = [to[0] - from[0], to[1] - from[1]];
    let length = edge[0].hypot(edge[1]).max(1e-12);
    [edge[1] * direction / length, -edge[0] * direction / length]
}

/// Absolute area of a closed polygon.
fn polygon_area(points: &[[f64; 2]]) -> f64 {
    signed_area(points).abs()
}

/// Signed area of a closed polygon, positive when counter-clockwise.
fn signed_area(points: &[[f64; 2]]) -> f64 {
    let mut sum = 0.0;
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        sum += point[0] * next[1] - next[0] * point[1];
    }
    sum / 2.0
}

/// Closed perimeter length.
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

/// Whether two vertices name the same location.
fn same_point(left: [f64; 2], right: [f64; 2]) -> bool {
    (left[0] - right[0]).abs() <= 1e-9 && (left[1] - right[1]).abs() <= 1e-9
}

/// Whether two segments meet, including a touching endpoint or collinear overlap.
fn segments_cross(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let first = cross(a, b, c);
    let second = cross(a, b, d);
    let third = cross(c, d, a);
    let fourth = cross(c, d, b);
    if ((first > 0.0 && second < 0.0) || (first < 0.0 && second > 0.0))
        && ((third > 0.0 && fourth < 0.0) || (third < 0.0 && fourth > 0.0))
    {
        return true;
    }
    let epsilon = 1e-9;
    (first.abs() <= epsilon && on_segment(a, b, c))
        || (second.abs() <= epsilon && on_segment(a, b, d))
        || (third.abs() <= epsilon && on_segment(c, d, a))
        || (fourth.abs() <= epsilon && on_segment(c, d, b))
}

/// Cross product of the vectors `a→b` and `a→c`.
fn cross(a: [f64; 2], b: [f64; 2], c: [f64; 2]) -> f64 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

/// Whether a collinear point falls inside a segment's bounding box.
fn on_segment(a: [f64; 2], b: [f64; 2], point: [f64; 2]) -> bool {
    point[0] >= a[0].min(b[0]) - 1e-9
        && point[0] <= a[0].max(b[0]) + 1e-9
        && point[1] >= a[1].min(b[1]) - 1e-9
        && point[1] <= a[1].max(b[1]) + 1e-9
}

/// Shortest distance from a point to a segment.
fn point_segment_distance(point: [f64; 2], start: [f64; 2], end: [f64; 2]) -> f64 {
    let edge = [end[0] - start[0], end[1] - start[1]];
    let length = edge[0] * edge[0] + edge[1] * edge[1];
    if length <= 1e-18 {
        return (point[0] - start[0]).hypot(point[1] - start[1]);
    }
    let ratio = (((point[0] - start[0]) * edge[0] + (point[1] - start[1]) * edge[1]) / length).clamp(0.0, 1.0);
    (point[0] - (start[0] + edge[0] * ratio)).hypot(point[1] - (start[1] + edge[1] * ratio))
}
