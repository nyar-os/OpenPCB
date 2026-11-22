//! Controlled-impedance calculations for microstrip and stripline traces.
//!
//! The formulas are the closed-form IPC-2141 approximations: a microstrip over
//! a ground plane and a stripline centred between two reference planes. They are
//! deterministic, need no field solver, and match the stackup values stored on
//! the board. Width conversion inverts the same formula by bisection.
//!
//! An edge-coupled pair adds the standard coupling correction to the isolated
//! single-ended impedance `Z0` of one half:
//!
//! - differential microstrip: `Zdiff = 2 * Z0 * (1 - 0.48 * exp(-0.96 * s / h))`
//! - differential stripline: `Zdiff = 2 * Z0 * (1 - 0.37 * exp(-2.9 * s / b))`
//!
//! with edge gap `s`, dielectric height `h` and reference-plane spacing `b`.
//! The stackup stores one dielectric height, so a stripline uses `b = 2 * h`.
//! Odd-mode impedance is half the differential value and even-mode impedance is
//! `2 * Z0 - Zodd`, so the arithmetic mean of the two modes is the isolated
//! single-ended impedance. The correction is an approximation: it assumes
//! symmetric edge coupling on a homogeneous dielectric and matches measured
//! boards best for `0.1 <= w / h <= 3` and `0.1 <= s / h <= 3`. It replaces
//! neither a field solver nor a fabrication stackup sign-off.

use crate::{ServiceError, board::Stackup};

/// Trace geometry accepted by the impedance helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Structure {
    /// Microstrip over a ground plane.
    Microstrip,
    /// Stripline between two reference planes.
    Stripline,
    /// Edge-coupled microstrip pair over a ground plane.
    DiffMicrostrip,
    /// Edge-coupled stripline pair between two reference planes.
    DiffStripline,
}

impl Structure {
    /// Parses the public structure name; an absent name means microstrip.
    pub(crate) fn parse(raw: Option<&str>) -> Result<Self, ServiceError> {
        match raw.map(str::trim).map(str::to_lowercase).as_deref() {
            None | Some("") | Some("microstrip") => Ok(Self::Microstrip),
            Some("stripline") => Ok(Self::Stripline),
            Some("diffmicrostrip") => Ok(Self::DiffMicrostrip),
            Some("diffstripline") => Ok(Self::DiffStripline),
            Some(other) => Err(crate::argument(format!(
                "`{other}` is not a trace structure; use microstrip, stripline, diffMicrostrip or diffStripline"
            ))),
        }
    }

    /// Public name written into reports.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Microstrip => "microstrip",
            Self::Stripline => "stripline",
            Self::DiffMicrostrip => "diffMicrostrip",
            Self::DiffStripline => "diffStripline",
        }
    }

    /// Whether the structure describes an edge-coupled pair.
    pub(crate) fn differential(self) -> bool {
        matches!(self, Self::DiffMicrostrip | Self::DiffStripline)
    }

    /// The single-ended geometry behind a pair.
    fn single(self) -> Self {
        match self {
            Self::DiffMicrostrip => Self::Microstrip,
            Self::DiffStripline => Self::Stripline,
            single => single,
        }
    }
}

/// Impedance of one trace or one differential pair, in ohms.
pub(crate) struct Values {
    /// Single-ended impedance, or differential impedance for a pair.
    pub(crate) impedance: f64,
    /// Odd-mode impedance of each half; `None` for a single-ended trace.
    pub(crate) odd: Option<f64>,
    /// Even-mode impedance of each half; `None` for a single-ended trace.
    pub(crate) even: Option<f64>,
}

/// Resolved stackup values shared by every formula.
#[derive(Clone, Copy)]
struct Parts {
    height: f64,
    thickness: f64,
    er: f64,
}

impl Parts {
    fn new(stackup: Stackup) -> Result<Self, ServiceError> {
        let height = positive(stackup.dielectric_height, "dielectricHeight")?;
        let thickness = positive(stackup.copper_thickness, "copperThickness")?;
        if stackup.dielectric_constant < 1.0 {
            return Err(crate::argument("`dielectricConstant` must be at least 1"));
        }
        Ok(Self { height, thickness, er: stackup.dielectric_constant })
    }
}

/// Impedance of one trace or pair, in ohms.
///
/// `gap` is the edge-to-edge spacing of a differential pair and must be absent
/// for a single-ended structure.
pub(crate) fn values(structure: Structure, width: f64, gap: Option<f64>, stackup: Stackup) -> Result<Values, ServiceError> {
    let width = positive(width, "width")?;
    let parts = Parts::new(stackup)?;
    if structure.differential() {
        let gap = required_gap(gap)?;
        let single = single_ended(structure.single(), width, parts);
        let (factor, ratio, reference) = match structure {
            Structure::DiffMicrostrip => (0.48, 0.96, parts.height),
            _ => (0.37, 2.9, parts.height * 2.0),
        };
        let coupling = factor * (-ratio * gap / reference).exp();
        let odd = single * (1.0 - coupling);
        let even = single * (1.0 + coupling);
        Ok(Values { impedance: 2.0 * odd, odd: Some(odd), even: Some(even) })
    }
    else {
        if gap.is_some() {
            return Err(crate::argument("`gap` applies only to a differential structure"));
        }
        Ok(Values { impedance: single_ended(structure, width, parts), odd: None, even: None })
    }
}

/// Trace width that reaches one target impedance, in millimetres.
///
/// The impedance falls as the width grows, so a bisection over a bounded width
/// range finds the single solution. A target outside the reachable range is
/// rejected instead of being clamped. For a pair the target is the differential
/// impedance and the edge gap is fixed by the request.
pub(crate) fn width(structure: Structure, target: f64, gap: Option<f64>, stackup: Stackup) -> Result<f64, ServiceError> {
    let target = positive(target, "impedance")?;
    let parts = Parts::new(stackup)?;
    let gap = if structure.differential() {
        Some(required_gap(gap)?)
    }
    else if gap.is_some() {
        return Err(crate::argument("`gap` applies only to a differential structure"));
    }
    else {
        None
    };
    let mut low = 0.0001f64;
    let mut high = widest(structure.single(), parts);
    if high <= low * 2.0 {
        return Err(crate::argument("the stackup dielectric is too thin for a controlled-width trace"));
    }
    high *= 0.999;
    let value_at = |width: f64| values(structure, width, gap, stackup).map(|values| values.impedance);
    let low_impedance = value_at(low)?;
    let high_impedance = value_at(high)?;
    if target > low_impedance {
        return Err(crate::argument(format!(
            "{target:.3} ohm is above the {:.3} ohm reachable at the narrowest trace",
            low_impedance
        )));
    }
    if target < high_impedance {
        return Err(crate::argument(format!(
            "{target:.3} ohm is below the {:.3} ohm reachable at the widest trace",
            high_impedance
        )));
    }
    for _ in 0..120 {
        let middle = (low + high) / 2.0;
        let value = value_at(middle)?;
        if (value - target).abs() <= 1e-6 {
            return Ok(middle);
        }
        if value > target {
            low = middle;
        }
        else {
            high = middle;
        }
    }
    Ok((low + high) / 2.0)
}

/// Single-ended impedance of one trace, in ohms.
fn single_ended(structure: Structure, width: f64, parts: Parts) -> f64 {
    match structure {
        Structure::Stripline => {
            let ratio = 4.0 * parts.height / (0.67 * std::f64::consts::PI * width * (0.8 + parts.thickness / width));
            60.0 / parts.er.sqrt() * ratio.ln()
        }
        _ => 87.0 / (parts.er + 1.41).sqrt() * (5.98 * parts.height / (0.8 * width + parts.thickness)).ln(),
    }
}

/// Widest trace the closed-form formula still describes.
///
/// Beyond this width the logarithm turns non-positive, so the formula no longer
/// returns a physical impedance. The bisection stays just inside it.
fn widest(structure: Structure, parts: Parts) -> f64 {
    match structure {
        Structure::Stripline => (4.0 * parts.height / (0.67 * std::f64::consts::PI) - parts.thickness) / 0.8,
        _ => (5.98 * parts.height - parts.thickness) / 0.8,
    }
}

/// Reads the edge gap of a pair; a differential request must declare one.
fn required_gap(gap: Option<f64>) -> Result<f64, ServiceError> {
    match gap {
        Some(gap) => positive(gap, "gap"),
        None => Err(crate::argument("`gap` is required for a differential structure")),
    }
}

fn positive(value: f64, field: &str) -> Result<f64, ServiceError> {
    if !value.is_finite() {
        return Err(crate::argument(format!("`{field}` must be a finite number")));
    }
    if value > 0.0 { Ok(value) } else { Err(crate::argument(format!("`{field}` must be greater than zero"))) }
}
