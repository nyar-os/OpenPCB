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
    /// Copper pour regions; absent on documents written before zones existed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) zones: Vec<Zone>,
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
    /// Next zone sequence number.
    #[serde(default = "one")]
    pub(crate) next_zone: u64,
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
            zones: Vec::new(),
            differential_pairs: Vec::new(),
            severity_overrides: BTreeMap::new(),
            waivers: Vec::new(),
            next_pair: 1,
            next_waiver: 1,
            next_footprint: 1,
            next_track: 1,
            next_via: 1,
            next_zone: 1,
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
fn polygon_area(points: &[[f64; 2]]) -> f64 {
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
        "waivers": board.waivers.iter().map(Waiver::to_value).collect::<Vec<_>>(),
        "severityOverrides": board.severity_overrides,
        "layers": layers,
        "counts": {
            "nets": board.nets.len(),
            "footprints": board.footprints.len(),
            "tracks": board.tracks.len(),
            "vias": board.vias.len(),
            "zones": board.zones.len(),
            "differentialPairs": board.differential_pairs.len(),
            "waivers": board.waivers.len(),
        },
    })
}

fn objects_on(board: &Board, layer: &Layer) -> usize {
    let mut count = board.footprints.iter().filter(|footprint| footprint.layer == layer.id).count();
    count += board.tracks.iter().filter(|track| track.layer == layer.id).count();
    count += board.zones.iter().filter(|zone| zone.layer == layer.id).count();
    if layer.kind == LayerKind::Copper {
        count += board.vias.iter().filter(|via| board.via_covers(via, &layer.id)).count();
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
