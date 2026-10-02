use core::simd::{Select, Simd, cmp::SimdPartialOrd};
use std::{
    ops,
    simd::{SimdCast, SimdElement, num::SimdFloat},
};

/// Clamped linear interpolation.
///
/// Clamps the interpolation factor to [0, 1] before interpolating.
///
/// Java reference: `Mth.clampedLerp(double, double, double)`.
/// Note: Vanilla's parameter order is `(factor, min, max)`, ours is `(min, max, factor)`.
#[inline]
#[must_use]
pub fn clamped_lerp(min: f64, max: f64, factor: f64) -> f64 {
    if factor < 0.0 {
        min
    } else if factor > 1.0 {
        max
    } else {
        lerp(factor, min, max)
    }
}

/// Clamped lerp for N lanes.
#[inline]
#[must_use]
pub fn clamped_lerp_simd<const N: usize>(
    min: Simd<f64, N>,
    max: Simd<f64, N>,
    factor: Simd<f64, N>,
) -> Simd<f64, N> {
    let zero = Simd::splat(0.0);
    let one = Simd::splat(1.0);
    let below = factor.simd_lt(zero);
    let above = factor.simd_gt(one);

    // lerp result for the middle case
    let lerped = min + factor * (max - min);

    // Select: below zero → min, above one → max, otherwise → lerped
    let result = below.select(min, lerped);
    above.select(max, result)
}

/// Clamp a value to the range [min, max].
///
/// Java reference: `Mth.clamp(double, double, double)`
#[inline]
#[must_use]
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

#[cfg(test)]
mod clamp_tests {
    use super::*;

    #[test]
    #[expect(clippy::float_cmp, reason = "clamp returns exact input, min, or max float")]
    fn test_clamp_below_min() {
        assert_eq!(clamp(-5.0, 0.0, 10.0), 0.0);
        assert_eq!(clamp(-100.0, -50.0, 50.0), -50.0);
    }

    #[test]
    #[expect(clippy::float_cmp, reason = "clamp returns exact input, min, or max float")]
    fn test_clamp_above_max() {
        assert_eq!(clamp(15.0, 0.0, 10.0), 10.0);
        assert_eq!(clamp(100.0, -50.0, 50.0), 50.0);
    }

    #[test]
    #[expect(clippy::float_cmp, reason = "clamp returns exact input, min, or max float")]
    fn test_clamp_within_range() {
        assert_eq!(clamp(5.0, 0.0, 10.0), 5.0);
        assert_eq!(clamp(0.0, -50.0, 50.0), 0.0);
    }

    #[test]
    #[expect(clippy::float_cmp, reason = "clamp returns exact input, min, or max float")]
    fn test_clamp_at_boundaries() {
        assert_eq!(clamp(0.0, 0.0, 10.0), 0.0);
        assert_eq!(clamp(10.0, 0.0, 10.0), 10.0);
    }

    #[test]
    #[expect(clippy::float_cmp, reason = "clamp returns exact input, min, or max float")]
    fn test_clamp_negative_range() {
        assert_eq!(clamp(-15.0, -20.0, -10.0), -15.0);
        assert_eq!(clamp(-25.0, -20.0, -10.0), -20.0);
        assert_eq!(clamp(-5.0, -20.0, -10.0), -10.0);
    }

    #[test]
    fn test_clamp_i32() {
        assert_eq!(clamp_i32(-5, 0, 10), 0);
        assert_eq!(clamp_i32(15, 0, 10), 10);
        assert_eq!(clamp_i32(5, 0, 10), 5);
        assert_eq!(clamp_i32(0, 0, 10), 0);
        assert_eq!(clamp_i32(10, 0, 10), 10);
    }
}


/// Clamp a value to the range [min, max] (i32 version).
#[inline]
#[must_use]
pub const fn clamp_i32(value: i32, min: i32, max: i32) -> i32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}
/// Inverse linear interpolation (find the factor t such that lerp(t, a, b) == value).
///
/// Java reference: `Mth.inverseLerp(double, double, double)`
#[inline]
#[must_use]
pub fn inverse_lerp(value: f64, a: f64, b: f64) -> f64 {
    (value - a) / (b - a)
}

/// Linear interpolation.
///
/// Formula: a + alpha * (b - a)
///
/// Java reference: `Mth.lerp(double, double, double)`
#[expect(clippy::inline_always, reason = "hot-path noise primitive")]
#[inline(always)]
#[must_use]
pub fn lerp(alpha: f64, a: f64, b: f64) -> f64 {
    a + alpha * (b - a)
}

/// SIMD linear interpolation.
#[expect(clippy::inline_always, reason = "hot-path noise primitive")]
#[inline(always)]
#[must_use]
pub fn lerp_simd<F, const N: usize>(alpha: Simd<F, N>, a: Simd<F, N>, b: Simd<F, N>) -> Simd<F, N>
where
    F: SimdElement,
    Simd<F, N>: ops::Mul<Output = Simd<F, N>>
        + ops::Add<Output = Simd<F, N>>
        + ops::Sub<Output = Simd<F, N>>,
{
    a + alpha * (b - a)
}

/// Bilinear interpolation.
///
/// Interpolates between 4 values in a 2D grid.
///
/// Java reference: `Mth.lerp2(double, double, double, double, double, double)`
#[expect(clippy::inline_always, reason = "hot-path noise primitive")]
#[inline(always)]
#[must_use]
pub fn lerp2(a1: f64, a2: f64, x00: f64, x10: f64, x01: f64, x11: f64) -> f64 {
    lerp(a2, lerp(a1, x00, x10), lerp(a1, x01, x11))
}

/// SIMD bilinear interpolation.
#[expect(clippy::inline_always, reason = "hot-path noise primitive")]
#[inline(always)]
#[must_use]
pub fn lerp2_simd<F, const N: usize>(
    a1: Simd<F, N>,
    a2: Simd<F, N>,
    x00: Simd<F, N>,
    x10: Simd<F, N>,
    x01: Simd<F, N>,
    x11: Simd<F, N>,
) -> Simd<F, N>
where
    F: SimdElement,
    Simd<F, N>: ops::Mul<Output = Simd<F, N>>
        + ops::Add<Output = Simd<F, N>>
        + ops::Sub<Output = Simd<F, N>>,
{
    lerp_simd(a2, lerp_simd(a1, x00, x10), lerp_simd(a1, x01, x11))
}

/// Trilinear interpolation.
///
/// Interpolates between 8 values in a 3D grid.
///
/// Java reference: `Mth.lerp3(...)`
#[expect(clippy::inline_always, reason = "hot-path noise primitive")]
#[inline(always)]
#[must_use]
#[expect(
    clippy::too_many_arguments,
    reason = "matches vanilla's Mth.lerp3 signature with 8 grid corner values"
)]
pub fn lerp3(
    a1: f64,
    a2: f64,
    a3: f64,
    x000: f64,
    x100: f64,
    x010: f64,
    x110: f64,
    x001: f64,
    x101: f64,
    x011: f64,
    x111: f64,
) -> f64 {
    lerp(
        a3,
        lerp2(a1, a2, x000, x100, x010, x110),
        lerp2(a1, a2, x001, x101, x011, x111),
    )
}

/// Trilinear interpolation for N lanes. see lerp3.
#[inline]
#[expect(clippy::too_many_arguments, reason = "mirrors lerp3 with SIMD vectors")]
#[must_use]
pub fn lerp3_simd<F, const N: usize>(
    a1: Simd<F, N>,
    a2: Simd<F, N>,
    a3: Simd<F, N>,
    x000: Simd<F, N>,
    x100: Simd<F, N>,
    x010: Simd<F, N>,
    x110: Simd<F, N>,
    x001: Simd<F, N>,
    x101: Simd<F, N>,
    x011: Simd<F, N>,
    x111: Simd<F, N>,
) -> Simd<F, N>
where
    F: SimdElement,
    Simd<F, N>: ops::Mul<Output = Simd<F, N>>
        + ops::Add<Output = Simd<F, N>>
        + ops::Sub<Output = Simd<F, N>>,
{
    lerp_simd(
        a3,
        lerp2_simd(a1, a2, x000, x100, x010, x110),
        lerp2_simd(a1, a2, x001, x101, x011, x111),
    )
}

#[cfg(test)]
mod lerp_tests {
    use super::*;

    #[test]
    fn test_lerp() {
        assert!((lerp(0.0, 10.0, 20.0) - 10.0).abs() < 1e-10);
        assert!((lerp(1.0, 10.0, 20.0) - 20.0).abs() < 1e-10);
        assert!((lerp(0.5, 10.0, 20.0) - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_lerp3() {
        let x000 = 1.0;
        let x100 = 2.0;
        let x010 = 3.0;
        let x110 = 4.0;
        let x001 = 5.0;
        let x101 = 6.0;
        let x011 = 7.0;
        let x111 = 8.0;

        // Test all 8 corners
        assert!((lerp3(0.0, 0.0, 0.0, x000, x100, x010, x110, x001, x101, x011, x111) - x000).abs() < 1e-10);
        assert!((lerp3(1.0, 0.0, 0.0, x000, x100, x010, x110, x001, x101, x011, x111) - x100).abs() < 1e-10);
        assert!((lerp3(0.0, 1.0, 0.0, x000, x100, x010, x110, x001, x101, x011, x111) - x010).abs() < 1e-10);
        assert!((lerp3(1.0, 1.0, 0.0, x000, x100, x010, x110, x001, x101, x011, x111) - x110).abs() < 1e-10);
        assert!((lerp3(0.0, 0.0, 1.0, x000, x100, x010, x110, x001, x101, x011, x111) - x001).abs() < 1e-10);
        assert!((lerp3(1.0, 0.0, 1.0, x000, x100, x010, x110, x001, x101, x011, x111) - x101).abs() < 1e-10);
        assert!((lerp3(0.0, 1.0, 1.0, x000, x100, x010, x110, x001, x101, x011, x111) - x011).abs() < 1e-10);
        assert!((lerp3(1.0, 1.0, 1.0, x000, x100, x010, x110, x001, x101, x011, x111) - x111).abs() < 1e-10);

        // Test center (0.5, 0.5, 0.5) which should be average of all 8 corners
        let avg = (x000 + x100 + x010 + x110 + x001 + x101 + x011 + x111) / 8.0;
        assert!((lerp3(0.5, 0.5, 0.5, x000, x100, x010, x110, x001, x101, x011, x111) - avg).abs() < 1e-10);

        // Test intermediate point
        // lerp(a1, x000, x100) = lerp(0.25, 1, 2) = 1.25
        // lerp(a1, x010, x110) = lerp(0.25, 3, 4) = 3.25
        // lerp2(0.25, 0.5, z0) = lerp(0.5, 1.25, 3.25) = 2.25
        // lerp(a1, x001, x101) = lerp(0.25, 5, 6) = 5.25
        // lerp(a1, x011, x111) = lerp(0.25, 7, 8) = 7.25
        // lerp2(0.25, 0.5, z1) = lerp(0.5, 5.25, 7.25) = 6.25
        // lerp(0.75, 2.25, 6.25) = 2.25 + 0.75 * 4 = 5.25
        let val = lerp3(0.25, 0.5, 0.75, x000, x100, x010, x110, x001, x101, x011, x111);
        assert!((val - 5.25).abs() < 1e-10);
    }

    #[test]
    fn test_lerp2() {
        let x00 = 0.0;
        let x10 = 10.0;
        let x01 = 20.0;
        let x11 = 30.0;

        // Corners
        assert!((lerp2(0.0, 0.0, x00, x10, x01, x11) - x00).abs() < 1e-10);
        assert!((lerp2(1.0, 0.0, x00, x10, x01, x11) - x10).abs() < 1e-10);
        assert!((lerp2(0.0, 1.0, x00, x10, x01, x11) - x01).abs() < 1e-10);
        assert!((lerp2(1.0, 1.0, x00, x10, x01, x11) - x11).abs() < 1e-10);

        // Midpoints / fractional factors
        assert!((lerp2(0.5, 0.5, x00, x10, x01, x11) - 15.0).abs() < 1e-10);
        assert!((lerp2(0.25, 0.75, x00, x10, x01, x11) - 17.5).abs() < 1e-10);

        // Signed / negative grid values
        let nx00 = -10.0;
        let nx10 = 10.0;
        let nx01 = -20.0;
        let nx11 = 20.0;
        assert!((lerp2(0.5, 0.5, nx00, nx10, nx01, nx11) - 0.0).abs() < 1e-10);

    // Extrapolation outside [0, 1]
    assert!((lerp2(1.5, 0.0, x00, x10, x01, x11) - 15.0).abs() < 1e-10);
}
}

/// Map a value from one range to another (unclamped).
///
/// Unlike [`map_clamped`], the result can extrapolate outside `[to_min, to_max]`.
///
/// Java reference: `Mth.map(double, double, double, double, double)`
#[inline]
#[must_use]
pub fn map(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    lerp(inverse_lerp(value, from_min, from_max), to_min, to_max)
}

#[cfg(test)]
mod map_tests {
    use super::*;

    #[test]
    fn test_map_in_range() {
        // [0, 1] mapped to [10, 20]
        assert!((map(0.0, 0.0, 1.0, 10.0, 20.0) - 10.0).abs() < 1e-10);
        assert!((map(1.0, 0.0, 1.0, 10.0, 20.0) - 20.0).abs() < 1e-10);
        assert!((map(0.5, 0.0, 1.0, 10.0, 20.0) - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_map_inverted_range() {
        // [0, 1] mapped to inverted target range [100, 0]
        assert!((map(0.0, 0.0, 1.0, 100.0, 0.0) - 100.0).abs() < 1e-10);
        assert!((map(1.0, 0.0, 1.0, 100.0, 0.0) - 0.0).abs() < 1e-10);
        assert!((map(0.2, 0.0, 1.0, 100.0, 0.0) - 80.0).abs() < 1e-10);
    }

    #[test]
    fn test_map_unclamped_extrapolation() {
        // Values outside [from_min, from_max] extrapolate linearly
        assert!((map(1.5, 0.0, 1.0, 10.0, 20.0) - 25.0).abs() < 1e-10);
        assert!((map(-0.5, 0.0, 1.0, 10.0, 20.0) - 5.0).abs() < 1e-10);
    }
}

/// Map a value from one range to another with clamped lerp.
///
/// Used for Y-clamped gradients in density functions.
#[inline]
#[must_use]
pub fn map_clamped(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    let t = (value - from_min) / (from_max - from_min);
    clamped_lerp(to_min, to_max, t)
}
/// Smoothstep - quintic Hermite interpolation (NOT cubic!)
///
/// Formula: 6x^5 - 15x^4 + 10x^3
///
/// This is the standard smoothstep used in Perlin noise for smooth transitions.
/// Java reference: `Mth.smoothstep(double)`
#[expect(clippy::inline_always, reason = "hot-path noise primitive")]
#[inline(always)]
#[must_use]
pub fn smoothstep(x: f64) -> f64 {
    x * x * x * (x * (x * 6.0 - 15.0) + 10.0)
}

/// Smoothstep derivative for noise with derivatives.
///
/// Formula: 30x^2(x-1)^2
///
/// Java reference: `Mth.smoothstepDerivative(double)`
#[inline]
#[must_use]
pub fn smoothstep_derivative(x: f64) -> f64 {
    30.0 * x * x * (x - 1.0) * (x - 1.0)
}

/// Smoothstep for N lanes: 6x^5 - 15x^4 + 10x^3. Per-lane identical to [`smoothstep`].
#[inline]
#[must_use]
pub fn smoothstep_simd<F, const N: usize>(x: Simd<F, N>) -> Simd<F, N>
where
    F: SimdElement + SimdCast,
    Simd<F, N>: ops::Mul<Output = Simd<F, N>>
        + ops::Sub<Output = Simd<F, N>>
        + ops::Add<Output = Simd<F, N>>,
{
    x * x
        * x
        * (x * (x * Simd::splat(6.0).cast() - Simd::splat(15.0).cast()) + Simd::splat(10.0).cast())
}

#[cfg(test)]
mod smoothstep_tests {
    use super::*;
    #[test]
    fn test_smoothstep() {
        // At boundaries
        assert!((smoothstep(0.0) - 0.0).abs() < 1e-10);
        assert!((smoothstep(1.0) - 1.0).abs() < 1e-10);
        // At midpoint
        assert!((smoothstep(0.5) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_smoothstep_derivative() {
        // Key evaluation points
        assert!((smoothstep_derivative(0.0) - 0.0).abs() < 1e-10);
        assert!((smoothstep_derivative(1.0) - 0.0).abs() < 1e-10);
        assert!((smoothstep_derivative(0.5) - 1.875).abs() < 1e-10);

        // Symmetry property: S'(x) == S'(1.0 - x)
        let x = 0.25;
        assert!((smoothstep_derivative(x) - smoothstep_derivative(1.0 - x)).abs() < 1e-10);
        assert!((smoothstep_derivative(x) - 1.054_687_5).abs() < 1e-10);

        // Numerical derivative comparison using central finite differences
        let h = 1e-6;
        for &t in &[0.1, 0.3, 0.5, 0.7, 0.9] {
            let numerical_derivative = (smoothstep(t + h) - smoothstep(t - h)) / (2.0 * h);
            let analytical_derivative = smoothstep_derivative(t);
            assert!(
                (numerical_derivative - analytical_derivative).abs() < 1e-5,
                "Mismatch at t = {t}: numerical = {numerical_derivative}, analytical = {analytical_derivative}"
            );
        }
    }
}
