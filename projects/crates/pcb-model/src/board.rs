//! Native board document schema, validation and summary projection.
//!
//! The types in this module mirror the `.opcb` document. They stay internal:
//! callers reach a board through the handle-based operations in the crate root,
//! so the wire shape can evolve without exposing Rust structs.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::ServiceError;

/// Tag written to every native document.
pub(crate) const DOCUMENT_FORMAT: &str = "open-pcb-board";

/// Schema version this build reads and writes.
pub(crate) const DOCUMENT_VERSION: u32 = 1;

/// Coordinate unit of one board document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Units {
    /// Millimetres.
    Mm,
    /// Thousandths of an inch.
    Mil,
}

/// Role of one layer in the stackup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LayerKind {
    /// Electrical copper.
    Copper,
    /// Printed legend.
    Silkscreen,
    /// Solder mask opening.
    Soldermask,
    /// Assembly drawing.
    Assembly,
    /// Mechanical outline or documentation.
    Mechanical,
}

/// One stackup layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Layer {
    /// Stable layer id, also used by tracks and footprints.
    pub(crate) id: String,
    /// Display name.
    pub(crate) name: String,
    /// Role in the stackup.
    pub(crate) kind: LayerKind,
    /// CSS colour used by the SVG writer.
    pub(crate) color: String,
    /// Whether the layer is drawn.
    pub(crate) visible: bool,
    /// Whether the layer rejects edits.
    pub(crate) locked: bool,
}

/// One electrical net.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Net {
    /// Stable net id.
    pub(crate) id: String,
    /// Net name referenced by pads, tracks and vias.
    pub(crate) name: String,
}

/// One pad of a footprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Pad {
    /// Pad number or name.
    pub(crate) number: String,
    /// Pad centre, X.
    pub(crate) x: f64,
    /// Pad centre, Y.
    pub(crate) y: f64,
    /// Pad width.
    pub(crate) w: f64,
    /// Pad height.
    pub(crate) h: f64,
    /// Declared net name, when assigned.
    pub(crate) net: Option<String>,
}

/// One placed footprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Footprint {
    /// Stable footprint id.
    pub(crate) id: String,
    /// Reference designator.
    #[serde(rename = "ref")]
    pub(crate) ref_: String,
    /// Component value.
    pub(crate) value: String,
    /// Package or footprint name.
    pub(crate) footprint: String,
    /// Placement X.
    pub(crate) x: f64,
    /// Placement Y.
    pub(crate) y: f64,
    /// Rotation in degrees.
    pub(crate) rotation: f64,
    /// Layer the footprint is placed on.
    pub(crate) layer: String,
    /// Pads owned by the footprint.
    pub(crate) pads: Vec<Pad>,
}

impl Footprint {
    /// Serialises the public shape (reference designator is `ref`).
    pub(crate) fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "ref": self.ref_,
            "value": self.value,
            "footprint": self.footprint,
            "x": self.x,
            "y": self.y,
            "rotation": self.rotation,
            "layer": self.layer,
            "pads": self.pads,
        })
    }
}

/// One routed track segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Track {
    /// Stable track id.
    pub(crate) id: String,
    /// Declared net name, when assigned.
    pub(crate) net: Option<String>,
    /// Copper layer the segment lives on.
    pub(crate) layer: String,
    /// Trace width.
    pub(crate) width: f64,
    /// Segment start.
    pub(crate) from: [f64; 2],
    /// Segment end.
    pub(crate) to: [f64; 2],
    /// Whether this segment was generated as teardrop copper.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub(crate) teardrop: bool,
}

/// One pad resolved to board coordinates, as the router and checks see it.
///
/// Footprint rotation is folded into the centre and the axis-aligned half
/// extents, so callers never have to repeat that geometry.
pub(crate) struct PlacedPad<'a> {
    /// Stable key `footprint-id:pad-number`.
    pub(crate) key: String,
    /// Declared net name, when the pad is assigned.
    pub(crate) net: Option<String>,
    /// Pad centre in board coordinates.
    pub(crate) at: [f64; 2],
    /// Axis-aligned half extents after footprint rotation.
    pub(crate) half: [f64; 2],
    /// Layer the owning footprint is placed on.
    pub(crate) layer: &'a str,
}

/// One via.
///
/// A via without `from_layer`/`to_layer` spans the whole copper stack; vias
/// that set both endpoints are blind or buried. The document stores the span
/// in snake_case and omits it for through vias, so documents written before
/// blind and buried vias existed reopen unchanged.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Via {
    /// Stable via id.
    pub(crate) id: String,
    /// Declared net name, when assigned.
    pub(crate) net: Option<String>,
    /// Via centre.
    pub(crate) at: [f64; 2],
    /// Copper diameter.
    pub(crate) diameter: f64,
    /// Finished drill diameter.
    pub(crate) drill: f64,
    /// Copper layer the span starts on, for a blind or buried via.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) from_layer: Option<String>,
    /// Copper layer the span ends on, for a blind or buried via.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) to_layer: Option<String>,
    /// Back-drill information, when the via barrel is back drilled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) back_drill: Option<BackDrill>,
}

/// One plated routed slot.
///
/// A slot is a straight cut with round ends that spans every copper layer, so
/// the Excellon writer emits it as one `G85` routed segment under a tool whose
/// diameter is the slot width. Back drilling only applies to vias.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Slot {
    /// Stable slot id.
    pub(crate) id: String,
    /// Declared net name, when assigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) net: Option<String>,
    /// One end of the slot centreline.
    pub(crate) from: [f64; 2],
    /// The other end of the slot centreline.
    pub(crate) to: [f64; 2],
    /// Tool diameter, equal to the cut width.
    pub(crate) width: f64,
}

impl Slot {
    /// Api projection of one slot.
    pub(crate) fn to_value(&self) -> Value {
        json!({ "id": self.id, "net": self.net, "from": self.from, "to": self.to, "width": self.width })
    }
}

/// Back-drill depth and remaining stub on one through via.
///
/// Back drilling removes unused barrel copper from the secondary side. The
/// document stores the removed depth and the remaining stub so fabrication data
/// can be derived without re-reading the design intent.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct BackDrill {
    /// Depth removed from the secondary side, in board units.
    pub(crate) depth: f64,
    /// Remaining stub length after back drilling, in board units.
    pub(crate) stub: f64,
}

/// One declared differential pair.
///
/// The pair names two nets and the routing targets compared by the `diffpair`
/// check: the trace width, the edge-to-edge gap and the largest allowed length
/// difference between the two nets. Nothing is routed by declaring a pair; the
/// check only compares the copper the document already holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DifferentialPair {
    /// Stable pair id.
    pub(crate) id: String,
    /// Net routed as the positive half.
    pub(crate) positive: String,
    /// Net routed as the negative half.
    pub(crate) negative: String,
    /// Target trace width.
    pub(crate) width: f64,
    /// Target edge-to-edge gap between the two halves.
    pub(crate) gap: f64,
    /// Largest allowed route-length difference between the halves.
    pub(crate) max_skew: f64,
}

/// One waived inspection finding.
///
/// A waiver silences one finding code, optionally narrowed to a single object
/// id. The reason is free text the report echoes back, so a review can say why
/// the finding was accepted without reading the document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Waiver {
    /// Stable waiver id.
    pub(crate) id: String,
    /// Finding code the waiver silences, such as `drc/track-width-too-small`.
    pub(crate) code: String,
    /// Object id the waiver is narrowed to; absent means every object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) object: Option<String>,
    /// Human-readable reason the finding was accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
}

impl Waiver {
    /// Api projection of one waiver, omitting absent fields.
    pub(crate) fn to_value(&self) -> Value {
        let mut value = json!({ "id": self.id, "code": self.code });
        if let Some(object) = &self.object {
            value["object"] = Value::String(object.clone());
        }
        if let Some(reason) = &self.reason {
            value["reason"] = Value::String(reason.clone());
        }
        value
    }
}

impl DifferentialPair {
    /// Api projection of one pair, with camelCase field names.
    pub(crate) fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "positive": self.positive,
            "negative": self.negative,
            "width": self.width,
            "gap": self.gap,
            "maxSkew": self.max_skew,
        })
    }
}

/// Rectangular board outline in board units.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct Outline {
    /// Lower-left corner.
    pub(crate) from: [f64; 2],
    /// Upper-right corner.
    pub(crate) to: [f64; 2],
}

/// Physical layer stack used by controlled-impedance calculations.
///
/// The values describe the copper foil and the dielectric between adjacent
/// copper layers, in millimetres. The impedance helpers read this when a caller
/// does not override a value per request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Stackup {
    /// Copper foil thickness, in millimetres.
    pub(crate) copper_thickness: f64,
    /// Dielectric thickness between adjacent copper layers, in millimetres.
    pub(crate) dielectric_height: f64,
    /// Relative permittivity of the dielectric.
    pub(crate) dielectric_constant: f64,
}

impl Stackup {
    /// Api projection of the stackup.
    pub(crate) fn to_value(self) -> Value {
        json!({
            "copperThickness": self.copper_thickness,
            "dielectricHeight": self.dielectric_height,
            "dielectricConstant": self.dielectric_constant,
        })
    }
}

/// One named net class with routing targets.
///
/// Nets assigned to a class inherit its clearance and the width, via diameter
/// and via drill minima that the `netclass` check enforces. Every value is
/// optional, so a class can be declared first and filled in later.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetClass {
    /// Stable class id.
    pub(crate) id: String,
    /// Class name referenced by assignments.
    pub(crate) name: String,
    /// Target trace width, in board units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) width: Option<f64>,
    /// Minimum copper clearance for nets in the class, in board units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) clearance: Option<f64>,
    /// Minimum via copper diameter, in board units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) via_diameter: Option<f64>,
    /// Minimum via finished drill, in board units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) via_drill: Option<f64>,
    /// Net names assigned to the class.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) nets: Vec<String>,
}

impl NetClass {
    /// Api projection of one class, omitting absent targets.
    pub(crate) fn to_value(&self) -> Value {
        let mut value = json!({ "id": self.id, "name": self.name, "nets": self.nets });
        if let Some(width) = self.width {
            value["width"] = json!(width);
        }
        if let Some(clearance) = self.clearance {
            value["clearance"] = json!(clearance);
        }
        if let Some(via_diameter) = self.via_diameter {
            value["viaDiameter"] = json!(via_diameter);
        }
        if let Some(via_drill) = self.via_drill {
            value["viaDrill"] = json!(via_drill);
        }
        value
    }
}

/// One declared test point.
///
/// The target is either `footprint-id:pad-number` for a pad or a via id. The
/// marker stores no geometry, so moving or rotating a footprint or via moves
/// the reported probe position with it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TestPoint {
    /// Stable test point id.
    pub(crate) id: String,
    /// Pad key or via id the probe touches.
    pub(crate) target: String,
}

/// One test point resolved to a probe position.
pub(crate) struct ResolvedTestPoint {
    /// Stable test point id.
    pub(crate) id: String,
    /// Pad key or via id.
    pub(crate) target: String,
    /// Reference designator for a pad; empty for a via.
    pub(crate) designator: String,
    /// Pad number for a pad; empty for a via.
    pub(crate) pad: String,
    /// `pad` or `via`.
    pub(crate) kind: &'static str,
    /// Declared net name, when assigned.
    pub(crate) net: Option<String>,
    /// Layer for a pad, or the via span label.
    pub(crate) layer: String,
    /// Probe position in board units.
    pub(crate) at: [f64; 2],
}

/// One copper pour region on a copper layer.
///
/// A zone is a closed polygon with at least three vertices. It carries no fill
/// pattern: the Gerber writer emits the polygon as one `G36`/`G37` region, so
/// fabrication sees solid copper on the declared layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Zone {
    /// Stable zone id.
    pub(crate) id: String,
    /// Copper layer the pour lives on.
    pub(crate) layer: String,
    /// Declared net the pour belongs to, when assigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) net: Option<String>,
    /// Polygon vertices in board units, in drawing order.
    pub(crate) points: Vec<[f64; 2]>,
}

/// One silkscreen text item.
///
/// Text is a legend annotation, not copper: it sits on a silkscreen layer and
/// carries its own position, rotation, glyph size and stroke thickness. The
/// KiCad writer emits it as `gr_text` and the preview draws it as SVG text; the
/// Gerber legend writer does not stroke glyphs yet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextItem {
    /// Stable text id.
    pub(crate) id: String,
    /// Text content; never empty after trimming.
    pub(crate) text: String,
    /// Silkscreen layer the legend lives on.
    pub(crate) layer: String,
    /// Anchor position in board units.
    pub(crate) at: [f64; 2],
    /// Rotation in degrees, counter-clockwise.
    pub(crate) rotation: f64,
    /// Glyph height in board units.
    pub(crate) size: f64,
    /// Stroke thickness in board units.
    pub(crate) thickness: f64,
}

impl TextItem {
    /// Api projection of one text item.
    pub(crate) fn to_value(&self) -> Value {
        json!({
            "id": self.id,
            "text": self.text,
            "layer": self.layer,
            "at": self.at,
            "rotation": self.rotation,
            "size": self.size,
            "thickness": self.thickness,
        })
    }
}

/// One copper keepout region.
///
/// A keepout is a closed polygon with at least three vertices that no copper
/// object may enter. The layer is either a copper layer id or `*`, which rejects
/// copper on every copper layer. The `keepout` check reads these regions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Keepout {
    /// Stable keepout id.
    pub(crate) id: String,
    /// Copper layer id, or `*` for every copper layer.
    pub(crate) layer: String,
    /// Polygon vertices in board units, in drawing order.
    pub(crate) points: Vec<[f64; 2]>,
}

/// Default minimum gap between two drill holes of different nets, in mm.
const DEFAULT_HOLE_CLEARANCE_MM: f64 = 0.25;

/// Default minimum copper-to-board-edge gap, in mm.
const DEFAULT_COPPER_TO_EDGE_MM: f64 = 0.3;

/// Default minimum component (pad bounding box) to board-edge gap, in mm.
const DEFAULT_COMPONENT_TO_EDGE_MM: f64 = 0.25;

/// Default minimum drill-hole to board-edge gap, in mm.
const DEFAULT_HOLE_TO_EDGE_MM: f64 = 0.3;

/// Default solder-mask expansion around a pad, per side and in mm.
const DEFAULT_SOLDER_MASK_EXPANSION_MM: f64 = 0.05;

/// Design-rule values the inspection checks read.
///
/// Every field carries a serde default, so documents written before the rules
/// existed reopen with the documented values instead of zeroed ones.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct DesignRules {
    /// Minimum gap between two drill holes of different nets, in millimetres.
    pub(crate) hole_clearance: f64,
    /// Minimum copper-to-board-edge gap, in millimetres.
    pub(crate) copper_to_edge: f64,
    /// Minimum component-to-board-edge gap, in millimetres.
    pub(crate) component_to_edge: f64,
    /// Minimum drill-hole-to-board-edge gap, in millimetres.
    pub(crate) hole_to_edge: f64,
    /// Solder-mask expansion around each pad, per side and in millimetres.
    pub(crate) solder_mask_expansion: f64,
}

impl Default for DesignRules {
    fn default() -> Self {
        Self {
            hole_clearance: DEFAULT_HOLE_CLEARANCE_MM,
            copper_to_edge: DEFAULT_COPPER_TO_EDGE_MM,
            component_to_edge: DEFAULT_COMPONENT_TO_EDGE_MM,
            hole_to_edge: DEFAULT_HOLE_TO_EDGE_MM,
            solder_mask_expansion: DEFAULT_SOLDER_MASK_EXPANSION_MM,
        }
    }
}

impl DesignRules {
    /// Whether every value matches [`DesignRules::default`].
    ///
    /// Default rules are omitted from the document so existing files keep their
    /// historical byte shape.
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// One editable board at a committed revision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Board {
    /// Board identity, stable across saves.
    pub(crate) id: String,
    /// Optional human title.
    pub(crate) title: Option<String>,
    /// Committed revision; every mutation increments it.
    pub(crate) revision: u64,
    /// Coordinate unit.
    pub(crate) units: Units,
    /// Optional rectangular outline; absent on documents written before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) outline: Option<Outline>,
    /// Design-rule values; absent on documents written before the rules existed.
    #[serde(default, skip_serializing_if = "DesignRules::is_default")]
    pub(crate) rules: DesignRules,
    /// Physical stackup; absent until a controlled-impedance stack is declared.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) stackup: Option<Stackup>,
    /// Stackup layers.
    pub(crate) layers: Vec<Layer>,
    /// Declared nets.
    pub(crate) nets: Vec<Net>,
    /// Placed footprints.
    pub(crate) footprints: Vec<Footprint>,
    /// Routed tracks.
    pub(crate) tracks: Vec<Track>,
    /// Placed vias.
    pub(crate) vias: Vec<Via>,
    /// Plated routed slots; absent on documents written before slots existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) slots: Vec<Slot>,
    /// Copper pour regions; absent on documents written before zones existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) zones: Vec<Zone>,
    /// Copper keepout regions; absent on documents written before them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) keepouts: Vec<Keepout>,
    /// Silkscreen text items; absent on documents written before them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) texts: Vec<TextItem>,
    /// Declared net classes; absent on documents written before classes existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) net_classes: Vec<NetClass>,
    /// Declared test points; absent on documents written before them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) test_points: Vec<TestPoint>,
    /// Declared differential pairs; absent on documents written before pairs existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) differential_pairs: Vec<DifferentialPair>,
    /// Severity overrides keyed by finding code; absent when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub(crate) severity_overrides: BTreeMap<String, String>,
    /// Waived findings; absent when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) waivers: Vec<Waiver>,
    /// Next differential pair sequence number.
    #[serde(default = "one")]
    pub(crate) next_pair: u64,
    /// Next waiver sequence number.
    #[serde(default = "one")]
    pub(crate) next_waiver: u64,
    /// Next footprint sequence number.
    pub(crate) next_footprint: u64,
    /// Next track sequence number.
    pub(crate) next_track: u64,
    /// Next via sequence number.
    pub(crate) next_via: u64,
    /// Next slot sequence number.
    #[serde(default = "one")]
    pub(crate) next_slot: u64,
    /// Next zone sequence number.
    #[serde(default = "one")]
    pub(crate) next_zone: u64,
    /// Next keepout sequence number.
    #[serde(default = "one")]
    pub(crate) next_keepout: u64,
    /// Next text item sequence number.
    #[serde(default = "one")]
    pub(crate) next_text: u64,
    /// Next net class sequence number.
    #[serde(default = "one")]
    pub(crate) next_net_class: u64,
    /// Next test point sequence number.
    #[serde(default = "one")]
    pub(crate) next_test_point: u64,
    /// Next net sequence number.
    pub(crate) next_net: u64,
}

/// Native document envelope written to `.opcb` files.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Document {
    /// Document tag, always [`DOCUMENT_FORMAT`].
    pub(crate) format: String,
    /// Schema version, always [`DOCUMENT_VERSION`].
    pub(crate) version: u32,
    /// The board itself.
    pub(crate) document: Board,
}

impl Board {
    /// Builds the board a `create` call starts from.
    pub(crate) fn new(id: String, title: Option<String>, template: Option<String>) -> Result<Self, ServiceError> {
        if let Some(template) = template.as_deref()
            && template != "blank"
        {
            return Err(ServiceError::new(
                "capability/not-implemented",
                format!("template `{template}` is not implemented; only `blank` starts a board"),
            ));
        }
        let title = match title {
            Some(value) if value.trim().is_empty() => {
                return Err(crate::argument("`title` must be a non-empty string"));
            }
            Some(value) => Some(value),
            None => None,
        };
        Ok(Self {
            id,
            title,
            revision: 1,
            units: Units::Mm,
            outline: None,
            rules: DesignRules::default(),
            stackup: None,
            layers: vec![
                layer("F.Cu", "F.Cu", LayerKind::Copper, "#c87533"),
                layer("B.Cu", "B.Cu", LayerKind::Copper, "#2f6fb0"),
                layer("F.SilkS", "F.SilkS", LayerKind::Silkscreen, "#e6e6e6"),
                layer("B.SilkS", "B.SilkS", LayerKind::Silkscreen, "#c4c4c4"),
                layer("Edge.Cuts", "Edge.Cuts", LayerKind::Mechanical, "#d9b44a"),
            ],
            nets: Vec::new(),
            footprints: Vec::new(),
            tracks: Vec::new(),
            vias: Vec::new(),
            slots: Vec::new(),
            zones: Vec::new(),
            keepouts: Vec::new(),
            texts: Vec::new(),
            net_classes: Vec::new(),
            test_points: Vec::new(),
            differential_pairs: Vec::new(),
            severity_overrides: BTreeMap::new(),
            waivers: Vec::new(),
            next_pair: 1,
            next_waiver: 1,
            next_footprint: 1,
            next_track: 1,
            next_via: 1,
            next_slot: 1,
            next_zone: 1,
            next_keepout: 1,
            next_text: 1,
            next_net_class: 1,
            next_test_point: 1,
            next_net: 1,
        })
    }

    /// Scale factor from document units to millimetres.
    pub(crate) fn millimetres(&self) -> f64 {
        match self.units {
            Units::Mm => 1.0,
            Units::Mil => 0.0254,
        }
    }

    /// Looks a layer up by id.
    pub(crate) fn layer(&self, id: &str) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    /// Looks a net up by name.
    pub(crate) fn net(&self, name: &str) -> Option<&Net> {
        self.nets.iter().find(|net| net.name == name)
    }

    /// Copper layers in stack order: `F.Cu` first, inner copper in declaration
    /// order, `B.Cu` last.
    pub(crate) fn copper_stack(&self) -> Vec<&str> {
        let mut stack = Vec::new();
        if self.layer("F.Cu").is_some_and(|layer| layer.kind == LayerKind::Copper) {
            stack.push("F.Cu");
        }
        for layer in &self.layers {
            if layer.kind == LayerKind::Copper && layer.id != "F.Cu" && layer.id != "B.Cu" {
                stack.push(layer.id.as_str());
            }
        }
        if self.layer("B.Cu").is_some_and(|layer| layer.kind == LayerKind::Copper) {
            stack.push("B.Cu");
        }
        stack
    }

    /// Inclusive index span of one via in [`Board::copper_stack`].
    ///
    /// A via without endpoints covers the whole stack. `None` means an endpoint
    /// is missing, is not a copper layer, or the span is reversed.
    pub(crate) fn via_span(&self, via: &Via) -> Option<(usize, usize)> {
        let stack = self.copper_stack();
        if stack.is_empty() {
            return None;
        }
        match (&via.from_layer, &via.to_layer) {
            (None, None) => Some((0, stack.len() - 1)),
            (Some(from), Some(to)) => {
                let low = stack.iter().position(|id| *id == from)?;
                let high = stack.iter().position(|id| *id == to)?;
                (low < high).then_some((low, high))
            }
            _ => None,
        }
    }

    /// Copper layers one via actually touches, as `from` and `to` ids.
    ///
    /// A through via reports the outer layers of the current stack.
    pub(crate) fn via_endpoints(&self, via: &Via) -> (Option<String>, Option<String>) {
        match (&via.from_layer, &via.to_layer) {
            (Some(from), Some(to)) => (Some(from.clone()), Some(to.clone())),
            _ => {
                let stack = self.copper_stack();
                match self.via_span(via) {
                    Some((low, high)) => {
                        (stack.get(low).map(|id| (*id).to_string()), stack.get(high).map(|id| (*id).to_string()))
                    }
                    None => (None, None),
                }
            }
        }
    }

    /// Whether one via's copper reaches a layer.
    pub(crate) fn via_covers(&self, via: &Via, layer: &str) -> bool {
        let Some((low, high)) = self.via_span(via)
        else {
            return false;
        };
        self.copper_stack().iter().position(|id| *id == layer).is_some_and(|index| low <= index && index <= high)
    }

    /// Classifies one via as `through`, `blind` or `buried` by its endpoints.
    pub(crate) fn via_kind(&self, via: &Via) -> &'static str {
        let stack = self.copper_stack();
        let (Some((low, high)), false) = (self.via_span(via), stack.is_empty())
        else {
            return "through";
        };
        let last = stack.len() - 1;
        if low == 0 && high == last {
            "through"
        }
        else if low == 0 || high == last {
            "blind"
        }
        else {
            "buried"
        }
    }

    /// Api projection of one via, with camelCase span fields.
    pub(crate) fn via_value(&self, via: &Via) -> Value {
        let (from, to) = self.via_endpoints(via);
        let mut value = json!({
            "id": via.id,
            "net": via.net,
            "at": via.at,
            "diameter": via.diameter,
            "drill": via.drill,
            "kind": self.via_kind(via),
        });
        if let (Some(from), Some(to)) = (from, to)
            && let Some(object) = value.as_object_mut()
        {
            object.insert("fromLayer".to_string(), Value::String(from));
            object.insert("toLayer".to_string(), Value::String(to));
        }
        if let Some(back) = &via.back_drill
            && let Some(object) = value.as_object_mut()
        {
            object.insert("backDrill".to_string(), json!({ "depth": back.depth, "stub": back.stub }));
        }
        value
    }

    /// Adds a footprint and returns the public object shape.
    pub(crate) fn add_footprint(&mut self, request: AddFootprintRequest) -> Result<Value, ServiceError> {
        let reference = request
            .reference
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| crate::argument("a footprint needs a non-empty `reference`"))?;
        if self.footprints.iter().any(|footprint| footprint.ref_ == reference) {
            return Err(crate::argument(format!("reference `{reference}` already exists on this board")));
        }
        let layer = request.layer.as_deref().unwrap_or("F.Cu");
        self.editable_layer(layer)?;
        let x = finite(request.x.unwrap_or(0.0), "x")?;
        let y = finite(request.y.unwrap_or(0.0), "y")?;
        let rotation = finite(request.rotation.unwrap_or(0.0), "rotation")?;
        let id = format!("fp-{}", self.next_footprint);
        self.next_footprint += 1;
        let footprint = Footprint {
            id: id.clone(),
            ref_: reference.to_string(),
            value: request.value.unwrap_or_default(),
            footprint: request.footprint.unwrap_or_default(),
            x,
            y,
            rotation,
            layer: layer.to_string(),
            pads: Vec::new(),
        };
        let object = footprint.to_value();
        self.footprints.push(footprint);
        self.revision += 1;
        Ok(object)
    }

    /// Removes one footprint by id and returns the removed id.
    pub(crate) fn delete_footprint(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .footprints
            .iter()
            .position(|footprint| footprint.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no footprint with id `{id}`")))?;
        self.editable_layer(&self.footprints[index].layer)?;
        let removed = self.footprints.remove(index);
        let prefix = format!("{}:", removed.id);
        self.test_points.retain(|point| !point.target.starts_with(&prefix));
        self.revision += 1;
        Ok(removed.id)
    }

    /// Moves one footprint and returns its public object shape.
    pub(crate) fn move_footprint(&mut self, request: MoveFootprintRequest) -> Result<Value, ServiceError> {
        let index = self
            .footprints
            .iter()
            .position(|footprint| footprint.id == request.id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no footprint with id `{}`", request.id)))?;
        self.editable_layer(&self.footprints[index].layer)?;
        if let Some(layer) = request.layer.as_deref() {
            self.editable_layer(layer)?;
        }
        if request.x.is_none() && request.y.is_none() && request.rotation.is_none() && request.layer.is_none() {
            return Err(crate::argument("a move needs `x`, `y`, `rotation` or `layer`"));
        }
        let footprint = &mut self.footprints[index];
        if let Some(x) = request.x {
            footprint.x = finite(x, "x")?;
        }
        if let Some(y) = request.y {
            footprint.y = finite(y, "y")?;
        }
        if let Some(rotation) = request.rotation {
            footprint.rotation = finite(rotation, "rotation")?;
        }
        if let Some(layer) = request.layer {
            footprint.layer = layer;
        }
        self.revision += 1;
        Ok(self.footprints[index].to_value())
    }

    /// Updates the value or package of one footprint.
    pub(crate) fn update_footprint(&mut self, request: UpdateFootprintRequest) -> Result<Value, ServiceError> {
        let index = self
            .footprints
            .iter()
            .position(|footprint| footprint.id == request.id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no footprint with id `{}`", request.id)))?;
        self.editable_layer(&self.footprints[index].layer)?;
        if request.value.is_none() && request.footprint.is_none() {
            return Err(crate::argument("an update needs `value` or `footprint`"));
        }
        let footprint = &mut self.footprints[index];
        if let Some(value) = request.value {
            footprint.value = value;
        }
        if let Some(package) = request.footprint {
            footprint.footprint = package;
        }
        self.revision += 1;
        Ok(self.footprints[index].to_value())
    }

    /// Adds one pad to a footprint and returns its public shape.
    pub(crate) fn add_pad(&mut self, request: AddPadRequest) -> Result<Value, ServiceError> {
        let index =
            self.footprints.iter().position(|footprint| footprint.id == request.footprint).ok_or_else(|| {
                ServiceError::new("object/not-found", format!("no footprint with id `{}`", request.footprint))
            })?;
        self.editable_layer(&self.footprints[index].layer)?;
        if request.number.trim().is_empty() {
            return Err(crate::argument("a pad needs a non-empty `number`"));
        }
        if self.footprints[index].pads.iter().any(|pad| pad.number == request.number) {
            return Err(ServiceError::new(
                "object/duplicate",
                format!("pad `{}` already exists on footprint `{}`", request.number, self.footprints[index].ref_),
            ));
        }
        let x = finite(request.x, "x")?;
        let y = finite(request.y, "y")?;
        let w = positive(request.w, "w")?;
        let h = positive(request.h, "h")?;
        let net = self.pad_net(request.net.as_deref())?;
        let pad = Pad { number: request.number, x, y, w, h, net };
        let object = json!(pad);
        self.footprints[index].pads.push(pad);
        self.revision += 1;
        Ok(object)
    }

    /// Updates the geometry or net of one pad and returns its public shape.
    pub(crate) fn update_pad(&mut self, request: UpdatePadRequest) -> Result<Value, ServiceError> {
        let footprint_index =
            self.footprints.iter().position(|footprint| footprint.id == request.footprint).ok_or_else(|| {
                ServiceError::new("object/not-found", format!("no footprint with id `{}`", request.footprint))
            })?;
        self.editable_layer(&self.footprints[footprint_index].layer)?;
        let pad_index =
            self.footprints[footprint_index].pads.iter().position(|pad| pad.number == request.number).ok_or_else(|| {
                ServiceError::new(
                    "object/not-found",
                    format!("no pad `{}` on footprint `{}`", request.number, self.footprints[footprint_index].ref_),
                )
            })?;
        if request.x.is_none() && request.y.is_none() && request.w.is_none() && request.h.is_none() && request.net.is_none() {
            return Err(crate::argument("a pad update needs `x`, `y`, `w`, `h` or `net`"));
        }
        let net = self.pad_net(request.net.as_deref())?;
        let pad = &mut self.footprints[footprint_index].pads[pad_index];
        if let Some(x) = request.x {
            pad.x = finite(x, "x")?;
        }
        if let Some(y) = request.y {
            pad.y = finite(y, "y")?;
        }
        if let Some(w) = request.w {
            pad.w = positive(w, "w")?;
        }
        if let Some(h) = request.h {
            pad.h = positive(h, "h")?;
        }
        if net.is_some() {
            pad.net = net;
        }
        self.revision += 1;
        Ok(json!(self.footprints[footprint_index].pads[pad_index].clone()))
    }

    /// Removes one pad from a footprint and returns the removed pad number.
    pub(crate) fn delete_pad(&mut self, request: DeletePadRequest) -> Result<String, ServiceError> {
        let footprint_index =
            self.footprints.iter().position(|footprint| footprint.id == request.footprint).ok_or_else(|| {
                ServiceError::new("object/not-found", format!("no footprint with id `{}`", request.footprint))
            })?;
        self.editable_layer(&self.footprints[footprint_index].layer)?;
        let pad_index =
            self.footprints[footprint_index].pads.iter().position(|pad| pad.number == request.number).ok_or_else(|| {
                ServiceError::new(
                    "object/not-found",
                    format!("no pad `{}` on footprint `{}`", request.number, self.footprints[footprint_index].ref_),
                )
            })?;
        let removed = self.footprints[footprint_index].pads.remove(pad_index);
        let target = format!("{}:{}", self.footprints[footprint_index].id, removed.number);
        self.test_points.retain(|point| point.target != target);
        self.revision += 1;
        Ok(removed.number)
    }

    /// Adds a track and returns the public object shape.
    pub(crate) fn add_track(&mut self, request: AddTrackRequest) -> Result<Value, ServiceError> {
        let layer = request.layer.trim();
        if layer.is_empty() {
            return Err(crate::argument("a track needs a non-empty `layer`"));
        }
        self.editable_layer(layer)?;
        let net = self.declared_net(request.net.as_deref())?;
        let width = positive(request.width, "width")?;
        let from = finite_pair(request.from, "from")?;
        let to = finite_pair(request.to, "to")?;
        let id = format!("track-{}", self.next_track);
        self.next_track += 1;
        let track = Track { id: id.clone(), net, layer: layer.to_string(), width, from, to, teardrop: false };
        let object = json!(track);
        self.tracks.push(track);
        self.revision += 1;
        Ok(object)
    }

    /// Updates the net, layer, width or endpoints of one track and returns its public shape.
    pub(crate) fn update_track(&mut self, request: UpdateTrackRequest) -> Result<Value, ServiceError> {
        let index = self
            .tracks
            .iter()
            .position(|track| track.id == request.id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no track with id `{}`", request.id)))?;
        self.editable_layer(&self.tracks[index].layer)?;
        let layer = match request.layer {
            Some(layer) => {
                let layer = layer.trim().to_string();
                if layer.is_empty() {
                    return Err(crate::argument("a track needs a non-empty `layer`"));
                }
                self.editable_layer(&layer)?;
                Some(layer)
            }
            None => None,
        };
        if request.net.is_none() && layer.is_none() && request.width.is_none() && request.from.is_none() && request.to.is_none()
        {
            return Err(crate::argument("a track update needs `net`, `layer`, `width`, `from` or `to`"));
        }
        let net = self.declared_net(request.net.as_deref())?;
        let width = request.width.map(|width| positive(width, "width")).transpose()?;
        let from = request.from.map(|from| finite_pair(from, "from")).transpose()?;
        let to = request.to.map(|to| finite_pair(to, "to")).transpose()?;
        let track = &mut self.tracks[index];
        if net.is_some() {
            track.net = net;
        }
        if let Some(layer) = layer {
            track.layer = layer;
        }
        if let Some(width) = width {
            track.width = width;
        }
        if let Some(from) = from {
            track.from = from;
        }
        if let Some(to) = to {
            track.to = to;
        }
        self.revision += 1;
        Ok(json!(self.tracks[index].clone()))
    }

    /// Adds linear teardrop copper at every pad-track junction.
    ///
    /// A teardrop is materialised as two ordinary copper tracks: from two
    /// points on the pad edge to a point on the track beyond the pad. Only
    /// same-net, same-layer junctions are considered, existing identical
    /// segments are skipped, and the request is rejected when it repeats no
    /// work. The `arc` shape stays unimplemented.
    pub(crate) fn add_teardrops(&mut self, request: AddTeardropsRequest) -> Result<Value, ServiceError> {
        let shape =
            request.shape.as_deref().map(str::trim).filter(|shape| !shape.is_empty()).unwrap_or("linear").to_ascii_lowercase();
        if shape != "linear" {
            return Err(ServiceError::new(
                "capability/not-implemented",
                format!("teardrop shape `{shape}` is not implemented; only `linear` is"),
            ));
        }
        if let Some(length) = request.length
            && (!length.is_finite() || length <= 0.0)
        {
            return Err(crate::argument("`length` must be a finite number greater than zero"));
        }
        let pads = self.placed_pads();
        let mut pending: Vec<Track> = Vec::new();
        let mut skipped = 0usize;
        for track in self.tracks.clone() {
            if track.teardrop {
                continue;
            }
            let Some(net) = track.net.clone()
            else {
                continue;
            };
            if self.layer(&track.layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
                continue;
            }
            let length = request.length.unwrap_or((track.width * 2.0).max(0.2));
            for (end, other) in [(track.from, track.to), (track.to, track.from)] {
                let Some(pad) = pads.iter().find(|pad| {
                    pad.net.as_deref() == Some(net.as_str())
                        && pad.layer == track.layer
                        && point_box_distance(end, pad.at, pad.half) <= 1e-6
                })
                else {
                    continue;
                };
                let mut direction = [end[0] - pad.at[0], end[1] - pad.at[1]];
                if direction[0].abs() + direction[1].abs() < 1e-12 {
                    direction = [end[0] - other[0], end[1] - other[1]];
                }
                let norm = (direction[0] * direction[0] + direction[1] * direction[1]).sqrt();
                if norm <= 1e-12 {
                    continue;
                }
                let unit = [direction[0] / norm, direction[1] / norm];
                let side = [-unit[1], unit[0]];
                let x_run = if unit[0].abs() > 1e-12 { pad.half[0] / unit[0].abs() } else { f64::INFINITY };
                let y_run = if unit[1].abs() > 1e-12 { pad.half[1] / unit[1].abs() } else { f64::INFINITY };
                let edge_run = x_run.min(y_run);
                if !edge_run.is_finite() {
                    continue;
                }
                let edge = [pad.at[0] + unit[0] * edge_run, pad.at[1] + unit[1] * edge_run];
                let tip = [edge[0] + unit[0] * length, edge[1] + unit[1] * length];
                let half_base = pad.half[0] * side[0].abs() + pad.half[1] * side[1].abs();
                for base in [
                    [edge[0] + side[0] * half_base, edge[1] + side[1] * half_base],
                    [edge[0] - side[0] * half_base, edge[1] - side[1] * half_base],
                ] {
                    let exists = |from: [f64; 2], to: [f64; 2]| {
                        let matches = |existing: &Track| {
                            existing.net.as_deref() == Some(net.as_str())
                                && existing.layer == track.layer
                                && ((same_point(existing.from, from) && same_point(existing.to, to))
                                    || (same_point(existing.from, to) && same_point(existing.to, from)))
                        };
                        self.tracks.iter().any(matches) || pending.iter().any(matches)
                    };
                    if exists(base, tip) {
                        skipped += 1;
                        continue;
                    }
                    pending.push(Track {
                        id: String::new(),
                        net: Some(net.clone()),
                        layer: track.layer.clone(),
                        width: track.width,
                        from: base,
                        to: tip,
                        teardrop: true,
                    });
                }
            }
        }
        let added = pending.len();
        if added == 0 {
            return Ok(json!({ "shape": "linear", "teardrops": 0, "tracks": 0, "skipped": skipped }));
        }
        for mut track in pending {
            track.id = format!("track-{}", self.next_track);
            self.next_track += 1;
            self.tracks.push(track);
        }
        self.revision += 1;
        Ok(json!({ "shape": "linear", "teardrops": added, "tracks": added, "skipped": skipped }))
    }

    /// Removes one track by id and returns the removed id.
    pub(crate) fn delete_track(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .tracks
            .iter()
            .position(|track| track.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no track with id `{id}`")))?;
        self.editable_layer(&self.tracks[index].layer)?;
        let removed = self.tracks.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Adds a via and returns the public object shape.
    ///
    /// Omitting both `fromLayer` and `toLayer` places a through via. Setting
    /// both places a blind or buried via; the span is validated against the
    /// declared copper stack before anything is written.
    pub(crate) fn add_via(&mut self, request: AddViaRequest) -> Result<Value, ServiceError> {
        let (from_layer, to_layer) = match (request.from_layer, request.to_layer) {
            (None, None) => (None, None),
            (Some(from), Some(to)) => {
                let from = from.trim().to_string();
                let to = to.trim().to_string();
                if from.is_empty() || to.is_empty() {
                    return Err(crate::argument("`fromLayer` and `toLayer` must not be empty"));
                }
                if from == to {
                    return Err(crate::argument("`fromLayer` and `toLayer` must name different copper layers"));
                }
                (Some(from), Some(to))
            }
            _ => return Err(crate::argument("a via span needs both `fromLayer` and `toLayer`, or neither")),
        };
        let stack = self.copper_stack();
        if stack.is_empty() {
            return Err(crate::argument("the board declares no copper layers"));
        }
        let (low, high) = match (&from_layer, &to_layer) {
            (Some(from), Some(to)) => {
                let low = stack
                    .iter()
                    .position(|id| *id == from)
                    .ok_or_else(|| crate::argument(format!("`{from}` is not a declared copper layer")))?;
                let high = stack
                    .iter()
                    .position(|id| *id == to)
                    .ok_or_else(|| crate::argument(format!("`{to}` is not a declared copper layer")))?;
                if low > high {
                    return Err(crate::argument("`fromLayer` must sit above `toLayer` in the copper stack"));
                }
                (low, high)
            }
            _ => (0, stack.len() - 1),
        };
        let span = if from_layer.is_none() && to_layer.is_none() {
            ["F.Cu", "B.Cu"].iter().map(|id| (*id).to_string()).collect::<Vec<_>>()
        }
        else {
            stack[low..=high].iter().map(|id| (*id).to_string()).collect::<Vec<_>>()
        };
        drop(stack);
        for id in &span {
            if self.layer(id).is_some_and(|layer| layer.locked) {
                return Err(ServiceError::new("board/layer-locked", format!("layer `{id}` is locked")));
            }
        }
        let net = self.declared_net(request.net.as_deref())?;
        let at = finite_pair(request.at, "at")?;
        let diameter = positive(request.diameter, "diameter")?;
        let drill = positive(request.drill, "drill")?;
        let id = format!("via-{}", self.next_via);
        self.next_via += 1;
        let via = Via { id, net, at, diameter, drill, from_layer, to_layer, back_drill: None };
        let object = self.via_value(&via);
        self.vias.push(via);
        self.revision += 1;
        Ok(object)
    }

    /// Updates the net, position, diameter or drill of one via and returns its public shape.
    pub(crate) fn update_via(&mut self, request: UpdateViaRequest) -> Result<Value, ServiceError> {
        let index = self
            .vias
            .iter()
            .position(|via| via.id == request.id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no via with id `{}`", request.id)))?;
        self.editable_via(&self.vias[index])?;
        if request.net.is_none() && request.at.is_none() && request.diameter.is_none() && request.drill.is_none() {
            return Err(crate::argument("a via update needs `net`, `at`, `diameter` or `drill`"));
        }
        let net = self.declared_net(request.net.as_deref())?;
        let at = request.at.map(|at| finite_pair(at, "at")).transpose()?;
        let diameter = request.diameter.map(|diameter| positive(diameter, "diameter")).transpose()?;
        let drill = request.drill.map(|drill| positive(drill, "drill")).transpose()?;
        let via = &mut self.vias[index];
        if net.is_some() {
            via.net = net;
        }
        if let Some(at) = at {
            via.at = at;
        }
        if let Some(diameter) = diameter {
            via.diameter = diameter;
        }
        if let Some(drill) = drill {
            via.drill = drill;
        }
        self.revision += 1;
        Ok(self.via_value(&self.vias[index]))
    }

    /// Generates a stitching via fence along the inset board outline.
    ///
    /// Vias are placed on one evenly spaced perimeter ring inset from the four
    /// rectangular edges. The ring is closed and deterministic: the segment count
    /// is `ceil(perimeter / spacing)` and every segment has the same length.
    /// Positions that already hold a via are skipped rather than duplicated. The
    /// generated vias are ordinary through vias on the requested net.
    pub(crate) fn stitch_vias(&mut self, request: AddStitchingRequest) -> Result<Value, ServiceError> {
        let net = request.net.trim();
        if net.is_empty() {
            return Err(crate::argument("via stitching needs a non-empty `net`"));
        }
        if self.net(net).is_none() {
            return Err(crate::argument(format!("net `{net}` is not declared on this board")));
        }
        let spacing = positive(request.spacing, "spacing")?;
        let inset = non_negative(request.inset.unwrap_or(0.5), "inset")?;
        let diameter = positive(request.diameter.unwrap_or(0.6), "diameter")?;
        let drill = positive(request.drill.unwrap_or(0.3), "drill")?;
        if drill >= diameter {
            return Err(crate::argument("`drill` must be smaller than `diameter`"));
        }
        let Some(outline) = self.outline
        else {
            return Err(crate::argument("via stitching needs a board outline; set one first"));
        };
        for layer in ["F.Cu", "B.Cu"] {
            if self.layer(layer).is_some_and(|entry| entry.locked) {
                return Err(ServiceError::new("board/layer-locked", format!("layer `{layer}` is locked")));
            }
        }
        let width = (outline.to[0] - outline.from[0]) - inset * 2.0;
        let height = (outline.to[1] - outline.from[1]) - inset * 2.0;
        if width <= 0.0 || height <= 0.0 {
            return Err(crate::argument("`inset` leaves no perimeter inside the outline"));
        }
        let min = [outline.from[0] + inset, outline.from[1] + inset];
        let max = [outline.to[0] - inset, outline.to[1] - inset];
        let perimeter = 2.0 * (width + height);
        let segments = (perimeter / spacing).ceil().max(1.0);
        if !segments.is_finite() || segments > 1_000_000.0 {
            return Err(crate::argument("`spacing` is too small for this outline"));
        }
        let count = segments as usize;
        let step = perimeter / segments;
        let mut pending: Vec<Via> = Vec::new();
        let mut skipped = 0usize;
        for index in 0..count {
            let at = perimeter_point(min, max, width, height, step * index as f64);
            if self.vias.iter().any(|via| same_point(via.at, at)) || pending.iter().any(|via| same_point(via.at, at)) {
                skipped += 1;
                continue;
            }
            pending.push(Via {
                id: format!("via-{}", self.next_via + pending.len() as u64),
                net: Some(net.to_string()),
                at,
                diameter,
                drill,
                from_layer: None,
                to_layer: None,
                back_drill: None,
            });
        }
        let added = pending.len();
        if added == 0 {
            return Ok(json!({ "net": net, "vias": 0, "skipped": skipped, "spacing": step, "inset": inset }));
        }
        self.next_via += added as u64;
        self.vias.extend(pending);
        self.revision += 1;
        Ok(json!({ "net": net, "vias": added, "skipped": skipped, "spacing": step, "inset": inset }))
    }

    /// Removes one via by id and returns the removed id.
    pub(crate) fn delete_via(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .vias
            .iter()
            .position(|via| via.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no via with id `{id}`")))?;
        let via = self.vias[index].clone();
        self.editable_via(&via)?;
        let removed = self.vias.remove(index);
        self.test_points.retain(|point| point.target != removed.id);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Adds one plated routed slot and returns its public object shape.
    ///
    /// A slot needs a non-zero width and two distinct endpoints, so it always
    /// describes a routed cut rather than a round hole. It spans every copper
    /// layer, so a locked copper layer rejects the edit.
    pub(crate) fn add_slot(&mut self, request: AddSlotRequest) -> Result<Value, ServiceError> {
        let net = self.declared_net(request.net.as_deref())?;
        let from = finite_pair(request.from, "from")?;
        let to = finite_pair(request.to, "to")?;
        let width = positive(request.width, "width")?;
        if (to[0] - from[0]).hypot(to[1] - from[1]) <= 1e-9 {
            return Err(crate::argument("a slot needs two distinct endpoints"));
        }
        for layer in self.layers.iter().filter(|layer| layer.kind == LayerKind::Copper) {
            if layer.locked {
                return Err(ServiceError::new("board/layer-locked", format!("layer `{}` is locked", layer.id)));
            }
        }
        let id = format!("slot-{}", self.next_slot);
        self.next_slot += 1;
        let slot = Slot { id, net, from, to, width };
        let object = slot.to_value();
        self.slots.push(slot);
        self.revision += 1;
        Ok(object)
    }

    /// Removes one slot by id and returns the removed id.
    pub(crate) fn delete_slot(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .slots
            .iter()
            .position(|slot| slot.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no slot with id `{id}`")))?;
        for layer in self.layers.iter().filter(|layer| layer.kind == LayerKind::Copper) {
            if layer.locked {
                return Err(ServiceError::new("board/layer-locked", format!("layer `{}` is locked", layer.id)));
            }
        }
        let removed = self.slots.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Sets or updates the back-drill depth and remaining stub of one through via.
    pub(crate) fn set_via_back_drill(&mut self, request: SetViaBackDrillRequest) -> Result<Value, ServiceError> {
        let index = self
            .vias
            .iter()
            .position(|via| via.id == request.id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no via with id `{}`", request.id)))?;
        let via = self.vias[index].clone();
        self.editable_via(&via)?;
        if request.depth.is_none() && request.stub.is_none() {
            return Err(crate::argument("a back drill needs `depth` or `stub`"));
        }
        if self.via_kind(&via) != "through" {
            return Err(crate::argument("back drilling is only defined on a through via"));
        }
        let (current_depth, current_stub) = via.back_drill.map(|back| (back.depth, back.stub)).unwrap_or((0.0, 0.0));
        let depth = match request.depth {
            Some(value) => finite(value, "depth")?,
            None => current_depth,
        };
        let stub = match request.stub {
            Some(value) => finite(value, "stub")?,
            None => current_stub,
        };
        if depth <= 0.0 {
            return Err(crate::argument("`depth` must be greater than zero"));
        }
        if stub < 0.0 {
            return Err(crate::argument("`stub` must not be negative"));
        }
        self.vias[index].back_drill = Some(BackDrill { depth, stub });
        self.revision += 1;
        Ok(self.via_value(&self.vias[index]))
    }

    /// Adds one stackup layer and returns its public object shape.
    pub(crate) fn add_layer(&mut self, request: AddLayerRequest) -> Result<Value, ServiceError> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(crate::argument("a layer needs a non-empty `name`"));
        }
        if self.layers.iter().any(|layer| layer.id == name || layer.name == name) {
            return Err(crate::argument(format!("layer `{name}` is already declared")));
        }
        let kind = parse_kind(request.kind.as_deref())?;
        let color = match request.color.as_deref().map(str::trim) {
            Some("") => return Err(crate::argument("a layer `color` must not be empty")),
            Some(color) => color.to_string(),
            None => default_color(kind).to_string(),
        };
        let layer = Layer { id: name.to_string(), name: name.to_string(), kind, color, visible: true, locked: false };
        let object = layer_value(&layer);
        self.layers.push(layer);
        self.revision += 1;
        Ok(object)
    }

    /// Renames or reconfigures one layer, updating every object that names it.
    pub(crate) fn update_layer(&mut self, request: UpdateLayerRequest) -> Result<Value, ServiceError> {
        let index = self
            .layers
            .iter()
            .position(|layer| layer.id == request.id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no layer with id `{}`", request.id)))?;
        if request.name.is_none()
            && request.color.is_none()
            && request.kind.is_none()
            && request.visible.is_none()
            && request.locked.is_none()
        {
            return Err(crate::argument("an update needs `name`, `kind`, `color`, `visible` or `locked`"));
        }

        if let Some(name) = request.name.as_deref() {
            let name = name.trim();
            if name.is_empty() {
                return Err(crate::argument("a layer `name` must not be empty"));
            }
            if name != self.layers[index].id {
                if self
                    .layers
                    .iter()
                    .enumerate()
                    .any(|(other, layer)| other != index && (layer.id == name || layer.name == name))
                {
                    return Err(crate::argument(format!("layer `{name}` is already declared")));
                }
                let previous = self.layers[index].id.clone();
                for footprint in &mut self.footprints {
                    if footprint.layer == previous {
                        footprint.layer = name.to_string();
                    }
                }
                for track in &mut self.tracks {
                    if track.layer == previous {
                        track.layer = name.to_string();
                    }
                }
                for via in &mut self.vias {
                    if via.from_layer.as_deref() == Some(previous.as_str()) {
                        via.from_layer = Some(name.to_string());
                    }
                    if via.to_layer.as_deref() == Some(previous.as_str()) {
                        via.to_layer = Some(name.to_string());
                    }
                }
                self.layers[index].id = name.to_string();
                self.layers[index].name = name.to_string();
            }
        }
        if let Some(kind) = request.kind.as_deref() {
            self.layers[index].kind = parse_kind(Some(kind))?;
        }
        if let Some(color) = request.color.as_deref() {
            let color = color.trim();
            if color.is_empty() {
                return Err(crate::argument("a layer `color` must not be empty"));
            }
            self.layers[index].color = color.to_string();
        }
        if let Some(visible) = request.visible {
            self.layers[index].visible = visible;
        }
        if let Some(locked) = request.locked {
            self.layers[index].locked = locked;
        }
        self.revision += 1;
        Ok(layer_value(&self.layers[index]))
    }

    /// Sets the rectangular board outline.
    pub(crate) fn set_outline(&mut self, request: SetOutlineRequest) -> Result<Value, ServiceError> {
        let from = finite_pair(request.from, "from")?;
        let to = finite_pair(request.to, "to")?;
        if to[0] <= from[0] {
            return Err(crate::argument("the outline width must be greater than zero"));
        }
        if to[1] <= from[1] {
            return Err(crate::argument("the outline height must be greater than zero"));
        }
        let outline = Outline { from, to };
        self.outline = Some(outline);
        self.revision += 1;
        Ok(json!(outline))
    }

    /// Sets the physical stackup used by controlled-impedance calculations.
    pub(crate) fn set_stackup(&mut self, request: SetStackupRequest) -> Result<Value, ServiceError> {
        let copper_thickness = positive(request.copper_thickness, "copperThickness")?;
        let dielectric_height = positive(request.dielectric_height, "dielectricHeight")?;
        let dielectric_constant = finite(request.dielectric_constant, "dielectricConstant")?;
        if dielectric_constant < 1.0 {
            return Err(crate::argument("`dielectricConstant` must be at least 1"));
        }
        let stackup = Stackup { copper_thickness, dielectric_height, dielectric_constant };
        self.stackup = Some(stackup);
        self.revision += 1;
        Ok(stackup.to_value())
    }

    /// Resolves stackup values for one impedance request.
    ///
    /// Explicit values override the stored stackup. When the board has no
    /// stackup, `dielectricHeight` and `dielectricConstant` are required;
    /// `copperThickness` falls back to 0.035 mm, the usual 1 oz foil.
    pub(crate) fn resolved_stackup(
        &self,
        copper_thickness: Option<f64>,
        dielectric_height: Option<f64>,
        dielectric_constant: Option<f64>,
    ) -> Result<Stackup, ServiceError> {
        let stored = self.stackup;
        let copper_thickness = copper_thickness.or(stored.map(|stackup| stackup.copper_thickness)).unwrap_or(0.035);
        let dielectric_height = dielectric_height.or(stored.map(|stackup| stackup.dielectric_height)).ok_or_else(|| {
            crate::argument("this board has no stackup; set one with `board.setStackup` or pass `dielectricHeight`")
        })?;
        let dielectric_constant =
            dielectric_constant.or(stored.map(|stackup| stackup.dielectric_constant)).ok_or_else(|| {
                crate::argument("this board has no stackup; set one with `board.setStackup` or pass `dielectricConstant`")
            })?;
        Ok(Stackup {
            copper_thickness: positive(copper_thickness, "copperThickness")?,
            dielectric_height: positive(dielectric_height, "dielectricHeight")?,
            dielectric_constant: {
                let value = finite(dielectric_constant, "dielectricConstant")?;
                if value < 1.0 {
                    return Err(crate::argument("`dielectricConstant` must be at least 1"));
                }
                value
            },
        })
    }

    /// Adds one copper pour region and returns its public object shape.
    ///
    /// A zone needs a copper layer, at least three vertices and a non-zero
    /// enclosed area. The writer emits it as one solid Gerber region.
    pub(crate) fn add_zone(&mut self, request: AddZoneRequest) -> Result<Value, ServiceError> {
        let layer = request.layer.trim();
        if layer.is_empty() {
            return Err(crate::argument("a zone needs a non-empty `layer`"));
        }
        self.editable_layer(layer)?;
        if self.layer(layer).is_none_or(|layer| layer.kind != LayerKind::Copper) {
            return Err(crate::argument(format!("`{layer}` is not a copper layer; zones need copper")));
        }
        let net = self.declared_net(request.net.as_deref())?;
        if request.points.len() < 3 {
            return Err(crate::argument("a zone needs at least three points"));
        }
        let points = request.points.into_iter().map(|point| finite_pair(point, "points")).collect::<Result<Vec<_>, _>>()?;
        if polygon_area(&points) <= 1e-9 {
            return Err(crate::argument("a zone polygon must enclose a non-zero area"));
        }
        let id = format!("zone-{}", self.next_zone);
        self.next_zone += 1;
        let zone = Zone { id, layer: layer.to_string(), net, points };
        let object = json!(zone);
        self.zones.push(zone);
        self.revision += 1;
        Ok(object)
    }

    /// Removes one copper pour region by id and returns the removed id.
    pub(crate) fn delete_zone(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .zones
            .iter()
            .position(|zone| zone.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no zone with id `{id}`")))?;
        self.editable_layer(&self.zones[index].layer)?;
        let removed = self.zones.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Adds one silkscreen text item and returns its public object shape.
    ///
    /// A text item needs non-empty content and a declared silkscreen layer.
    /// `size` defaults to 1.0 and `thickness` to 0.15, both in board units.
    pub(crate) fn add_text(&mut self, request: AddTextRequest) -> Result<Value, ServiceError> {
        if request.text.trim().is_empty() {
            return Err(crate::argument("a text item needs a non-empty `text`"));
        }
        let layer = request.layer.trim();
        if layer.is_empty() {
            return Err(crate::argument("a text item needs a non-empty `layer`"));
        }
        self.text_layer(layer)?;
        let at = finite_pair(request.at, "at")?;
        let rotation = finite(request.rotation.unwrap_or(0.0), "rotation")?;
        let size = positive(request.size.unwrap_or(1.0), "size")?;
        let thickness = positive(request.thickness.unwrap_or(0.15), "thickness")?;
        let id = format!("text-{}", self.next_text);
        self.next_text += 1;
        let item = TextItem { id, text: request.text, layer: layer.to_string(), at, rotation, size, thickness };
        let object = item.to_value();
        self.texts.push(item);
        self.revision += 1;
        Ok(object)
    }

    /// Updates the content, layer, position or metrics of one text item.
    pub(crate) fn update_text(&mut self, request: UpdateTextRequest) -> Result<Value, ServiceError> {
        let index = self
            .texts
            .iter()
            .position(|item| item.id == request.id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no text item with id `{}`", request.id)))?;
        self.editable_layer(&self.texts[index].layer)?;
        let layer = match request.layer {
            Some(layer) => {
                let layer = layer.trim().to_string();
                if layer.is_empty() {
                    return Err(crate::argument("a text item needs a non-empty `layer`"));
                }
                self.text_layer(&layer)?;
                Some(layer)
            }
            None => None,
        };
        if request.text.is_none()
            && layer.is_none()
            && request.at.is_none()
            && request.rotation.is_none()
            && request.size.is_none()
            && request.thickness.is_none()
        {
            return Err(crate::argument("a text update needs `text`, `layer`, `at`, `rotation`, `size` or `thickness`"));
        }
        if let Some(text) = request.text.as_deref()
            && text.trim().is_empty()
        {
            return Err(crate::argument("a text item needs a non-empty `text`"));
        }
        let at = request.at.map(|at| finite_pair(at, "at")).transpose()?;
        let rotation = request.rotation.map(|value| finite(value, "rotation")).transpose()?;
        let size = request.size.map(|value| positive(value, "size")).transpose()?;
        let thickness = request.thickness.map(|value| positive(value, "thickness")).transpose()?;
        let item = &mut self.texts[index];
        if let Some(text) = request.text {
            item.text = text;
        }
        if let Some(layer) = layer {
            item.layer = layer;
        }
        if let Some(at) = at {
            item.at = at;
        }
        if let Some(rotation) = rotation {
            item.rotation = rotation;
        }
        if let Some(size) = size {
            item.size = size;
        }
        if let Some(thickness) = thickness {
            item.thickness = thickness;
        }
        self.revision += 1;
        Ok(self.texts[index].to_value())
    }

    /// Removes one text item by id and returns the removed id.
    pub(crate) fn delete_text(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .texts
            .iter()
            .position(|item| item.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no text item with id `{id}`")))?;
        self.editable_layer(&self.texts[index].layer)?;
        let removed = self.texts.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Adds one copper keepout region and returns its public object shape.
    ///
    /// A keepout needs at least three vertices and a non-zero enclosed area.
    /// `layer` defaults to `*`, which rejects copper on every copper layer; a
    /// named layer must be copper.
    pub(crate) fn add_keepout(&mut self, request: AddKeepoutRequest) -> Result<Value, ServiceError> {
        let layer = request.layer.as_deref().map(str::trim).filter(|layer| !layer.is_empty()).unwrap_or("*");
        if layer != "*" {
            self.editable_layer(layer)?;
            if self.layer(layer).is_none_or(|entry| entry.kind != LayerKind::Copper) {
                return Err(crate::argument(format!("`{layer}` is not a copper layer; keepouts need copper")));
            }
        }
        if request.points.len() < 3 {
            return Err(crate::argument("a keepout needs at least three points"));
        }
        let points = request.points.into_iter().map(|point| finite_pair(point, "points")).collect::<Result<Vec<_>, _>>()?;
        if polygon_area(&points) <= 1e-9 {
            return Err(crate::argument("a keepout polygon must enclose a non-zero area"));
        }
        let id = format!("keepout-{}", self.next_keepout);
        self.next_keepout += 1;
        let keepout = Keepout { id, layer: layer.to_string(), points };
        let object = json!(keepout);
        self.keepouts.push(keepout);
        self.revision += 1;
        Ok(object)
    }

    /// Removes one keepout region by id and returns the removed id.
    pub(crate) fn delete_keepout(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .keepouts
            .iter()
            .position(|keepout| keepout.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no keepout with id `{id}`")))?;
        if self.keepouts[index].layer != "*" {
            self.editable_layer(&self.keepouts[index].layer)?;
        }
        let removed = self.keepouts.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Whether one keepout rejects copper on a layer.
    pub(crate) fn keepout_covers(&self, keepout: &Keepout, layer: &str) -> bool {
        keepout.layer == "*" || keepout.layer == layer
    }

    /// Declares one net class and returns its public object shape.
    ///
    /// The class starts with no nets; assign them with
    /// [`Board::assign_net_class`]. Every provided target must be positive.
    pub(crate) fn add_net_class(&mut self, request: AddNetClassRequest) -> Result<Value, ServiceError> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(crate::argument("a net class needs a non-empty `name`"));
        }
        if self.net_classes.iter().any(|class| class.name == name) {
            return Err(crate::argument(format!("net class `{name}` is already declared")));
        }
        let width = request.width.map(|value| positive(value, "width")).transpose()?;
        let clearance = request.clearance.map(|value| positive(value, "clearance")).transpose()?;
        let via_diameter = request.via_diameter.map(|value| positive(value, "viaDiameter")).transpose()?;
        let via_drill = request.via_drill.map(|value| positive(value, "viaDrill")).transpose()?;
        let id = format!("netclass-{}", self.next_net_class);
        self.next_net_class += 1;
        let class = NetClass { id, name: name.to_string(), width, clearance, via_diameter, via_drill, nets: Vec::new() };
        let object = class.to_value();
        self.net_classes.push(class);
        self.revision += 1;
        Ok(object)
    }

    /// Removes one net class by id and returns the removed id.
    pub(crate) fn delete_net_class(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .net_classes
            .iter()
            .position(|class| class.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no net class with id `{id}`")))?;
        let removed = self.net_classes.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Assigns one declared net to a net class by id or name.
    ///
    /// A net belongs to at most one class, so assigning it again moves it.
    /// Returns the updated class.
    pub(crate) fn assign_net_class(&mut self, request: AssignNetClassRequest) -> Result<Value, ServiceError> {
        let net = request.net.trim();
        if net.is_empty() {
            return Err(crate::argument("a net class assignment needs a non-empty `net`"));
        }
        if self.net(net).is_none() {
            return Err(ServiceError::new("object/not-found", format!("net `{net}` is not declared on this board")));
        }
        let class = request.class.trim();
        let index = self
            .net_classes
            .iter()
            .position(|entry| entry.id == class || entry.name == class)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no net class with id or name `{class}`")))?;
        for entry in &mut self.net_classes {
            entry.nets.retain(|name| name != net);
        }
        if !self.net_classes[index].nets.iter().any(|name| name == net) {
            self.net_classes[index].nets.push(net.to_string());
        }
        self.revision += 1;
        Ok(self.net_classes[index].to_value())
    }

    /// Returns the class one net belongs to, when it is assigned.
    pub(crate) fn class_of_net(&self, net: &str) -> Option<&NetClass> {
        self.net_classes.iter().find(|class| class.nets.iter().any(|name| name == net))
    }

    /// Declares one test point on an existing pad or via.
    ///
    /// The target is a pad key `footprint-id:pad-number` or a via id. A target
    /// can be declared only once, and the object must already exist.
    pub(crate) fn add_test_point(&mut self, request: AddTestPointRequest) -> Result<Value, ServiceError> {
        let target = request.target.trim();
        if target.is_empty() {
            return Err(crate::argument("a test point needs a non-empty `target`"));
        }
        if self.test_points.iter().any(|point| point.target == target) {
            return Err(crate::argument(format!("test point `{target}` is already declared")));
        }
        let resolved = self.resolve_test_point(target).ok_or_else(|| {
            ServiceError::new(
                "object/not-found",
                format!("no pad key `footprint-id:pad-number` or via id `{target}` exists on this board"),
            )
        })?;
        let id = format!("testpoint-{}", self.next_test_point);
        self.next_test_point += 1;
        let object = json!({
            "id": id,
            "target": resolved.target,
            "kind": resolved.kind,
            "designator": resolved.designator,
            "pad": resolved.pad,
            "net": resolved.net,
            "layer": resolved.layer,
            "at": resolved.at,
        });
        self.test_points.push(TestPoint { id, target: target.to_string() });
        self.revision += 1;
        Ok(object)
    }

    /// Removes one declared test point by id and returns the removed id.
    pub(crate) fn delete_test_point(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .test_points
            .iter()
            .position(|point| point.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no test point with id `{id}`")))?;
        let removed = self.test_points.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Resolves every declared test point to its current probe position.
    pub(crate) fn resolved_test_points(&self) -> Vec<ResolvedTestPoint> {
        self.test_points
            .iter()
            .filter_map(|point| {
                self.resolve_test_point(&point.target).map(|mut resolved| {
                    resolved.id = point.id.clone();
                    resolved
                })
            })
            .collect()
    }

    /// Resolves one target to its current probe position.
    pub(crate) fn resolve_test_point(&self, target: &str) -> Option<ResolvedTestPoint> {
        if let Some((footprint_id, pad_number)) = target.split_once(':') {
            let footprint = self.footprints.iter().find(|footprint| footprint.id == footprint_id)?;
            let pad = footprint.pads.iter().find(|pad| pad.number == pad_number)?;
            let placed = self.placed_pads().into_iter().find(|pad| pad.key == target)?;
            return Some(ResolvedTestPoint {
                id: String::new(),
                target: target.to_string(),
                designator: footprint.ref_.clone(),
                pad: pad.number.clone(),
                kind: "pad",
                net: pad.net.clone(),
                layer: footprint.layer.clone(),
                at: placed.at,
            });
        }
        let via = self.vias.iter().find(|via| via.id == target)?;
        Some(ResolvedTestPoint {
            id: String::new(),
            target: target.to_string(),
            designator: String::new(),
            pad: String::new(),
            kind: "via",
            net: via.net.clone(),
            layer: match (&via.from_layer, &via.to_layer) {
                (Some(from), Some(to)) => format!("{from}..{to}"),
                _ => "F.Cu..B.Cu".to_string(),
            },
            at: via.at,
        })
    }

    /// Replaces one or more design-rule values.
    ///
    /// Every value is in millimetres. The request must name at least one rule;
    /// a rejected request leaves the board and its revision untouched.
    pub(crate) fn set_rules(&mut self, request: SetRulesRequest) -> Result<Value, ServiceError> {
        if request.hole_clearance.is_none()
            && request.copper_to_edge.is_none()
            && request.component_to_edge.is_none()
            && request.hole_to_edge.is_none()
            && request.solder_mask_expansion.is_none()
        {
            return Err(crate::argument("a rules update needs at least one design rule value"));
        }
        let mut rules = self.rules;
        if let Some(value) = request.hole_clearance {
            rules.hole_clearance = non_negative(value, "holeClearance")?;
        }
        if let Some(value) = request.copper_to_edge {
            rules.copper_to_edge = non_negative(value, "copperToEdge")?;
        }
        if let Some(value) = request.component_to_edge {
            rules.component_to_edge = non_negative(value, "componentToEdge")?;
        }
        if let Some(value) = request.hole_to_edge {
            rules.hole_to_edge = non_negative(value, "holeToEdge")?;
        }
        if let Some(value) = request.solder_mask_expansion {
            rules.solder_mask_expansion = non_negative(value, "solderMaskExpansion")?;
        }
        self.rules = rules;
        self.revision += 1;
        Ok(json!(rules))
    }

    /// Declares a net and returns the public object shape.
    pub(crate) fn add_net(&mut self, request: AddNetRequest) -> Result<Value, ServiceError> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(crate::argument("a net needs a non-empty `name`"));
        }
        if self.nets.iter().any(|net| net.name == name) {
            return Err(crate::argument(format!("net `{name}` is already declared")));
        }
        let id = format!("net-{}", self.next_net);
        self.next_net += 1;
        let net = Net { id: id.clone(), name: name.to_string() };
        let object = json!(net);
        self.nets.push(net);
        self.revision += 1;
        Ok(object)
    }

    /// Declares one differential pair and returns its public object shape.
    pub(crate) fn add_differential_pair(&mut self, request: AddDifferentialPairRequest) -> Result<Value, ServiceError> {
        let positive_net = request.positive.trim();
        let negative_net = request.negative.trim();
        if positive_net.is_empty() || negative_net.is_empty() {
            return Err(crate::argument("a differential pair needs two non-empty net names"));
        }
        if positive_net == negative_net {
            return Err(crate::argument("a differential pair needs two different nets"));
        }
        if self.net(positive_net).is_none() {
            return Err(crate::argument(format!("net `{positive_net}` is not declared on this board")));
        }
        if self.net(negative_net).is_none() {
            return Err(crate::argument(format!("net `{negative_net}` is not declared on this board")));
        }
        if self.differential_pairs.iter().any(|pair| pair.positive == positive_net && pair.negative == negative_net) {
            return Err(ServiceError::new(
                "object/duplicate",
                format!("differential pair `{positive_net}`/`{negative_net}` is already declared"),
            ));
        }
        let width = positive(request.width, "width")?;
        let gap = positive(request.gap, "gap")?;
        let max_skew = finite(request.max_skew, "maxSkew")?;
        if max_skew < 0.0 {
            return Err(crate::argument("`maxSkew` must not be negative"));
        }
        let id = format!("dp-{}", self.next_pair);
        self.next_pair += 1;
        let pair = DifferentialPair {
            id,
            positive: positive_net.to_string(),
            negative: negative_net.to_string(),
            width,
            gap,
            max_skew,
        };
        let object = pair.to_value();
        self.differential_pairs.push(pair);
        self.revision += 1;
        Ok(object)
    }

    /// Removes one differential pair by id and returns the removed id.
    pub(crate) fn delete_differential_pair(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .differential_pairs
            .iter()
            .position(|pair| pair.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no differential pair with id `{id}`")))?;
        let removed = self.differential_pairs.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Declares one waiver for a finding code and returns its public shape.
    ///
    /// A waiver with an `object` only silences findings on that object; without
    /// one it silences every finding with the code. The `(code, object)` pair
    /// must be unique.
    pub(crate) fn add_waiver(&mut self, request: AddWaiverRequest) -> Result<Value, ServiceError> {
        let code = request.code.trim();
        if code.is_empty() {
            return Err(crate::argument("a waiver needs a non-empty `code`"));
        }
        let object = match request.object.as_deref().map(str::trim) {
            Some("") => return Err(crate::argument("a waiver `object` must not be empty")),
            Some(object) => Some(object.to_string()),
            None => None,
        };
        if self.waivers.iter().any(|waiver| waiver.code == code && waiver.object == object) {
            let scope = object.as_deref().map_or("every object".to_string(), |object| format!("object `{object}`"));
            return Err(ServiceError::new("object/duplicate", format!("a waiver for `{code}` on {scope} already exists")));
        }
        let reason = request.reason.map(|reason| reason.trim().to_string()).filter(|reason| !reason.is_empty());
        let id = format!("waiver-{}", self.next_waiver);
        self.next_waiver += 1;
        let waiver = Waiver { id, code: code.to_string(), object, reason };
        let object_value = waiver.to_value();
        self.waivers.push(waiver);
        self.revision += 1;
        Ok(object_value)
    }

    /// Removes one waiver by id and returns the removed id.
    pub(crate) fn delete_waiver(&mut self, id: &str) -> Result<String, ServiceError> {
        let index = self
            .waivers
            .iter()
            .position(|waiver| waiver.id == id)
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no waiver with id `{id}`")))?;
        let removed = self.waivers.remove(index);
        self.revision += 1;
        Ok(removed.id)
    }

    /// Overrides the severity of one finding code.
    pub(crate) fn set_severity(&mut self, request: SetSeverityRequest) -> Result<Value, ServiceError> {
        let code = request.code.trim();
        if code.is_empty() {
            return Err(crate::argument("a severity override needs a non-empty `code`"));
        }
        let severity = request.severity.trim().to_ascii_lowercase();
        if !matches!(severity.as_str(), "info" | "warning" | "error") {
            return Err(crate::argument("`severity` must be info, warning or error"));
        }
        self.severity_overrides.insert(code.to_string(), severity.clone());
        self.revision += 1;
        Ok(json!({ "code": code, "severity": severity }))
    }

    /// Removes one severity override and returns its previous value.
    pub(crate) fn delete_severity(&mut self, code: &str) -> Result<Value, ServiceError> {
        let removed = self
            .severity_overrides
            .remove(code.trim())
            .ok_or_else(|| ServiceError::new("object/not-found", format!("no severity override for `{code}`")))?;
        self.revision += 1;
        Ok(json!({ "code": code.trim(), "severity": removed }))
    }

    /// Resolves every pad to board coordinates.
    ///
    /// Footprint rotation is applied once here so router and inspection code
    /// share one pad geometry model.
    pub(crate) fn placed_pads(&self) -> Vec<PlacedPad<'_>> {
        let mut pads = Vec::new();
        for footprint in &self.footprints {
            let angle = footprint.rotation.to_radians();
            let (sin, cos) = (angle.sin(), angle.cos());
            for pad in &footprint.pads {
                pads.push(PlacedPad {
                    key: format!("{}:{}", footprint.id, pad.number),
                    net: pad.net.clone(),
                    at: [footprint.x + pad.x * cos - pad.y * sin, footprint.y + pad.x * sin + pad.y * cos],
                    half: [(cos.abs() * pad.w + sin.abs() * pad.h) / 2.0, (sin.abs() * pad.w + cos.abs() * pad.h) / 2.0],
                    layer: &footprint.layer,
                });
            }
        }
        pads
    }

    /// Rejects a missing or locked layer before an edit touches it.
    fn editable_layer(&self, id: &str) -> Result<(), ServiceError> {
        let layer = self.layer(id).ok_or_else(|| crate::argument(format!("layer `{id}` is not declared on this board")))?;
        if layer.locked {
            return Err(ServiceError::new("board/layer-locked", format!("layer `{id}` is locked")));
        }
        Ok(())
    }

    /// Rejects a missing or non-silkscreen text layer before an edit uses it.
    fn text_layer(&self, id: &str) -> Result<(), ServiceError> {
        self.editable_layer(id)?;
        if self.layer(id).is_none_or(|layer| layer.kind != LayerKind::Silkscreen) {
            return Err(crate::argument(format!("`{id}` is not a silkscreen layer; text needs a legend layer")));
        }
        Ok(())
    }

    /// Rejects a via edit when a layer its span touches is locked.
    ///
    /// A through via keeps the historical check on the two outer layers; a
    /// blind or buried via checks every layer between its endpoints.
    fn editable_via(&self, via: &Via) -> Result<(), ServiceError> {
        let ids = if via.from_layer.is_none() && via.to_layer.is_none() {
            vec!["F.Cu".to_string(), "B.Cu".to_string()]
        }
        else {
            match self.via_span(via) {
                Some((low, high)) => self.copper_stack()[low..=high].iter().map(|id| (*id).to_string()).collect(),
                None => Vec::new(),
            }
        };
        for id in ids {
            if self.layer(&id).is_some_and(|layer| layer.locked) {
                return Err(ServiceError::new("board/layer-locked", format!("layer `{id}` is locked")));
            }
        }
        Ok(())
    }

    /// Rejects a pad net that the board does not declare, or returns it.
    fn pad_net(&self, name: Option<&str>) -> Result<Option<String>, ServiceError> {
        match name {
            None => Ok(None),
            Some(name) if name.trim().is_empty() => Err(crate::argument("a net name must not be empty")),
            Some(name) => {
                if self.net(name).is_none() {
                    return Err(ServiceError::new("object/not-found", format!("net `{name}` is not declared on this board")));
                }
                Ok(Some(name.to_string()))
            }
        }
    }

    /// Rejects an undeclared net name, or returns the normalised name.
    fn declared_net(&self, name: Option<&str>) -> Result<Option<String>, ServiceError> {
        match name {
            None => Ok(None),
            Some(name) if name.trim().is_empty() => Err(crate::argument("a net name must not be empty")),
            Some(name) => {
                if self.net(name).is_none() {
                    return Err(crate::argument(format!("net `{name}` is not declared on this board")));
                }
                Ok(Some(name.to_string()))
            }
        }
    }
}

impl Document {
    /// Wraps a board into the native document envelope.
    pub(crate) fn new(board: Board) -> Self {
        Self { format: DOCUMENT_FORMAT.to_string(), version: DOCUMENT_VERSION, document: board }
    }
}

/// Serde default for counters added after the first schema version.
fn one() -> u64 {
    1
}

fn layer(id: &str, name: &str, kind: LayerKind, color: &str) -> Layer {
    Layer { id: id.to_string(), name: name.to_string(), kind, color: color.to_string(), visible: true, locked: false }
}

fn layer_value(layer: &Layer) -> Value {
    json!({
        "id": layer.id,
        "name": layer.name,
        "kind": layer.kind,
        "color": layer.color,
        "visible": layer.visible,
        "locked": layer.locked,
    })
}

fn parse_kind(raw: Option<&str>) -> Result<LayerKind, ServiceError> {
    match raw.map(str::trim).map(str::to_lowercase).as_deref() {
        None | Some("") | Some("copper") => Ok(LayerKind::Copper),
        Some("silkscreen") => Ok(LayerKind::Silkscreen),
        Some("soldermask") => Ok(LayerKind::Soldermask),
        Some("assembly") => Ok(LayerKind::Assembly),
        Some("mechanical") => Ok(LayerKind::Mechanical),
        Some(other) => Err(crate::argument(format!(
            "`{other}` is not a layer kind; use copper, silkscreen, soldermask, assembly or mechanical"
        ))),
    }
}

fn default_color(kind: LayerKind) -> &'static str {
    match kind {
        LayerKind::Copper => "#c87533",
        LayerKind::Silkscreen => "#e6e6e6",
        LayerKind::Soldermask => "#2f6fb0",
        LayerKind::Assembly => "#9b7bd4",
        LayerKind::Mechanical => "#d9b44a",
    }
}

/// Signed area of a polygon in board units, using the shoelace formula.
pub(crate) fn polygon_area(points: &[[f64; 2]]) -> f64 {
    let mut sum = 0.0;
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        sum += point[0] * next[1] - next[0] * point[1];
    }
    (sum / 2.0).abs()
}

/// Distance from a point to an axis-aligned box; zero inside the box.
fn point_box_distance(point: [f64; 2], centre: [f64; 2], half: [f64; 2]) -> f64 {
    let dx = ((point[0] - centre[0]).abs() - half[0]).max(0.0);
    let dy = ((point[1] - centre[1]).abs() - half[1]).max(0.0);
    (dx * dx + dy * dy).sqrt()
}

/// Whether two ordered points name the same location.
fn same_point(left: [f64; 2], right: [f64; 2]) -> bool {
    (left[0] - right[0]).abs() <= 1e-9 && (left[1] - right[1]).abs() <= 1e-9
}

/// One point at a distance along the inset rectangle, walking its perimeter
/// clockwise from the lower-left corner.
fn perimeter_point(min: [f64; 2], max: [f64; 2], width: f64, height: f64, distance: f64) -> [f64; 2] {
    if distance <= width {
        [min[0] + distance, min[1]]
    }
    else if distance <= width + height {
        [max[0], min[1] + (distance - width)]
    }
    else if distance <= 2.0 * width + height {
        [max[0] - (distance - width - height), max[1]]
    }
    else {
        [min[0], max[1] - (distance - 2.0 * width - height)]
    }
}

fn finite(value: f64, field: &str) -> Result<f64, ServiceError> {
    if value.is_finite() { Ok(value) } else { Err(crate::argument(format!("`{field}` must be a finite number"))) }
}

fn finite_pair(value: [f64; 2], field: &str) -> Result<[f64; 2], ServiceError> {
    Ok([finite(value[0], field)?, finite(value[1], field)?])
}

fn positive(value: f64, field: &str) -> Result<f64, ServiceError> {
    let value = finite(value, field)?;
    if value > 0.0 { Ok(value) } else { Err(crate::argument(format!("`{field}` must be greater than zero"))) }
}

fn non_negative(value: f64, field: &str) -> Result<f64, ServiceError> {
    let value = finite(value, field)?;
    if value < 0.0 { Err(crate::argument(format!("`{field}` must not be negative"))) } else { Ok(value) }
}

/// Validates a board loaded from disk before it receives a handle.
///
/// Failures use `format/invalid`; a version mismatch is reported by the caller
/// as `format/unsupported-version`.
pub(crate) fn validate(board: &Board) -> Result<(), ServiceError> {
    if board.id.trim().is_empty() {
        return Err(crate::invalid("the board id is empty"));
    }
    for (value, field) in [
        (board.rules.hole_clearance, "holeClearance"),
        (board.rules.copper_to_edge, "copperToEdge"),
        (board.rules.component_to_edge, "componentToEdge"),
        (board.rules.hole_to_edge, "holeToEdge"),
        (board.rules.solder_mask_expansion, "solderMaskExpansion"),
    ] {
        checks(value, &format!("design rule `{field}`"))?;
        if value < 0.0 {
            return Err(crate::invalid(format!("design rule `{field}` must not be negative")));
        }
    }
    if let Some(stackup) = &board.stackup {
        sizes(stackup.copper_thickness, "stackup copperThickness")?;
        sizes(stackup.dielectric_height, "stackup dielectricHeight")?;
        checks(stackup.dielectric_constant, "stackup dielectricConstant")?;
        if stackup.dielectric_constant < 1.0 {
            return Err(crate::invalid("stackup dielectricConstant must be at least 1"));
        }
    }
    if let Some(outline) = &board.outline {
        checks(outline.from[0], "outline from.x")?;
        checks(outline.from[1], "outline from.y")?;
        checks(outline.to[0], "outline to.x")?;
        checks(outline.to[1], "outline to.y")?;
        if outline.to[0] <= outline.from[0] || outline.to[1] <= outline.from[1] {
            return Err(crate::invalid("the outline must have a positive width and height"));
        }
    }
    let mut layer_ids = HashSet::new();
    for layer in &board.layers {
        if layer.id.trim().is_empty() {
            return Err(crate::invalid("a layer has an empty id"));
        }
        if !layer_ids.insert(layer.id.as_str()) {
            return Err(crate::invalid(format!("layer id `{}` is duplicated", layer.id)));
        }
        if layer.color.trim().is_empty() {
            return Err(crate::invalid(format!("layer `{}` has an empty color", layer.id)));
        }
    }
    let mut net_ids = HashSet::new();
    let mut net_names = HashSet::new();
    for net in &board.nets {
        if net.id.trim().is_empty() {
            return Err(crate::invalid("a net has an empty id"));
        }
        if net.name.trim().is_empty() {
            return Err(crate::invalid(format!("net `{}` has an empty name", net.id)));
        }
        if !net_ids.insert(net.id.as_str()) {
            return Err(crate::invalid(format!("net id `{}` is duplicated", net.id)));
        }
        if !net_names.insert(net.name.as_str()) {
            return Err(crate::invalid(format!("net name `{}` is duplicated", net.name)));
        }
    }

    let mut footprint_ids = HashSet::new();
    let mut references = HashSet::new();
    for footprint in &board.footprints {
        if footprint.id.trim().is_empty() {
            return Err(crate::invalid("a footprint has an empty id"));
        }
        if !footprint_ids.insert(footprint.id.as_str()) {
            return Err(crate::invalid(format!("footprint id `{}` is duplicated", footprint.id)));
        }
        if footprint.ref_.trim().is_empty() {
            return Err(crate::invalid(format!("footprint `{}` has an empty reference", footprint.id)));
        }
        if !references.insert(footprint.ref_.as_str()) {
            return Err(crate::invalid(format!("reference `{}` is duplicated", footprint.ref_)));
        }
        if !layer_ids.contains(footprint.layer.as_str()) {
            return Err(crate::invalid(format!(
                "footprint `{}` references missing layer `{}`",
                footprint.ref_, footprint.layer
            )));
        }
        checks(footprint.x, &format!("footprint `{}` x", footprint.ref_))?;
        checks(footprint.y, &format!("footprint `{}` y", footprint.ref_))?;
        checks(footprint.rotation, &format!("footprint `{}` rotation", footprint.ref_))?;
        let mut pad_numbers = HashSet::new();
        for pad in &footprint.pads {
            if pad.number.trim().is_empty() {
                return Err(crate::invalid(format!("a pad of `{}` has an empty number", footprint.ref_)));
            }
            if !pad_numbers.insert(pad.number.as_str()) {
                return Err(crate::invalid(format!("pad number `{}` is duplicated on `{}`", pad.number, footprint.ref_)));
            }
            checks(pad.x, &format!("pad `{}` of `{}` x", pad.number, footprint.ref_))?;
            checks(pad.y, &format!("pad `{}` of `{}` y", pad.number, footprint.ref_))?;
            sizes(pad.w, &format!("pad `{}` of `{}` width", pad.number, footprint.ref_))?;
            sizes(pad.h, &format!("pad `{}` of `{}` height", pad.number, footprint.ref_))?;
            if let Some(net) = pad.net.as_deref()
                && !net_names.contains(net)
            {
                return Err(crate::invalid(format!(
                    "pad `{}` of `{}` references undeclared net `{net}`",
                    pad.number, footprint.ref_
                )));
            }
        }
    }

    let mut track_ids = HashSet::new();
    for track in &board.tracks {
        if track.id.trim().is_empty() {
            return Err(crate::invalid("a track has an empty id"));
        }
        if !track_ids.insert(track.id.as_str()) {
            return Err(crate::invalid(format!("track id `{}` is duplicated", track.id)));
        }
        if !layer_ids.contains(track.layer.as_str()) {
            return Err(crate::invalid(format!("track `{}` references missing layer `{}`", track.id, track.layer)));
        }
        sizes(track.width, &format!("track `{}` width", track.id))?;
        checks(track.from[0], &format!("track `{}` from.x", track.id))?;
        checks(track.from[1], &format!("track `{}` from.y", track.id))?;
        checks(track.to[0], &format!("track `{}` to.x", track.id))?;
        checks(track.to[1], &format!("track `{}` to.y", track.id))?;
        if let Some(net) = track.net.as_deref()
            && !net_names.contains(net)
        {
            return Err(crate::invalid(format!("track `{}` references undeclared net `{net}`", track.id)));
        }
    }

    let mut via_ids = HashSet::new();
    for via in &board.vias {
        if via.id.trim().is_empty() {
            return Err(crate::invalid("a via has an empty id"));
        }
        if !via_ids.insert(via.id.as_str()) {
            return Err(crate::invalid(format!("via id `{}` is duplicated", via.id)));
        }
        if via.from_layer.is_some() != via.to_layer.is_some() {
            return Err(crate::invalid(format!("via `{}` declares only one end of its span", via.id)));
        }
        if let (Some(from), Some(to)) = (&via.from_layer, &via.to_layer) {
            if from == to {
                return Err(crate::invalid(format!("via `{}` needs two different copper layers", via.id)));
            }
            let stack = board.copper_stack();
            let ordered =
                match (stack.iter().position(|id| *id == from.as_str()), stack.iter().position(|id| *id == to.as_str())) {
                    (Some(low), Some(high)) => low < high,
                    _ => false,
                };
            if !ordered {
                return Err(crate::invalid(format!(
                    "via `{}` does not span two declared copper layers in stack order",
                    via.id
                )));
            }
        }
        checks(via.at[0], &format!("via `{}` at.x", via.id))?;
        checks(via.at[1], &format!("via `{}` at.y", via.id))?;
        sizes(via.diameter, &format!("via `{}` diameter", via.id))?;
        sizes(via.drill, &format!("via `{}` drill", via.id))?;
        if let Some(back) = &via.back_drill {
            checks(back.depth, &format!("via `{}` back-drill depth", via.id))?;
            checks(back.stub, &format!("via `{}` back-drill stub", via.id))?;
            if back.depth <= 0.0 {
                return Err(crate::invalid(format!("via `{}` has a non-positive back-drill depth", via.id)));
            }
            if back.stub < 0.0 {
                return Err(crate::invalid(format!("via `{}` has a negative back-drill stub", via.id)));
            }
            if board.via_kind(via) != "through" {
                return Err(crate::invalid(format!("via `{}` is not a through via and cannot carry a back drill", via.id)));
            }
        }
        if let Some(net) = via.net.as_deref()
            && !net_names.contains(net)
        {
            return Err(crate::invalid(format!("via `{}` references undeclared net `{net}`", via.id)));
        }
    }

    let mut slot_ids = HashSet::new();
    for slot in &board.slots {
        if slot.id.trim().is_empty() {
            return Err(crate::invalid("a slot has an empty id"));
        }
        if !slot_ids.insert(slot.id.as_str()) {
            return Err(crate::invalid(format!("slot id `{}` is duplicated", slot.id)));
        }
        checks(slot.from[0], &format!("slot `{}` from.x", slot.id))?;
        checks(slot.from[1], &format!("slot `{}` from.y", slot.id))?;
        checks(slot.to[0], &format!("slot `{}` to.x", slot.id))?;
        checks(slot.to[1], &format!("slot `{}` to.y", slot.id))?;
        sizes(slot.width, &format!("slot `{}` width", slot.id))?;
        if (slot.to[0] - slot.from[0]).hypot(slot.to[1] - slot.from[1]) <= 1e-9 {
            return Err(crate::invalid(format!("slot `{}` has no length", slot.id)));
        }
        if let Some(net) = slot.net.as_deref()
            && !net_names.contains(net)
        {
            return Err(crate::invalid(format!("slot `{}` references undeclared net `{net}`", slot.id)));
        }
    }

    let mut class_ids = HashSet::new();
    let mut class_names = HashSet::new();
    let mut assigned_nets = HashSet::new();
    for class in &board.net_classes {
        if class.id.trim().is_empty() {
            return Err(crate::invalid("a net class has an empty id"));
        }
        if !class_ids.insert(class.id.as_str()) {
            return Err(crate::invalid(format!("net class id `{}` is duplicated", class.id)));
        }
        if class.name.trim().is_empty() {
            return Err(crate::invalid(format!("net class `{}` has an empty name", class.id)));
        }
        if !class_names.insert(class.name.as_str()) {
            return Err(crate::invalid(format!("net class name `{}` is duplicated", class.name)));
        }
        for (value, field) in [
            (class.width, "width"),
            (class.clearance, "clearance"),
            (class.via_diameter, "viaDiameter"),
            (class.via_drill, "viaDrill"),
        ] {
            if let Some(value) = value {
                sizes(value, &format!("net class `{}` {field}", class.name))?;
            }
        }
        for net in &class.nets {
            if net.trim().is_empty() {
                return Err(crate::invalid(format!("net class `{}` has an empty net name", class.name)));
            }
            if !net_names.contains(net.as_str()) {
                return Err(crate::invalid(format!("net class `{}` references undeclared net `{net}`", class.name)));
            }
            if !assigned_nets.insert(net.as_str()) {
                return Err(crate::invalid(format!("net `{net}` is assigned to more than one net class")));
            }
        }
    }

    let mut test_point_ids = HashSet::new();
    let mut test_targets = HashSet::new();
    for point in &board.test_points {
        if point.id.trim().is_empty() {
            return Err(crate::invalid("a test point has an empty id"));
        }
        if !test_point_ids.insert(point.id.as_str()) {
            return Err(crate::invalid(format!("test point id `{}` is duplicated", point.id)));
        }
        if point.target.trim().is_empty() {
            return Err(crate::invalid(format!("test point `{}` has an empty target", point.id)));
        }
        if !test_targets.insert(point.target.as_str()) {
            return Err(crate::invalid(format!("test point target `{}` is declared twice", point.target)));
        }
        if board.resolve_test_point(&point.target).is_none() {
            return Err(crate::invalid(format!("test point `{}` references missing target `{}`", point.id, point.target)));
        }
    }

    let mut zone_ids = HashSet::new();
    for zone in &board.zones {
        if zone.id.trim().is_empty() {
            return Err(crate::invalid("a zone has an empty id"));
        }
        if !zone_ids.insert(zone.id.as_str()) {
            return Err(crate::invalid(format!("zone id `{}` is duplicated", zone.id)));
        }
        let Some(layer) = board.layer(&zone.layer)
        else {
            return Err(crate::invalid(format!("zone `{}` references missing layer `{}`", zone.id, zone.layer)));
        };
        if layer.kind != LayerKind::Copper {
            return Err(crate::invalid(format!("zone `{}` sits on `{}`, which is not a copper layer", zone.id, zone.layer)));
        }
        if zone.points.len() < 3 {
            return Err(crate::invalid(format!("zone `{}` has fewer than three points", zone.id)));
        }
        for (index, point) in zone.points.iter().enumerate() {
            checks(point[0], &format!("zone `{}` point {index} x", zone.id))?;
            checks(point[1], &format!("zone `{}` point {index} y", zone.id))?;
        }
        if polygon_area(&zone.points) <= 1e-9 {
            return Err(crate::invalid(format!("zone `{}` encloses no area", zone.id)));
        }
        if let Some(net) = zone.net.as_deref()
            && !net_names.contains(net)
        {
            return Err(crate::invalid(format!("zone `{}` references undeclared net `{net}`", zone.id)));
        }
    }

    let mut text_ids = HashSet::new();
    for item in &board.texts {
        if item.id.trim().is_empty() {
            return Err(crate::invalid("a text item has an empty id"));
        }
        if !text_ids.insert(item.id.as_str()) {
            return Err(crate::invalid(format!("text item id `{}` is duplicated", item.id)));
        }
        if item.text.trim().is_empty() {
            return Err(crate::invalid(format!("text item `{}` has empty content", item.id)));
        }
        let Some(layer) = board.layer(&item.layer)
        else {
            return Err(crate::invalid(format!("text item `{}` references missing layer `{}`", item.id, item.layer)));
        };
        if layer.kind != LayerKind::Silkscreen {
            return Err(crate::invalid(format!(
                "text item `{}` sits on `{}`, which is not a silkscreen layer",
                item.id, item.layer
            )));
        }
        checks(item.at[0], &format!("text item `{}` at.x", item.id))?;
        checks(item.at[1], &format!("text item `{}` at.y", item.id))?;
        checks(item.rotation, &format!("text item `{}` rotation", item.id))?;
        sizes(item.size, &format!("text item `{}` size", item.id))?;
        sizes(item.thickness, &format!("text item `{}` thickness", item.id))?;
    }

    let mut keepout_ids = HashSet::new();
    for keepout in &board.keepouts {
        if keepout.id.trim().is_empty() {
            return Err(crate::invalid("a keepout has an empty id"));
        }
        if !keepout_ids.insert(keepout.id.as_str()) {
            return Err(crate::invalid(format!("keepout id `{}` is duplicated", keepout.id)));
        }
        if keepout.layer != "*" {
            let Some(layer) = board.layer(&keepout.layer)
            else {
                return Err(crate::invalid(format!("keepout `{}` references missing layer `{}`", keepout.id, keepout.layer)));
            };
            if layer.kind != LayerKind::Copper {
                return Err(crate::invalid(format!(
                    "keepout `{}` sits on `{}`, which is not a copper layer",
                    keepout.id, keepout.layer
                )));
            }
        }
        if keepout.points.len() < 3 {
            return Err(crate::invalid(format!("keepout `{}` has fewer than three points", keepout.id)));
        }
        for (index, point) in keepout.points.iter().enumerate() {
            checks(point[0], &format!("keepout `{}` point {index} x", keepout.id))?;
            checks(point[1], &format!("keepout `{}` point {index} y", keepout.id))?;
        }
        if polygon_area(&keepout.points) <= 1e-9 {
            return Err(crate::invalid(format!("keepout `{}` encloses no area", keepout.id)));
        }
    }

    let mut pair_ids = HashSet::new();
    for pair in &board.differential_pairs {
        if pair.id.trim().is_empty() {
            return Err(crate::invalid("a differential pair has an empty id"));
        }
        if !pair_ids.insert(pair.id.as_str()) {
            return Err(crate::invalid(format!("differential pair id `{}` is duplicated", pair.id)));
        }
        if pair.positive.trim().is_empty() || pair.negative.trim().is_empty() {
            return Err(crate::invalid(format!("differential pair `{}` has an empty net name", pair.id)));
        }
        if pair.positive == pair.negative {
            return Err(crate::invalid(format!("differential pair `{}` names the same net twice", pair.id)));
        }
        for net in [&pair.positive, &pair.negative] {
            if !net_names.contains(net.as_str()) {
                return Err(crate::invalid(format!("differential pair `{}` references undeclared net `{net}`", pair.id)));
            }
        }
        sizes(pair.width, &format!("differential pair `{}` width", pair.id))?;
        sizes(pair.gap, &format!("differential pair `{}` gap", pair.id))?;
        checks(pair.max_skew, &format!("differential pair `{}` maxSkew", pair.id))?;
        if pair.max_skew < 0.0 {
            return Err(crate::invalid(format!("differential pair `{}` has a negative maxSkew", pair.id)));
        }
    }

    let mut waiver_ids = HashSet::new();
    for waiver in &board.waivers {
        if waiver.id.trim().is_empty() {
            return Err(crate::invalid("a waiver has an empty id"));
        }
        if !waiver_ids.insert(waiver.id.as_str()) {
            return Err(crate::invalid(format!("waiver id `{}` is duplicated", waiver.id)));
        }
        if waiver.code.trim().is_empty() {
            return Err(crate::invalid(format!("waiver `{}` has an empty code", waiver.id)));
        }
        if let Some(object) = waiver.object.as_deref()
            && object.trim().is_empty()
        {
            return Err(crate::invalid(format!("waiver `{}` has an empty object", waiver.id)));
        }
    }
    for (code, severity) in &board.severity_overrides {
        if code.trim().is_empty() {
            return Err(crate::invalid("a severity override has an empty code"));
        }
        if !matches!(severity.as_str(), "info" | "warning" | "error") {
            return Err(crate::invalid(format!(
                "severity override for `{code}` is `{severity}`; expected info, warning or error"
            )));
        }
    }
    Ok(())
}

fn checks(value: f64, field: &str) -> Result<(), ServiceError> {
    if value.is_finite() { Ok(()) } else { Err(crate::invalid(format!("{field} is not finite"))) }
}

fn sizes(value: f64, field: &str) -> Result<(), ServiceError> {
    checks(value, field)?;
    if value > 0.0 { Ok(()) } else { Err(crate::invalid(format!("{field} must be greater than zero"))) }
}

/// Projects a board to the summary shape shared by the SDK, CLI and studio.
pub(crate) fn summary(board: &Board) -> Value {
    let layers: Vec<Value> = board
        .layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            json!({
                "id": layer.id,
                "index": index,
                "title": layer.name,
                "kind": layer.kind,
                "visible": layer.visible,
                "locked": layer.locked,
                "objects": objects_on(board, layer),
            })
        })
        .collect();
    json!({
        "id": board.id,
        "revision": board.revision,
        "title": board.title,
        "units": board.units,
        "outline": board.outline,
        "rules": board.rules,
        "stackup": board.stackup.map(Stackup::to_value),
        "waivers": board.waivers.iter().map(Waiver::to_value).collect::<Vec<_>>(),
        "severityOverrides": board.severity_overrides,
        "netClasses": board.net_classes.iter().map(NetClass::to_value).collect::<Vec<_>>(),
        "layers": layers,
        "counts": {
            "nets": board.nets.len(),
            "footprints": board.footprints.len(),
            "tracks": board.tracks.len(),
            "vias": board.vias.len(),
            "slots": board.slots.len(),
            "zones": board.zones.len(),
            "keepouts": board.keepouts.len(),
            "texts": board.texts.len(),
            "netClasses": board.net_classes.len(),
            "testPoints": board.test_points.len(),
            "differentialPairs": board.differential_pairs.len(),
            "waivers": board.waivers.len(),
        },
    })
}

fn objects_on(board: &Board, layer: &Layer) -> usize {
    let mut count = board.footprints.iter().filter(|footprint| footprint.layer == layer.id).count();
    count += board.tracks.iter().filter(|track| track.layer == layer.id).count();
    count += board.zones.iter().filter(|zone| zone.layer == layer.id).count();
    count += board.keepouts.iter().filter(|keepout| board.keepout_covers(keepout, &layer.id)).count();
    count += board.texts.iter().filter(|item| item.layer == layer.id).count();
    if layer.kind == LayerKind::Copper {
        count += board.vias.iter().filter(|via| board.via_covers(via, &layer.id)).count();
        count += board.slots.len();
    }
    count
}

/// Request of [`crate::add_footprint`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddFootprintRequest {
    /// Reference designator, required.
    pub reference: Option<String>,
    /// Component value.
    pub value: Option<String>,
    /// Package or footprint name.
    pub footprint: Option<String>,
    /// Placement X.
    pub x: Option<f64>,
    /// Placement Y.
    pub y: Option<f64>,
    /// Rotation in degrees.
    pub rotation: Option<f64>,
    /// Layer id; defaults to `F.Cu`.
    pub layer: Option<String>,
}

/// Request of [`crate::add_pad`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPadRequest {
    /// Footprint id that owns the pad.
    pub footprint: String,
    /// Pad number or name, unique within the footprint.
    pub number: String,
    /// Pad centre, X.
    pub x: f64,
    /// Pad centre, Y.
    pub y: f64,
    /// Pad width.
    pub w: f64,
    /// Pad height.
    pub h: f64,
    /// Declared net name, when assigned.
    #[serde(default)]
    pub net: Option<String>,
}

/// Request of [`crate::update_pad`]. At least one field is required.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePadRequest {
    /// Footprint id that owns the pad.
    pub footprint: String,
    /// Pad number or name to update.
    pub number: String,
    /// New pad centre, X.
    pub x: Option<f64>,
    /// New pad centre, Y.
    pub y: Option<f64>,
    /// New pad width.
    pub w: Option<f64>,
    /// New pad height.
    pub h: Option<f64>,
    /// New declared net name.
    pub net: Option<String>,
}

/// Request of [`crate::delete_pad`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePadRequest {
    /// Footprint id that owns the pad.
    pub footprint: String,
    /// Pad number or name to delete.
    pub number: String,
}

/// Request of [`crate::add_net_class`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddNetClassRequest {
    /// Class name, unique on the board.
    pub name: String,
    /// Target trace width, in board units.
    #[serde(default)]
    pub width: Option<f64>,
    /// Minimum copper clearance for nets in the class, in board units.
    #[serde(default)]
    pub clearance: Option<f64>,
    /// Minimum via copper diameter, in board units.
    #[serde(default)]
    pub via_diameter: Option<f64>,
    /// Minimum via finished drill, in board units.
    #[serde(default)]
    pub via_drill: Option<f64>,
}

/// Request of [`crate::assign_net_class`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssignNetClassRequest {
    /// Declared net name to assign.
    pub net: String,
    /// Net class id or name.
    pub class: String,
}

/// Request of [`crate::add_test_point`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddTestPointRequest {
    /// Pad key `footprint-id:pad-number` or via id.
    pub target: String,
}

/// Request of [`crate::add_zone`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddZoneRequest {
    /// Copper layer id that receives the pour.
    pub layer: String,
    /// Declared net the pour belongs to, when assigned.
    #[serde(default)]
    pub net: Option<String>,
    /// Polygon vertices in board units, in drawing order.
    pub points: Vec<[f64; 2]>,
}

/// Request of [`crate::add_text`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddTextRequest {
    /// Text content; must not be blank.
    pub text: String,
    /// Silkscreen layer id the legend lives on.
    pub layer: String,
    /// Anchor position in board units.
    pub at: [f64; 2],
    /// Rotation in degrees; defaults to 0.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// Glyph height in board units; defaults to 1.0.
    #[serde(default)]
    pub size: Option<f64>,
    /// Stroke thickness in board units; defaults to 0.15.
    #[serde(default)]
    pub thickness: Option<f64>,
}

/// Request of [`crate::update_text`]. At least one field is required.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTextRequest {
    /// Text item id to update.
    pub id: String,
    /// New content.
    #[serde(default)]
    pub text: Option<String>,
    /// New silkscreen layer id.
    #[serde(default)]
    pub layer: Option<String>,
    /// New anchor position in board units.
    #[serde(default)]
    pub at: Option<[f64; 2]>,
    /// New rotation in degrees.
    #[serde(default)]
    pub rotation: Option<f64>,
    /// New glyph height in board units.
    #[serde(default)]
    pub size: Option<f64>,
    /// New stroke thickness in board units.
    #[serde(default)]
    pub thickness: Option<f64>,
}

/// Request of [`crate::add_keepout`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddKeepoutRequest {
    /// Copper layer id, or `*` for every copper layer; defaults to `*`.
    #[serde(default)]
    pub layer: Option<String>,
    /// Polygon vertices in board units, in drawing order.
    pub points: Vec<[f64; 2]>,
}

/// Request of [`crate::add_track`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddTrackRequest {
    /// Declared net name, when assigned.
    #[serde(default)]
    pub net: Option<String>,
    /// Copper layer id.
    pub layer: String,
    /// Trace width.
    pub width: f64,
    /// Segment start.
    pub from: [f64; 2],
    /// Segment end.
    pub to: [f64; 2],
}

/// Request of [`crate::add_via`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddViaRequest {
    /// Declared net name, when assigned.
    #[serde(default)]
    pub net: Option<String>,
    /// Via centre.
    pub at: [f64; 2],
    /// Copper diameter.
    pub diameter: f64,
    /// Finished drill diameter.
    pub drill: f64,
    /// Copper layer the span starts on; omit both span fields for a through via.
    #[serde(default)]
    pub from_layer: Option<String>,
    /// Copper layer the span ends on; omit both span fields for a through via.
    #[serde(default)]
    pub to_layer: Option<String>,
}

/// Request of [`crate::update_track`]. At least one editable field is required.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTrackRequest {
    /// Track id to update.
    pub id: String,
    /// New declared net name.
    #[serde(default)]
    pub net: Option<String>,
    /// New copper layer id.
    #[serde(default)]
    pub layer: Option<String>,
    /// New trace width.
    #[serde(default)]
    pub width: Option<f64>,
    /// New segment start.
    #[serde(default)]
    pub from: Option<[f64; 2]>,
    /// New segment end.
    #[serde(default)]
    pub to: Option<[f64; 2]>,
}

/// Request of [`crate::update_via`]. At least one editable field is required.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateViaRequest {
    /// Via id to update.
    pub id: String,
    /// New declared net name.
    #[serde(default)]
    pub net: Option<String>,
    /// New via centre.
    #[serde(default)]
    pub at: Option<[f64; 2]>,
    /// New copper diameter.
    #[serde(default)]
    pub diameter: Option<f64>,
    /// New finished drill diameter.
    #[serde(default)]
    pub drill: Option<f64>,
}

/// Request of [`crate::add_slot`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddSlotRequest {
    /// Declared net name, when assigned.
    #[serde(default)]
    pub net: Option<String>,
    /// One end of the slot centreline.
    pub from: [f64; 2],
    /// The other end of the slot centreline.
    pub to: [f64; 2],
    /// Tool diameter, equal to the cut width; greater than zero.
    pub width: f64,
}

/// Request of [`crate::add_teardrops`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddTeardropsRequest {
    /// Teardrop shape; `linear` is the only implemented value.
    #[serde(default)]
    pub shape: Option<String>,
    /// Length from the pad edge to the teardrop tip, in board units.
    #[serde(default)]
    pub length: Option<f64>,
}

/// Request of [`crate::stitch_vias`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddStitchingRequest {
    /// Declared net the stitching vias join.
    pub net: String,
    /// Target spacing between adjacent vias along the perimeter; greater than zero.
    pub spacing: f64,
    /// Inset from the board edge; defaults to 0.5.
    #[serde(default)]
    pub inset: Option<f64>,
    /// Via copper diameter; defaults to 0.6.
    #[serde(default)]
    pub diameter: Option<f64>,
    /// Via finished drill; defaults to 0.3.
    #[serde(default)]
    pub drill: Option<f64>,
}

/// Request of [`crate::set_via_back_drill`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetViaBackDrillRequest {
    /// Via id, required.
    pub id: String,
    /// Removed barrel depth, in board units; greater than zero.
    #[serde(default)]
    pub depth: Option<f64>,
    /// Remaining stub length, in board units; not negative.
    #[serde(default)]
    pub stub: Option<f64>,
}

/// Request of [`crate::add_net`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddNetRequest {
    /// Net name, required.
    pub name: String,
}

/// Request of [`crate::add_differential_pair`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDifferentialPairRequest {
    /// Net routed as the positive half; must be declared.
    pub positive: String,
    /// Net routed as the negative half; must be declared and differ from `positive`.
    pub negative: String,
    /// Target trace width in board units, greater than zero.
    pub width: f64,
    /// Target edge-to-edge gap between the halves, in board units and greater than zero.
    pub gap: f64,
    /// Largest allowed route-length difference between the halves, in board units.
    #[serde(default)]
    pub max_skew: f64,
}

/// Request of [`crate::add_waiver`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddWaiverRequest {
    /// Finding code the waiver silences, required.
    pub code: String,
    /// Object id the waiver is narrowed to; omit to cover every object.
    #[serde(default)]
    pub object: Option<String>,
    /// Human-readable reason the finding was accepted.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Request of [`crate::set_severity`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSeverityRequest {
    /// Finding code whose severity is overridden, required.
    pub code: String,
    /// New severity: `info`, `warning` or `error`.
    pub severity: String,
}

/// Request of [`crate::move_footprint`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveFootprintRequest {
    /// Footprint id, required.
    pub id: String,
    /// New placement X.
    pub x: Option<f64>,
    /// New placement Y.
    pub y: Option<f64>,
    /// New rotation in degrees.
    pub rotation: Option<f64>,
    /// New layer id.
    pub layer: Option<String>,
}

/// Request of [`crate::update_footprint`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFootprintRequest {
    /// Footprint id, required.
    pub id: String,
    /// New component value.
    pub value: Option<String>,
    /// New package or footprint name.
    pub footprint: Option<String>,
}

/// Request of [`crate::add_layer`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLayerRequest {
    /// Layer name, required and unique.
    pub name: String,
    /// Layer kind; defaults to `copper`.
    pub kind: Option<String>,
    /// CSS colour; defaults to the kind colour.
    pub color: Option<String>,
}

/// Request of [`crate::update_layer`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateLayerRequest {
    /// Layer id, required.
    pub id: String,
    /// New name; renames the id and updates every reference.
    pub name: Option<String>,
    /// New kind.
    pub kind: Option<String>,
    /// New CSS colour.
    pub color: Option<String>,
    /// New visibility.
    pub visible: Option<bool>,
    /// New lock state.
    pub locked: Option<bool>,
}

/// Request of [`crate::set_outline`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetOutlineRequest {
    /// Lower-left corner in board units.
    pub from: [f64; 2],
    /// Upper-right corner in board units.
    pub to: [f64; 2],
}

/// Request of [`crate::set_rules`]. At least one rule is required.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRulesRequest {
    /// Minimum gap between two drill holes of different nets, in millimetres.
    pub hole_clearance: Option<f64>,
    /// Minimum copper-to-board-edge gap, in millimetres.
    pub copper_to_edge: Option<f64>,
    /// Minimum component-to-board-edge gap, in millimetres.
    pub component_to_edge: Option<f64>,
    /// Minimum drill-hole-to-board-edge gap, in millimetres.
    pub hole_to_edge: Option<f64>,
    /// Solder-mask expansion around each pad, per side and in millimetres.
    pub solder_mask_expansion: Option<f64>,
}

/// Request of [`crate::set_stackup`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetStackupRequest {
    /// Copper foil thickness, in millimetres.
    pub copper_thickness: f64,
    /// Dielectric thickness between adjacent copper layers, in millimetres.
    pub dielectric_height: f64,
    /// Relative permittivity of the dielectric; at least 1.
    pub dielectric_constant: f64,
}

/// Request of [`crate::stackup_impedance`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpedanceRequest {
    /// Trace geometry: `microstrip` (default) or `stripline`.
    #[serde(default)]
    pub structure: Option<String>,
    /// Trace width, in millimetres.
    pub width: f64,
    /// Dielectric thickness, in millimetres; defaults to the board stackup.
    #[serde(default)]
    pub dielectric_height: Option<f64>,
    /// Relative permittivity; defaults to the board stackup.
    #[serde(default)]
    pub dielectric_constant: Option<f64>,
    /// Copper foil thickness, in millimetres; defaults to the board stackup or 0.035.
    #[serde(default)]
    pub copper_thickness: Option<f64>,
}

/// Request of [`crate::stackup_width`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidthRequest {
    /// Trace geometry: `microstrip` (default) or `stripline`.
    #[serde(default)]
    pub structure: Option<String>,
    /// Target single-ended impedance, in ohms.
    pub impedance: f64,
    /// Dielectric thickness, in millimetres; defaults to the board stackup.
    #[serde(default)]
    pub dielectric_height: Option<f64>,
    /// Relative permittivity; defaults to the board stackup.
    #[serde(default)]
    pub dielectric_constant: Option<f64>,
    /// Copper foil thickness, in millimetres; defaults to the board stackup or 0.035.
    #[serde(default)]
    pub copper_thickness: Option<f64>,
}

/// Request of [`crate::impedance_table`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpedanceTableRequest {
    /// Trace geometry: `microstrip` (default) or `stripline`.
    #[serde(default)]
    pub structure: Option<String>,
    /// Target single-ended impedances in ohms; defaults to `[50]`.
    #[serde(default)]
    pub targets: Option<Vec<f64>>,
    /// Dielectric thickness in millimetres; defaults to the board stackup.
    #[serde(default)]
    pub dielectric_height: Option<f64>,
    /// Relative permittivity; defaults to the board stackup.
    #[serde(default)]
    pub dielectric_constant: Option<f64>,
    /// Copper foil thickness in millimetres; defaults to the board stackup or 0.035.
    #[serde(default)]
    pub copper_thickness: Option<f64>,
}

/// Request of [`crate::route`].
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteOptions {
    /// Copper layers to route on, in preference order; defaults to `F.Cu` then `B.Cu`.
    #[serde(default)]
    pub layers: Option<Vec<String>>,
    /// Trace width; defaults to 0.25 board units.
    #[serde(default)]
    pub width: Option<f64>,
    /// Clearance kept from existing copper; defaults to 0.2 board units.
    #[serde(default)]
    pub clearance: Option<f64>,
    /// Via copper diameter; defaults to 0.6 board units.
    #[serde(default)]
    pub via_diameter: Option<f64>,
    /// Via finished drill; defaults to 0.3 board units.
    #[serde(default)]
    pub via_drill: Option<f64>,
    /// Route only this declared net.
    #[serde(default)]
    pub net: Option<String>,
    /// Rip-up and retry rounds after the first pass; defaults to 2, capped at 8.
    #[serde(default)]
    pub retries: Option<u32>,
}
