//! Controlled-impedance calculations for microstrip and stripline traces.
//!
//! The formulas are the closed-form IPC-2141 approximations: a microstrip over
//! a ground plane and a stripline centred between two reference planes. They are
//! deterministic, need no field solver, and match the stackup values stored on
//! the board. Width conversion inverts the same formula by bisection.

use crate::{ServiceError, board::Stackup};

/// Trace geometry accepted by the impedance helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Structure {
    /// Microstrip over a ground plane.
    Microstrip,
    /// Stripline between two reference planes.
    Stripline,
}

impl Structure {
    /// Parses the public structure name; an absent name means microstrip.
    pub(crate) fn parse(raw: Option<&str>) -> Result<Self, ServiceError> {
        match raw.map(str::trim).map(str::to_lowercase).as_deref() {
            None | Some("") | Some("microstrip") => Ok(Self::Microstrip),
            Some("stripline") => Ok(Self::Stripline),
            Some(other) => Err(crate::argument(format!("`{other}` is not a trace structure; use microstrip or stripline"))),
        }
    }

    /// Public name written into reports.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Microstrip => "microstrip",
            Self::Stripline => "stripline",
        }
    }
}

/// Single-ended impedance of one trace, in ohms.
pub(crate) fn impedance(structure: Structure, width: f64, stackup: Stackup) -> Result<f64, ServiceError> {
    let width = positive(width, "width")?;
    let height = positive(stackup.dielectric_height, "dielectricHeight")?;
    let thickness = positive(stackup.copper_thickness, "copperThickness")?;
    if stackup.dielectric_constant < 1.0 {
        return Err(crate::argument("`dielectricConstant` must be at least 1"));
    }
    let er = stackup.dielectric_constant;
    let value = match structure {
        Structure::Microstrip => 87.0 / (er + 1.41).sqrt() * (5.98 * height / (0.8 * width + thickness)).ln(),
        Structure::Stripline => {
            let ratio = 4.0 * height / (0.67 * std::f64::consts::PI * width * (0.8 + thickness / width));
            60.0 / er.sqrt() * ratio.ln()
        }
    };
    if value.is_finite() && value > 0.0 {
        Ok(value)
    }
    else {
        Err(crate::argument("the stackup values do not produce a finite impedance"))
    }
}

/// Trace width that reaches one target impedance, in millimetres.
///
/// The impedance falls as the width grows, so a bisection over a bounded width
/// range finds the single solution. A target outside the reachable range is
/// rejected instead of being clamped.
pub(crate) fn width(structure: Structure, target: f64, stackup: Stackup) -> Result<f64, ServiceError> {
    let target = positive(target, "impedance")?;
    let mut low = 0.0001f64;
    let mut high = widest(structure, stackup);
    if high <= low * 2.0 {
        return Err(crate::argument("the stackup dielectric is too thin for a controlled-width trace"));
    }
    high *= 0.999;
    let low_impedance = impedance(structure, low, stackup)?;
    let high_impedance = impedance(structure, high, stackup)?;
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
        let value = impedance(structure, middle, stackup)?;
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

/// Widest trace the closed-form formula still describes.
///
/// Beyond this width the logarithm turns non-positive, so the formula no longer
/// returns a physical impedance. The bisection stays just inside it.
fn widest(structure: Structure, stackup: Stackup) -> f64 {
    let height = stackup.dielectric_height;
    let thickness = stackup.copper_thickness;
    match structure {
        Structure::Microstrip => (5.98 * height - thickness) / 0.8,
        Structure::Stripline => (4.0 * height / (0.67 * std::f64::consts::PI) - thickness) / 0.8,
    }
}

fn positive(value: f64, field: &str) -> Result<f64, ServiceError> {    if !value.is_finite() {
        return Err(crate::argument(format!("`{field}` must be a finite number")));
    }
    if value > 0.0 { Ok(value) } else { Err(crate::argument(format!("`{field}` must be greater than zero"))) }
}
