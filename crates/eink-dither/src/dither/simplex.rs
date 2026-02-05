//! Simplex (Delaunay) ordered dithering algorithm.
//!
//! Simplex dithering decomposes each pixel's color into barycentric weights
//! over the enclosing Delaunay tetrahedron (up to 4 palette colors), then
//! uses a blue noise threshold to stochastically select one of those colors.
//! Over an area, this produces provably correct color averages.
//!
//! # Comparison with other algorithms
//!
//! | Algorithm | Colors/pixel | Spatial | Best for |
//! |-----------|-------------|---------|----------|
//! | Error diffusion | 1 (nearest) | Error diffusion | Gradients |
//! | BlueNoiseDither | 2 (linear interp) | Ordered + blue noise | Graphics |
//! | **SimplexDither** | **up to 4 (barycentric)** | **Ordered + blue noise** | **Both** |
//!
//! # Algorithm
//!
//! 1. **Precomputation**: Delaunay triangulation of palette in OKLab 3D space.
//!    For N ≤ 16, brute-force enumeration of all C(N,4) candidate tetrahedra
//!    with circumsphere emptiness check.
//! 2. **Per-pixel**: Find enclosing tetrahedron, compute barycentric weights,
//!    use blue noise threshold to select one color stochastically.
//! 3. **Fallback**: Pixels outside the convex hull or with low chroma fall
//!    back to 2-nearest-color linear blend (same as BlueNoiseDither).

use super::blue_noise::find_second_nearest;
use super::blue_noise_matrix::BLUE_NOISE_64;
use super::{find_exact_match, Dither, DitherOptions};
use crate::color::{LinearRgb, Oklab};
use crate::palette::Palette;

/// OKLab chroma threshold below which pixels are treated as achromatic.
/// Pixels with chroma below this bypass simplex lookup and use 2-nearest
/// achromatic blend instead, preventing grey→chromatic contamination.
///
/// Matches the project's established pattern: HyAB kchroma=10 prevents
/// the same issue for BlueNoiseDither's find_second_nearest.
const ACHROMATIC_THRESHOLD: f32 = 0.03;

/// A precomputed Delaunay tetrahedron with inverse barycentric matrix.
#[derive(Debug, Clone)]
struct Tetrahedron {
    /// Palette indices of the 4 vertices
    indices: [usize; 4],
    /// Inverse of the edge matrix [v1-v0, v2-v0, v3-v0] for barycentric coords.
    /// Stored as row-major 3x3.
    inv_matrix: [f32; 9],
    /// Origin vertex (v0) in OKLab
    origin: [f32; 3],
}

/// A precomputed Delaunay triangle (for N=3 palettes, 2D simplex).
#[derive(Debug, Clone)]
struct Triangle {
    /// Palette indices of the 3 vertices
    indices: [usize; 3],
    /// Inverse of the 2D edge matrix [v1-v0, v2-v0] projected onto the
    /// triangle's plane. Stored as 2x2 row-major, plus the plane basis vectors.
    /// We store the full 3D→barycentric transform.
    inv_2x2: [f32; 4],
    /// Basis vectors of the triangle plane (2 vectors, each 3 components)
    basis: [[f32; 3]; 2],
    /// Origin vertex (v0) in OKLab
    origin: [f32; 3],
}

/// Result of decomposing a color into simplex weights.
pub(crate) struct DecompositionResult {
    /// Palette indices (up to 4)
    indices: [usize; 4],
    /// Barycentric weights (sum to 1.0)
    weights: [f32; 4],
    /// Number of active vertices (2, 3, or 4)
    count: usize,
}

/// Reusable color decomposition logic for Delaunay simplex dithering.
///
/// Builds a 3D Delaunay triangulation of the palette in OKLab space and
/// provides per-pixel decomposition into barycentric weights.
#[derive(Debug, Clone)]
pub(crate) struct SimplexDecomposer {
    /// Precomputed tetrahedra (N >= 4)
    tetrahedra: Vec<Tetrahedron>,
    /// Precomputed triangle (N = 3)
    triangle: Option<Triangle>,
    /// Number of palette colors
    palette_size: usize,
}

impl SimplexDecomposer {
    /// Build the Delaunay triangulation for a palette.
    pub fn build(palette: &Palette) -> Self {
        let n = palette.len();

        let points: Vec<[f32; 3]> = (0..n)
            .map(|i| {
                let c = palette.actual_oklab(i);
                [c.l, c.a, c.b]
            })
            .collect();

        if n < 3 {
            return Self {
                tetrahedra: Vec::new(),
                triangle: None,
                palette_size: n,
            };
        }

        if n == 3 {
            let triangle = build_triangle(&points, [0, 1, 2]);
            return Self {
                tetrahedra: Vec::new(),
                triangle: Some(triangle),
                palette_size: n,
            };
        }

        // N >= 4: brute-force Delaunay triangulation
        let tetrahedra = build_delaunay(&points);

        // For N=3 case embedded in larger palette, we may want triangles too,
        // but for simplicity we rely on tetrahedra + fallback.
        Self {
            tetrahedra,
            triangle: None,
            palette_size: n,
        }
    }

    /// Decompose a color into barycentric weights over the enclosing simplex.
    ///
    /// Returns `None` if the point is outside the convex hull (needs fallback).
    pub fn decompose(&self, color: Oklab) -> Option<DecompositionResult> {
        let p = [color.l, color.a, color.b];

        match self.palette_size {
            0 => None,
            1 => Some(DecompositionResult {
                indices: [0, 0, 0, 0],
                weights: [1.0, 0.0, 0.0, 0.0],
                count: 1,
            }),
            2 => None, // Always use fallback for 2-color
            3 => {
                // Use triangle decomposition
                if let Some(ref tri) = self.triangle {
                    decompose_triangle(tri, &p)
                } else {
                    None
                }
            }
            _ => {
                // Try each tetrahedron
                for tet in &self.tetrahedra {
                    if let Some(result) = decompose_tetrahedron(tet, &p) {
                        return Some(result);
                    }
                }
                None
            }
        }
    }
}

/// Simplex (Delaunay) ordered dithering.
///
/// Uses Delaunay triangulation in OKLab space to decompose each pixel's color
/// into barycentric weights over up to 4 palette colors, then selects one
/// color stochastically using a blue noise threshold. This produces provably
/// correct color averages over any area.
///
/// # When to Use
///
/// - Photos and graphics alike (4-color blending captures inter-palette colors)
/// - When you want no error bleeding across edges (per-pixel independent)
/// - When you want provably correct color averages
///
/// # Ignored Options
///
/// Since this is a per-pixel algorithm with no error propagation:
/// - `serpentine`: Ignored
/// - `error_clamp`: Ignored
/// - `chroma_clamp`: Ignored
#[derive(Debug, Clone)]
pub struct SimplexDither {
    decomposer: SimplexDecomposer,
}

impl SimplexDither {
    /// Create a new SimplexDither for the given palette.
    ///
    /// Precomputes the Delaunay triangulation. This is O(N^4) for N palette
    /// colors but N is typically 2-16 so it's negligible.
    pub fn new(palette: &Palette) -> Self {
        Self {
            decomposer: SimplexDecomposer::build(palette),
        }
    }
}

impl Dither for SimplexDither {
    fn dither(
        &self,
        image: &[LinearRgb],
        width: usize,
        height: usize,
        palette: &Palette,
        options: &DitherOptions,
    ) -> Vec<u8> {
        let mut output = vec![0u8; width * height];

        for y in 0..height {
            for x in 0..width {
                let idx = y * width + x;
                let pixel = image[idx];

                // Check for exact palette match first (if enabled)
                if options.preserve_exact_matches {
                    if let Some(palette_idx) = find_exact_match(pixel, palette) {
                        output[idx] = palette_idx;
                        continue;
                    }
                }

                let oklab = Oklab::from(pixel);
                let pixel_chroma = (oklab.a * oklab.a + oklab.b * oklab.b).sqrt();

                // Achromatic bypass: low-chroma pixels use 2-nearest blend
                // to prevent grey→chromatic contamination from simplex lookup.
                if pixel_chroma < ACHROMATIC_THRESHOLD && palette.len() > 2 {
                    output[idx] = two_nearest_blend(oklab, pixel_chroma, palette, x, y);
                    continue;
                }

                // Try simplex decomposition
                if let Some(result) = self.decomposer.decompose(oklab) {
                    output[idx] = stochastic_select(
                        &result.indices[..result.count],
                        &result.weights[..result.count],
                        x,
                        y,
                    );
                } else {
                    // Fallback: 2-nearest-color blend (like BlueNoiseDither)
                    output[idx] = two_nearest_blend(oklab, pixel_chroma, palette, x, y);
                }
            }
        }

        output
    }
}

// ============================================================================
// Stochastic color selection
// ============================================================================

/// Select a palette index from weighted candidates using blue noise threshold.
fn stochastic_select(indices: &[usize], weights: &[f32], x: usize, y: usize) -> u8 {
    let threshold = BLUE_NOISE_64[y % 64][x % 64] as f32 / 255.0;
    let mut cumulative = 0.0;
    for (i, &w) in weights.iter().enumerate() {
        cumulative += w;
        if threshold < cumulative {
            return indices[i] as u8;
        }
    }
    // Safety fallback (rounding)
    *indices.last().unwrap() as u8
}

/// Two-nearest-color blend fallback (matches BlueNoiseDither behavior).
fn two_nearest_blend(oklab: Oklab, pixel_chroma: f32, palette: &Palette, x: usize, y: usize) -> u8 {
    let (idx1, raw_dist1) = palette.find_nearest(oklab);
    let (idx2, raw_dist2) = find_second_nearest(oklab, palette, idx1, pixel_chroma);

    let dist1 = if palette.is_euclidean() {
        raw_dist1.sqrt()
    } else {
        raw_dist1
    };
    let dist2 = if palette.is_euclidean() {
        raw_dist2.sqrt()
    } else {
        raw_dist2
    };
    let total_dist = dist1 + dist2;

    if total_dist < 1e-10 {
        return idx1 as u8;
    }

    let blend = dist1 / total_dist;
    let threshold = BLUE_NOISE_64[y % 64][x % 64] as f32 / 255.0;

    if threshold < (1.0 - blend) {
        idx1 as u8
    } else {
        idx2 as u8
    }
}

// ============================================================================
// Delaunay triangulation (brute-force for N ≤ 16)
// ============================================================================

/// Build Delaunay tetrahedralization by brute-force circumsphere test.
///
/// Enumerates all C(N,4) candidate tetrahedra, computes circumspheres,
/// and keeps only those where no other point is strictly inside.
fn build_delaunay(points: &[[f32; 3]]) -> Vec<Tetrahedron> {
    let n = points.len();
    let mut tetrahedra = Vec::new();

    // Enumerate all combinations of 4 points
    for i in 0..n {
        for j in (i + 1)..n {
            for k in (j + 1)..n {
                for l in (k + 1)..n {
                    let p0 = points[i];
                    let p1 = points[j];
                    let p2 = points[k];
                    let p3 = points[l];

                    // Compute circumsphere
                    let Some((center, radius_sq)) = circumsphere(&p0, &p1, &p2, &p3) else {
                        continue; // Degenerate (coplanar)
                    };

                    // Check if any other point is strictly inside
                    let mut is_delaunay = true;
                    for (m, pt) in points.iter().enumerate() {
                        if m == i || m == j || m == k || m == l {
                            continue;
                        }
                        let dx = pt[0] - center[0];
                        let dy = pt[1] - center[1];
                        let dz = pt[2] - center[2];
                        let dist_sq = dx * dx + dy * dy + dz * dz;
                        if dist_sq < radius_sq - 1e-10 {
                            is_delaunay = false;
                            break;
                        }
                    }

                    if is_delaunay {
                        // Compute inverse barycentric matrix
                        if let Some(tet) = build_tetrahedron(&p0, &p1, &p2, &p3, [i, j, k, l]) {
                            tetrahedra.push(tet);
                        }
                    }
                }
            }
        }
    }

    tetrahedra
}

/// Compute circumsphere center and squared radius of 4 points.
///
/// Solves the 3x3 linear system using Cramer's rule:
/// ```text
/// (p1-p0) · c = ((p1-p0) · (p1+p0)) / 2
/// (p2-p0) · c = ((p2-p0) · (p2+p0)) / 2
/// (p3-p0) · c = ((p3-p0) · (p3+p0)) / 2
/// ```
fn circumsphere(
    p0: &[f32; 3],
    p1: &[f32; 3],
    p2: &[f32; 3],
    p3: &[f32; 3],
) -> Option<([f32; 3], f32)> {
    let d1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let d2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let d3 = [p3[0] - p0[0], p3[1] - p0[1], p3[2] - p0[2]];

    let rhs1 = 0.5 * (d1[0] * (p1[0] + p0[0]) + d1[1] * (p1[1] + p0[1]) + d1[2] * (p1[2] + p0[2]));
    let rhs2 = 0.5 * (d2[0] * (p2[0] + p0[0]) + d2[1] * (p2[1] + p0[1]) + d2[2] * (p2[2] + p0[2]));
    let rhs3 = 0.5 * (d3[0] * (p3[0] + p0[0]) + d3[1] * (p3[1] + p0[1]) + d3[2] * (p3[2] + p0[2]));

    // 3x3 Cramer's rule
    let det = det3x3(&[d1, d2, d3]);

    if det.abs() < 1e-12 {
        return None; // Degenerate (coplanar or coincident)
    }

    let inv_det = 1.0 / det;
    let cx = det3x3(&[
        [rhs1, d1[1], d1[2]],
        [rhs2, d2[1], d2[2]],
        [rhs3, d3[1], d3[2]],
    ]) * inv_det;
    let cy = det3x3(&[
        [d1[0], rhs1, d1[2]],
        [d2[0], rhs2, d2[2]],
        [d3[0], rhs3, d3[2]],
    ]) * inv_det;
    let cz = det3x3(&[
        [d1[0], d1[1], rhs1],
        [d2[0], d2[1], rhs2],
        [d3[0], d3[1], rhs3],
    ]) * inv_det;

    let dx = cx - p0[0];
    let dy = cy - p0[1];
    let dz = cz - p0[2];
    let radius_sq = dx * dx + dy * dy + dz * dz;

    Some(([cx, cy, cz], radius_sq))
}

/// 3x3 determinant from row-major matrix [[row0], [row1], [row2]].
#[inline]
fn det3x3(m: &[[f32; 3]; 3]) -> f32 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

/// Build a Tetrahedron with precomputed inverse barycentric matrix.
fn build_tetrahedron(
    p0: &[f32; 3],
    p1: &[f32; 3],
    p2: &[f32; 3],
    p3: &[f32; 3],
    indices: [usize; 4],
) -> Option<Tetrahedron> {
    // Edge vectors from v0
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
    let e3 = [p3[0] - p0[0], p3[1] - p0[1], p3[2] - p0[2]];

    // Matrix M = [e1, e2, e3] as column vectors → to invert we need det
    // of the column matrix, which equals det of its transpose (row form).
    let det = det3x3(&[
        [e1[0], e2[0], e3[0]],
        [e1[1], e2[1], e3[1]],
        [e1[2], e2[2], e3[2]],
    ]);

    if det.abs() < 1e-12 {
        return None; // Degenerate
    }

    let inv_det = 1.0 / det;

    // Cofactor matrix (transposed = adjugate), divided by det
    let inv = [
        (e2[1] * e3[2] - e2[2] * e3[1]) * inv_det,
        (e2[2] * e3[0] - e2[0] * e3[2]) * inv_det,
        (e2[0] * e3[1] - e2[1] * e3[0]) * inv_det,
        (e1[2] * e3[1] - e1[1] * e3[2]) * inv_det,
        (e1[0] * e3[2] - e1[2] * e3[0]) * inv_det,
        (e1[1] * e3[0] - e1[0] * e3[1]) * inv_det,
        (e1[1] * e2[2] - e1[2] * e2[1]) * inv_det,
        (e1[2] * e2[0] - e1[0] * e2[2]) * inv_det,
        (e1[0] * e2[1] - e1[1] * e2[0]) * inv_det,
    ];

    Some(Tetrahedron {
        indices,
        inv_matrix: inv,
        origin: *p0,
    })
}

/// Try to decompose a point using a tetrahedron's barycentric coordinates.
///
/// Returns `Some(result)` if the point is inside (all weights >= 0).
fn decompose_tetrahedron(tet: &Tetrahedron, p: &[f32; 3]) -> Option<DecompositionResult> {
    let d = [
        p[0] - tet.origin[0],
        p[1] - tet.origin[1],
        p[2] - tet.origin[2],
    ];

    // λ = inv_matrix * d
    let l1 = tet.inv_matrix[0] * d[0] + tet.inv_matrix[1] * d[1] + tet.inv_matrix[2] * d[2];
    let l2 = tet.inv_matrix[3] * d[0] + tet.inv_matrix[4] * d[1] + tet.inv_matrix[5] * d[2];
    let l3 = tet.inv_matrix[6] * d[0] + tet.inv_matrix[7] * d[1] + tet.inv_matrix[8] * d[2];
    let l0 = 1.0 - l1 - l2 - l3;

    // Small tolerance for numerical precision at boundaries
    const EPS: f32 = -1e-6;

    if l0 >= EPS && l1 >= EPS && l2 >= EPS && l3 >= EPS {
        // Clamp and normalize to ensure exact sum = 1.0
        let w0 = l0.max(0.0);
        let w1 = l1.max(0.0);
        let w2 = l2.max(0.0);
        let w3 = l3.max(0.0);
        let total = w0 + w1 + w2 + w3;

        if total < 1e-10 {
            return None;
        }

        let inv_total = 1.0 / total;
        Some(DecompositionResult {
            indices: tet.indices,
            weights: [
                w0 * inv_total,
                w1 * inv_total,
                w2 * inv_total,
                w3 * inv_total,
            ],
            count: 4,
        })
    } else {
        None
    }
}

// ============================================================================
// Triangle (N=3) support
// ============================================================================

/// Build a Triangle for 3-color palettes.
///
/// Projects the 3D problem onto the triangle's plane and precomputes
/// the 2D inverse matrix for barycentric coordinates.
fn build_triangle(points: &[[f32; 3]], indices: [usize; 3]) -> Triangle {
    let p0 = points[indices[0]];
    let p1 = points[indices[1]];
    let p2 = points[indices[2]];

    // Edge vectors in 3D
    let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
    let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];

    // Project into 2D: use dot products to get the 2x2 Gram matrix
    let d11 = dot3(&e1, &e1);
    let d12 = dot3(&e1, &e2);
    let d22 = dot3(&e2, &e2);

    let det = d11 * d22 - d12 * d12;
    let inv_det = if det.abs() > 1e-12 { 1.0 / det } else { 0.0 };

    // Inverse of 2x2 Gram matrix [[d11,d12],[d12,d22]]
    let inv_2x2 = [d22 * inv_det, -d12 * inv_det, -d12 * inv_det, d11 * inv_det];

    Triangle {
        indices,
        inv_2x2,
        basis: [e1, e2],
        origin: p0,
    }
}

/// Decompose a point using triangle barycentric coordinates.
fn decompose_triangle(tri: &Triangle, p: &[f32; 3]) -> Option<DecompositionResult> {
    let d = [
        p[0] - tri.origin[0],
        p[1] - tri.origin[1],
        p[2] - tri.origin[2],
    ];

    // Project d onto the triangle's basis: (d·e1, d·e2)
    let proj1 = dot3(&d, &tri.basis[0]);
    let proj2 = dot3(&d, &tri.basis[1]);

    // Barycentric: [λ1, λ2] = inv_2x2 * [proj1, proj2]
    let l1 = tri.inv_2x2[0] * proj1 + tri.inv_2x2[1] * proj2;
    let l2 = tri.inv_2x2[2] * proj1 + tri.inv_2x2[3] * proj2;
    let l0 = 1.0 - l1 - l2;

    const EPS: f32 = -1e-4;

    if l0 >= EPS && l1 >= EPS && l2 >= EPS {
        let w0 = l0.max(0.0);
        let w1 = l1.max(0.0);
        let w2 = l2.max(0.0);
        let total = w0 + w1 + w2;

        if total < 1e-10 {
            return None;
        }

        let inv_total = 1.0 / total;
        Some(DecompositionResult {
            indices: [tri.indices[0], tri.indices[1], tri.indices[2], 0],
            weights: [w0 * inv_total, w1 * inv_total, w2 * inv_total, 0.0],
            count: 3,
        })
    } else {
        None
    }
}

/// 3D dot product.
#[inline]
fn dot3(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Srgb;

    fn make_bw_palette() -> Palette {
        Palette::new(
            &[Srgb::from_u8(0, 0, 0), Srgb::from_u8(255, 255, 255)],
            None,
        )
        .unwrap()
    }

    fn make_3_color_palette() -> Palette {
        Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
            ],
            None,
        )
        .unwrap()
    }

    fn make_6_color_palette() -> Palette {
        let colors = [
            Srgb::from_u8(0, 0, 0),       // black
            Srgb::from_u8(255, 255, 255), // white
            Srgb::from_u8(255, 0, 0),     // red
            Srgb::from_u8(0, 255, 0),     // green
            Srgb::from_u8(0, 0, 255),     // blue
            Srgb::from_u8(255, 255, 0),   // yellow
        ];
        Palette::new(&colors, None).unwrap()
    }

    #[test]
    fn test_single_color_palette() {
        let colors = [Srgb::from_u8(128, 128, 128)];
        let palette = Palette::new(&colors, None).unwrap();
        let dither = SimplexDither::new(&palette);
        let options = DitherOptions::new();

        let image: Vec<LinearRgb> = (0..16)
            .map(|i| {
                let v = i as f32 / 15.0;
                LinearRgb::new(v, v, v)
            })
            .collect();

        let result = dither.dither(&image, 4, 4, &palette, &options);
        for &idx in &result {
            assert_eq!(idx, 0, "Single-color palette should always return 0");
        }
    }

    #[test]
    fn test_two_color_palette() {
        let palette = make_bw_palette();
        let dither = SimplexDither::new(&palette);
        let options = DitherOptions::new();

        let mid_gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![mid_gray; 64];

        let result = dither.dither(&image, 8, 8, &palette, &options);

        let blacks = result.iter().filter(|&&x| x == 0).count();
        let whites = result.iter().filter(|&&x| x == 1).count();
        assert!(
            blacks > 0 && whites > 0,
            "Mid-gray should dither to mix: {} blacks, {} whites",
            blacks,
            whites
        );
    }

    #[test]
    fn test_three_color_palette() {
        let palette = make_3_color_palette();
        let dither = SimplexDither::new(&palette);
        let options = DitherOptions::new();

        // Red pixel should map to red
        let red = LinearRgb::from(Srgb::from_u8(255, 0, 0));
        let result = dither.dither(&[red], 1, 1, &palette, &options);
        assert_eq!(result[0], 2, "Pure red should map to index 2");
    }

    #[test]
    fn test_deterministic() {
        let palette = make_6_color_palette();
        let dither = SimplexDither::new(&palette);
        let options = DitherOptions::new();

        let mid_gray = LinearRgb::new(0.5, 0.5, 0.5);
        let image = vec![mid_gray; 64];

        let r1 = dither.dither(&image, 8, 8, &palette, &options);
        let r2 = dither.dither(&image, 8, 8, &palette, &options);
        assert_eq!(r1, r2, "Same input should produce same output");
    }

    #[test]
    fn test_output_in_palette_range() {
        let palette = make_6_color_palette();
        let dither = SimplexDither::new(&palette);
        let options = DitherOptions::new();

        let image: Vec<LinearRgb> = (0..100)
            .map(|i| {
                let r = ((i * 7) % 256) as f32 / 255.0;
                let g = ((i * 13) % 256) as f32 / 255.0;
                let b = ((i * 23) % 256) as f32 / 255.0;
                LinearRgb::from(Srgb::new(r, g, b))
            })
            .collect();

        let result = dither.dither(&image, 10, 10, &palette, &options);

        for (i, &idx) in result.iter().enumerate() {
            assert!(
                (idx as usize) < palette.len(),
                "Index {} at position {} exceeds palette size {}",
                idx,
                i,
                palette.len()
            );
        }
    }

    #[test]
    fn test_exact_match_preserved() {
        let palette = make_bw_palette();
        let dither = SimplexDither::new(&palette);
        let options = DitherOptions::new().preserve_exact_matches(true);

        let black = LinearRgb::from(Srgb::from_u8(0, 0, 0));
        let image = vec![black; 16];

        let result = dither.dither(&image, 4, 4, &palette, &options);
        for &idx in &result {
            assert_eq!(idx, 0, "Exact black should map to index 0");
        }
    }

    #[test]
    fn test_empty_image() {
        let palette = make_bw_palette();
        let dither = SimplexDither::new(&palette);
        let options = DitherOptions::new();
        let image: Vec<LinearRgb> = vec![];

        let result = dither.dither(&image, 0, 0, &palette, &options);
        assert!(result.is_empty(), "Empty input should produce empty output");
    }

    #[test]
    fn test_grey_gradient_no_chromatic_noise() {
        use crate::DistanceMetric;

        let colors = [
            Srgb::from_u8(0, 0, 0),
            Srgb::from_u8(255, 255, 255),
            Srgb::from_u8(255, 0, 0),
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(0, 0, 255),
            Srgb::from_u8(255, 255, 0),
        ];
        let palette =
            Palette::new(&colors, None)
                .unwrap()
                .with_distance_metric(DistanceMetric::HyAB {
                    kl: 2.0,
                    kc: 1.0,
                    kchroma: 10.0,
                });
        let dither = SimplexDither::new(&palette);
        let options = DitherOptions::new();

        // Grey gradient (64x64)
        let image: Vec<LinearRgb> = (0..64 * 64)
            .map(|i| {
                let v = (i % 64) as f32 / 63.0;
                LinearRgb::from(Srgb::new(v, v, v))
            })
            .collect();

        let result = dither.dither(&image, 64, 64, &palette, &options);

        for (i, &idx) in result.iter().enumerate() {
            assert!(
                idx <= 1,
                "Grey pixel at position {} mapped to chromatic index {} (expected 0 or 1)",
                i,
                idx
            );
        }
    }

    #[test]
    fn test_circumsphere_known_case() {
        // Four points forming a regular tetrahedron-like shape
        let p0 = [0.0, 0.0, 0.0];
        let p1 = [1.0, 0.0, 0.0];
        let p2 = [0.5, 0.866, 0.0];
        let p3 = [0.5, 0.289, 0.816];

        let result = circumsphere(&p0, &p1, &p2, &p3);
        assert!(result.is_some(), "Should compute circumsphere");

        let (center, _radius_sq) = result.unwrap();
        // All four points should be equidistant from center
        for p in [&p0, &p1, &p2, &p3] {
            let d = (0..3).map(|i| (p[i] - center[i]).powi(2)).sum::<f32>();
            assert!(
                (d - _radius_sq).abs() < 1e-3,
                "Point should be on circumsphere: dist_sq={d}, radius_sq={_radius_sq}"
            );
        }
    }

    #[test]
    fn test_degenerate_coplanar_returns_none() {
        let p0 = [0.0, 0.0, 0.0];
        let p1 = [1.0, 0.0, 0.0];
        let p2 = [0.0, 1.0, 0.0];
        let p3 = [1.0, 1.0, 0.0]; // Coplanar with p0,p1,p2

        let result = circumsphere(&p0, &p1, &p2, &p3);
        assert!(result.is_none(), "Coplanar points should return None");
    }

    #[test]
    fn test_decomposer_builds_for_all_sizes() {
        // Verify construction doesn't panic for any palette size
        for size in [1, 2, 3, 4, 5, 6, 7, 8, 16] {
            let colors: Vec<Srgb> = (0..size)
                .map(|i| {
                    let v = (i * (255 / size.max(1))) as u8;
                    let g = ((i * 97 + 30) % 256) as u8;
                    let b = ((i * 151 + 60) % 256) as u8;
                    Srgb::from_u8(v, g, b)
                })
                .collect();

            if let Ok(palette) = Palette::new(&colors, None) {
                let _decomposer = SimplexDecomposer::build(&palette);
            }
        }
    }
}
