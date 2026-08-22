//! A fan of tetrahedra around the palette's black-white axis.
//!
//! Nearest-neighbour selection picks the single closest ink. On a panel whose
//! chromatic inks are dull they crowd the neutral axis, out-compete black and
//! white on distance, and a muted colour is rendered as a field of one ink
//! with black never chosen at all.
//!
//! Ostromoukhov's construction (SPIE 1909, 1993) partitions the gamut into
//! wedges spanned by two hue-adjacent chromatic inks **plus black and white**,
//! so the black-white segment is a shared edge of every wedge. A neutral's
//! coordinates are then `(w, 0, 0, k)` whichever wedge it lands in, and grey
//! balance is exact by construction rather than by tuning.
//!
//! Delaunay would not do: nothing in the empty-circumsphere criterion
//! guarantees the black-white segment is even an edge.
//!
//! All geometry is in **linear RGB**, where light adds. The palette's colours
//! are not a convex set in OKLab, so the mixture cannot be solved there --
//! `gamut/hull.rs` computes its hull in linear RGB for the same reason.
//!
//! **Out-of-gamut targets are projected, not extrapolated.** The original
//! design solved the barycentric coordinates in every wedge and kept the wedge
//! whose smallest coordinate was largest, on the claim that this "degrades to
//! the nearest wedge" outside the hull. Measured, that claim is false: each
//! wedge's coordinates are an affine map extrapolated to all of R^3, all four
//! wedges return negative minima outside the hull, and which minimum is
//! largest flips as the target moves a fraction of a linear-RGB unit, because
//! the four bases are unrelated to one another away from their shared faces.
//! The continuity walk measured the weight field jumping **0.9137** between
//! the adjacent 8-bit colours sRGB (78,137,196) and (78,137,197) -- roughly 45
//! times the 0.02 bound, at a pair of colours the eye cannot tell apart. This
//! panel's hull is 16.5% of the sRGB cube and nothing upstream guarantees
//! containment, so out-of-hull targets reach [`WedgeFan::weights`] routinely.
//! See [`WedgeFan::weights`] for the rule that replaced it.

use crate::color::LinearRgb;
use crate::gamut::hull::{plane_basis, Hull};
use crate::palette::CHROMA_DETECTION_THRESHOLD;
use crate::Palette;

/// One tetrahedron: black, white, and two hue-adjacent chromatic inks.
#[derive(Debug, Clone)]
struct Wedge {
    /// Palette indices in coordinate order: `[black, white, c0, c1]`.
    inks: [usize; 4],
    /// The four vertices in linear RGB, in the same order as `inks`.
    /// `verts[0]` is black, the origin the other three are measured from.
    verts: [[f32; 3]; 4],
    /// Inverse of the matrix whose columns are `white - black`, `c0 - black`
    /// and `c1 - black`. Multiplying it by `p - black` gives the last three
    /// barycentric coordinates; black's is one minus their sum.
    inv: [[f32; 3]; 3],
}

/// The fan of wedges around a palette's black-white axis.
#[derive(Debug, Clone)]
pub struct WedgeFan {
    wedges: Vec<Wedge>,
    len: usize,
}

/// A wedge whose matrix determinant is below this is treated as flat.
const DET_EPS: f32 = 1e-9;

/// A black-white separation shorter than this leaves no axis to fan about.
const AXIS_EPS: f32 = 1e-6;

/// How far the wedge volumes may fall short of, or overshoot, the hull's
/// volume before the fan is refused. **Relative**, because the quantity it
/// bounds scales with the gamut.
///
/// The two figures it sits between are both measured, not chosen:
///
/// - **The floating-point floor.** The two palettes that do tessellate come
///   out at a relative discrepancy of 1.2e-7 (`panel_measured`) and exactly
///   zero (`six_colour`). That is f32 round-off between two unrelated
///   summations — four determinants against a pyramid decomposition over the
///   hull's facets — and not a geometric defect.
/// - **The smallest real failure.** Swapping two inks in the fan's order so
///   that adjacent wedges overlap loses 48.96% of `panel_measured`'s hull and
///   25.00% of `six_colour`'s. A seven-ink palette whose orange is inside its
///   own gamut rather than a vertex of it loses 26.42%. A tessellation defect
///   is a chunk of the gamut, never a rounding error.
///
/// 1e-3 sits four orders above the noise and two and a half below the smallest
/// defect measured. The risk worth guarding is the *tight* side: a tolerance
/// that silently refused a fan on a real panel would look like the feature
/// simply not working, with nothing to say why.
/// `a_six_ink_panel_yields_four_wedges` and its siblings are the guard
/// against that.
const TESSELLATION_TOLERANCE: f32 = 1e-3;

/// The inverse of `m` and its determinant. `None` when `m` is flat.
///
/// The determinant comes back because the caller needs it: six times the
/// volume of the tetrahedron the three columns span.
fn invert3(m: [[f32; 3]; 3]) -> Option<([[f32; 3]; 3], f32)> {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    if det.abs() < DET_EPS {
        return None;
    }
    let d = 1.0 / det;
    let inv = [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * d,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * d,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * d,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * d,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * d,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * d,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * d,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * d,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * d,
        ],
    ];
    Some((inv, det))
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Barycentric coordinates, over `(a, b, c)`, of the point of that triangle
/// nearest to `p`.
///
/// The textbook case analysis (Ericson, *Real-Time Collision Detection*,
/// §5.1.5): the answer is the perpendicular foot when it falls inside the
/// triangle, and otherwise the nearest point of one of the three edges or
/// three vertices. The returned coordinates are non-negative and sum to one,
/// so no clamping is needed downstream.
///
/// Every divisor here is a squared edge length of the triangle -- `|ab|^2`,
/// `|ac|^2`, `|bc|^2` -- so a non-degenerate triangle cannot divide by zero.
/// `invert3` has already rejected any wedge whose vertices are coplanar.
fn closest_bary_triangle(p: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = sub3(b, a);
    let ac = sub3(c, a);

    let ap = sub3(p, a);
    let d1 = dot3(ab, ap);
    let d2 = dot3(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return [1.0, 0.0, 0.0]; // vertex a
    }

    let bp = sub3(p, b);
    let d3 = dot3(ab, bp);
    let d4 = dot3(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return [0.0, 1.0, 0.0]; // vertex b
    }

    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3); // d1 - d3 == |ab|^2
        return [1.0 - v, v, 0.0]; // edge ab
    }

    let cp = sub3(p, c);
    let d5 = dot3(ab, cp);
    let d6 = dot3(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return [0.0, 0.0, 1.0]; // vertex c
    }

    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6); // d2 - d6 == |ac|^2
        return [1.0 - w, 0.0, w]; // edge ac
    }

    let va = d3 * d6 - d5 * d4;
    let bc0 = d4 - d3;
    let bc1 = d5 - d6;
    if va <= 0.0 && bc0 >= 0.0 && bc1 >= 0.0 {
        let w = bc0 / (bc0 + bc1); // bc0 + bc1 == |bc|^2
        return [0.0, 1.0 - w, w]; // edge bc
    }

    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    [1.0 - v - w, v, w] // the face's interior
}

impl Wedge {
    /// The target's barycentric coordinates in this wedge, in `inks` order:
    /// `[black, white, c0, c1]`. A negative entry means the target lies
    /// outside the tetrahedron, on the far side of the face opposite that ink.
    fn coords(&self, p: [f32; 3]) -> [f32; 4] {
        let o = self.verts[0];
        let d = [p[0] - o[0], p[1] - o[1], p[2] - o[2]];
        let cw = self.inv[0][0] * d[0] + self.inv[0][1] * d[1] + self.inv[0][2] * d[2];
        let c0 = self.inv[1][0] * d[0] + self.inv[1][1] * d[1] + self.inv[1][2] * d[2];
        let c1 = self.inv[2][0] * d[0] + self.inv[2][1] * d[1] + self.inv[2][2] * d[2];
        [1.0 - cw - c0 - c1, cw, c0, c1]
    }

    /// The point of this tetrahedron's **surface** nearest to `p`, as
    /// barycentric coordinates in `inks` order, with its squared distance
    /// from `p`.
    ///
    /// Called only when `p` lies in no wedge at all, so the nearest point of
    /// the solid tetrahedron is on its boundary and the four triangular faces
    /// are the whole search.
    fn closest_on_surface(&self, p: [f32; 3]) -> ([f32; 4], f32) {
        /// Face `i` is the triangle that omits vertex `i`.
        const FACES: [[usize; 3]; 4] = [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]];

        let mut best = [0.0f32; 4];
        let mut best_d2 = f32::INFINITY;
        for face in &FACES {
            let bary = closest_bary_triangle(
                p,
                self.verts[face[0]],
                self.verts[face[1]],
                self.verts[face[2]],
            );
            let mut coords = [0.0f32; 4];
            let mut q = [0.0f32; 3];
            for (slot, &v) in face.iter().enumerate() {
                coords[v] = bary[slot];
                q[0] += bary[slot] * self.verts[v][0];
                q[1] += bary[slot] * self.verts[v][1];
                q[2] += bary[slot] * self.verts[v][2];
            }
            let diff = sub3(p, q);
            let d2 = dot3(diff, diff);
            if d2 < best_d2 {
                best_d2 = d2;
                best = coords;
            }
        }
        (best, best_d2)
    }
}

impl WedgeFan {
    /// Build the fan, or `None` when the palette cannot carry one.
    ///
    /// Returns `None` when there are fewer than three chromatic inks
    /// (greyscale, black-and-white, a single spot colour), when black and
    /// white coincide, when the hull is not mappable, when any wedge comes out
    /// flat, **or when the wedges do not tessellate the hull**. Callers fall
    /// back to plain nearest-neighbour selection, which is today's behaviour
    /// bit for bit.
    ///
    /// `Hull::is_mappable()` is `shape == Volume && neutral_found` — exactly
    /// "a full 3-D hull in which a reachable neutral was found", which is the
    /// condition under which a fan around the black-white axis means anything.
    ///
    /// **The tessellation condition, and why it is checked rather than
    /// assumed.** [`WedgeFan::weights`] projects an out-of-gamut target onto
    /// the nearest wedge, and that is only the same thing as projecting onto
    /// the hull — the thing that makes the weight field continuous — if the
    /// wedges fill the hull and do not overlap. For six inks it comes down to
    /// the chromatic inks being in convex position as seen along the
    /// black–white axis: a palette with a dull ink that is not a vertex of its
    /// own gamut fails it. Every wedge vertex is a palette point, so the fan
    /// is inside the hull unconditionally, and the whole condition is
    /// therefore one volume identity: the tetrahedra's volumes must add up to
    /// [`Hull::volume`]. That is checked below on the determinants `invert3`
    /// already computes.
    ///
    /// A palette that fails it gets no fan and renders exactly as it does
    /// today. Shipping the fan anyway would contour smooth saturated areas —
    /// the symptom would look like a rendering bug, not like a palette
    /// problem, which is why this returns `None` instead of trusting the
    /// caller to have supplied a well-shaped panel.
    pub fn from_palette(palette: &Palette) -> Option<Self> {
        let len = palette.len();
        if len < 5 {
            return None;
        }

        let mut white = 0usize;
        let mut black = 0usize;
        for i in 1..len {
            if palette.actual_oklab(i).l > palette.actual_oklab(white).l {
                white = i;
            }
            if palette.actual_oklab(i).l < palette.actual_oklab(black).l {
                black = i;
            }
        }
        if white == black {
            return None;
        }

        let vert = |i: usize| {
            let c = palette.actual_linear(i);
            [c.r, c.g, c.b]
        };
        let origin = vert(black);
        let edge = |i: usize| sub3(vert(i), origin);
        let e_white = edge(white);

        // The axis the fan turns about, and an orthonormal basis of the plane
        // across it. Two entries with the same linear-RGB colour would leave
        // no axis to turn about.
        let axis_len = dot3(e_white, e_white).sqrt();
        if axis_len < AXIS_EPS {
            return None;
        }
        let axis = [
            e_white[0] / axis_len,
            e_white[1] / axis_len,
            e_white[2] / axis_len,
        ];
        let (basis_u, basis_v) = plane_basis(axis);

        // Chromatic inks, ordered by their **dihedral angle about the
        // black-white line in linear RGB** — the angle the wedges actually
        // partition, since a wedge is two inks joined to that line. Sorting by
        // OKLab hue instead would be sorting in a space the partition knows
        // nothing about: OKLab's per-channel cube root is monotone in neither
        // direction with respect to this angle, so for inks of very different
        // lightness or low chroma the two cyclic orders can disagree, and
        // where they disagree adjacent wedges overlap and leave a gap. Spec
        // §3.1's "sort them by OKLab hue angle" is superseded by the plan's
        // own rule that mixture geometry is done in linear RGB; choosing the
        // partition is mixture geometry.
        //
        // Whether an ink *is* chromatic stays an OKLab question — that is a
        // perceptual judgement, and `CHROMA_DETECTION_THRESHOLD` is the
        // palette's own single source of truth for it.
        //
        // `atan2` returns (-pi, pi]; sorting on it walks the circle once, and
        // the last pair wraps to the first.
        let mut chromatic: Vec<(f32, usize)> = (0..len)
            .filter(|&i| i != white && i != black)
            .filter_map(|i| {
                let c = palette.actual_oklab(i);
                let chroma = (c.a * c.a + c.b * c.b).sqrt();
                if chroma <= CHROMA_DETECTION_THRESHOLD {
                    return None;
                }
                let d = edge(i);
                let along = dot3(d, axis);
                let perp = [
                    d[0] - along * axis[0],
                    d[1] - along * axis[1],
                    d[2] - along * axis[2],
                ];
                Some((dot3(perp, basis_v).atan2(dot3(perp, basis_u)), i))
            })
            .collect();
        if chromatic.len() < 3 {
            return None;
        }
        chromatic.sort_by(|a, b| a.0.total_cmp(&b.0));

        let hull = Hull::from_palette(palette);
        if !hull.is_mappable() {
            return None;
        }

        let mut wedges = Vec::with_capacity(chromatic.len());
        let mut fan_volume = 0.0f32;
        for pair in 0..chromatic.len() {
            let c0 = chromatic[pair].1;
            let c1 = chromatic[(pair + 1) % chromatic.len()].1;
            let e0 = edge(c0);
            let e1 = edge(c1);
            // Rows are the coordinate axes; columns are the edge vectors.
            let m = [
                [e_white[0], e0[0], e1[0]],
                [e_white[1], e0[1], e1[1]],
                [e_white[2], e0[2], e1[2]],
            ];
            let (inv, det) = invert3(m)?;
            fan_volume += det.abs() / 6.0;
            wedges.push(Wedge {
                inks: [black, white, c0, c1],
                verts: [origin, vert(white), vert(c0), vert(c1)],
                inv,
            });
        }

        // The tessellation check. The fan is inside the hull by construction,
        // so equal volumes mean it fills the hull *and* no two wedges overlap.
        let hull_volume = hull.volume();
        if (fan_volume - hull_volume).abs() > TESSELLATION_TOLERANCE * hull_volume {
            return None;
        }

        Some(Self { wedges, len })
    }

    /// How many wedges the fan holds. One per hue-adjacent pair.
    pub fn wedge_count(&self) -> usize {
        self.wedges.len()
    }

    /// The mixture that reproduces `target`, one weight per palette entry.
    ///
    /// Weights are non-negative and sum to one. `out.len()` must equal the
    /// palette's length.
    ///
    /// **Inside the fan** the containing wedge is kept and its barycentric
    /// coordinates are the weights, exactly reproducing `target`.
    ///
    /// **Outside the fan** — an out-of-gamut colour, which the error-loaded
    /// path produces routinely on a panel whose hull is a sixth of the sRGB
    /// cube — the target is *projected*: for each wedge take the point of that
    /// solid tetrahedron nearest to the target, and keep the wedge whose
    /// nearest point is nearest. Those coordinates are already non-negative
    /// and sum to one, so no clamp-and-renormalise step is needed.
    ///
    /// **Why projection and not extrapolation.** The wedges tessellate the
    /// palette's convex hull — `from_palette` refuses to build a fan at all
    /// unless the wedge volumes add up to the hull's, so this is guaranteed
    /// and not assumed — so the smallest of the four wedge distances *is* the
    /// distance to the hull. The
    /// nearest point in a convex set is unique and 1-Lipschitz in the target.
    /// So wherever the winning wedge changes, both winners realise that same
    /// unique point; it lies in the intersection of the two tetrahedra, hence
    /// on a shared face; and on a shared face the two wedges return identical
    /// weights, because the absent ink's coordinate is exactly zero from both
    /// sides. The field is therefore continuous everywhere, in the hull and
    /// out of it. The rule this replaced — keep the wedge whose smallest
    /// extrapolated coordinate is largest — is discontinuous outside the hull
    /// and was measured jumping 0.9137 in one 8-bit step; see the module doc.
    ///
    /// **Cost.** The four coordinate sets are computed first with the existing
    /// matrix multiplies, and a wedge that contains the target wins at
    /// distance zero, so in-gamut content never runs the projection at all.
    /// Only out-of-hull pixels pay for the sixteen point-triangle cases.
    pub fn weights(&self, target: LinearRgb, out: &mut [f32]) {
        debug_assert_eq!(out.len(), self.len);
        out.fill(0.0);

        let p = [target.r, target.g, target.b];
        let mut best_coords = [0.0f32; 4];
        let mut best_inks = self.wedges[0].inks;
        let mut found = false;

        // Fast path: a wedge that contains the target is its own nearest
        // point, at distance zero, and no other wedge can beat that.
        for w in &self.wedges {
            let c = w.coords(p);
            if c.iter().all(|&x| x >= 0.0) {
                best_coords = c;
                best_inks = w.inks;
                found = true;
                break;
            }
        }

        // Slow path: the target is outside the hull. Project onto each wedge
        // and keep the closest.
        if !found {
            let mut best_d2 = f32::INFINITY;
            for w in &self.wedges {
                let (coords, d2) = w.closest_on_surface(p);
                if d2 < best_d2 {
                    best_d2 = d2;
                    best_coords = coords;
                    best_inks = w.inks;
                }
            }
        }

        // Both paths yield non-negative coordinates summing to one; the clamp
        // and the division only absorb floating-point drift.
        let mut sum = 0.0f32;
        for c in best_coords.iter_mut() {
            if *c < 0.0 {
                *c = 0.0;
            }
            sum += *c;
        }
        if sum <= 0.0 {
            return;
        }
        for (slot, &ink) in best_inks.iter().enumerate() {
            out[ink] = best_coords[slot] / sum;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Only the tests name `Oklab`; importing it at module scope would trip
    // `clippy -D warnings` on an unused import.
    use crate::color::Oklab;
    use crate::gamut::test_support::{four_grey, panel_measured, six_colour};
    use crate::Srgb;

    fn weights_of(fan: &WedgeFan, palette: &Palette, c: Srgb) -> Vec<f32> {
        let mut w = vec![0.0; palette.len()];
        fan.weights(LinearRgb::from(c), &mut w);
        w
    }

    /// The colour a mixture actually makes: the weighted sum of the inks in
    /// linear RGB, which is what physical dot mixing does.
    fn reconstruct(palette: &Palette, w: &[f32]) -> LinearRgb {
        let mut acc = [0.0f32; 3];
        for (i, &weight) in w.iter().enumerate() {
            let c = palette.actual_linear(i);
            acc[0] += weight * c.r;
            acc[1] += weight * c.g;
            acc[2] += weight * c.b;
        }
        LinearRgb::new(acc[0], acc[1], acc[2])
    }

    fn distance(a: LinearRgb, b: LinearRgb) -> f32 {
        let d = [a.r - b.r, a.g - b.g, a.b - b.b];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }

    /// Six inks, four hue-adjacent pairs, four wedges.
    #[test]
    fn a_six_ink_panel_yields_four_wedges() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).expect("six-ink panel carries a fan");
        assert_eq!(fan.wedge_count(), 4);
    }

    /// A greyscale palette has no hue circle to fan around.
    #[test]
    fn a_greyscale_palette_carries_no_fan() {
        assert!(WedgeFan::from_palette(&four_grey()).is_none());
    }

    /// A palette whose inks do not tessellate their own gamut carries no fan.
    ///
    /// Seven-ink ACeP-class panels add an orange to the six inks, and a dull
    /// one lands *inside* the hull of the other six rather than being a vertex
    /// of it — here 0x9A6A2E against the measured six. The fan built around it
    /// then dents inward: 0.09483618 against a hull of 0.12888747, a 26.42%
    /// shortfall. Every premise of the out-of-gamut projection fails on such a
    /// palette, so `from_palette` declines and the caller falls back to plain
    /// nearest-neighbour, which is today's behaviour. The alternative would be
    /// contouring in smooth saturated areas with nothing to explain it.
    #[test]
    fn a_palette_whose_inks_do_not_tessellate_carries_no_fan() {
        let seven = Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(255, 255, 0),
                Srgb::from_u8(0, 0, 255),
                Srgb::from_u8(0, 255, 0),
                Srgb::from_u8(255, 128, 0),
            ],
            Some(&[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 255, 255),
                Srgb::from_u8(0xB5, 0x03, 0x03),
                Srgb::from_u8(0xFF, 0xEE, 0x00),
                Srgb::from_u8(0x20, 0x54, 0x97),
                Srgb::from_u8(0x0D, 0x87, 0x6B),
                Srgb::from_u8(0x9A, 0x6A, 0x2E),
            ]),
        )
        .unwrap();

        // The fixture is only honest if the orange really is interior; the
        // first six inks are `panel_measured`, so its hull is the comparison.
        assert!(
            Hull::from_palette(&panel_measured()).contains(seven.actual_linear(6)),
            "fixture broken: the orange must lie inside the six-ink hull"
        );
        // It fails the volume identity, not one of the counting conditions.
        assert!(
            Hull::from_palette(&seven).is_mappable(),
            "fixture broken: this palette must clear every other condition"
        );

        assert!(
            WedgeFan::from_palette(&seven).is_none(),
            "a fan that does not tessellate its hull must be refused"
        );
    }

    /// The whole point: a neutral's recipe is black and white only.
    #[test]
    fn a_neutral_recruits_only_black_and_white() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        for level in [32u8, 64, 96, 128, 160, 192, 224] {
            let w = weights_of(&fan, &p, Srgb::from_u8(level, level, level));
            let chromatic: f32 = w[2..].iter().sum();
            assert!(
                chromatic < 1e-4,
                "grey {level} recruited {:.4} of chromatic ink: {w:?}",
                chromatic
            );
            assert!((w[0] + w[1] - 1.0).abs() < 1e-4, "grey {level}: {w:?}");
        }
    }

    /// Grey 128 on this panel is 26% white and 74% black, because measured
    /// white is 0.7157 in linear light. The fan must produce that split.
    #[test]
    fn the_neutral_split_matches_the_closed_form() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        let w = weights_of(&fan, &p, Srgb::from_u8(128, 128, 128));
        let target = LinearRgb::from(Srgb::from_u8(128, 128, 128)).r;
        let white = p.actual_linear(1).r;
        let expected_white = target / white;
        assert!(
            (w[1] - expected_white).abs() < 0.01,
            "white share {:.3}, closed form {:.3}",
            w[1],
            expected_white
        );
    }

    /// An ink is its own pure recipe.
    #[test]
    fn every_ink_is_its_own_recipe() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        for i in 0..p.len() {
            let mut w = vec![0.0; p.len()];
            fan.weights(p.actual_linear(i), &mut w);
            assert!(w[i] > 0.99, "ink {i} weighted {:.3} on itself: {w:?}", w[i]);
        }
    }

    /// Weights are a mixture: non-negative and summing to one.
    #[test]
    fn weights_are_a_normalised_mixture() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        for r in (0..=255u32).step_by(51) {
            for g in (0..=255u32).step_by(51) {
                for b in (0..=255u32).step_by(51) {
                    let w = weights_of(&fan, &p, Srgb::from_u8(r as u8, g as u8, b as u8));
                    let sum: f32 = w.iter().sum();
                    assert!((sum - 1.0).abs() < 1e-3, "sum {sum} at {r},{g},{b}");
                    assert!(w.iter().all(|&x| x >= 0.0), "negative weight: {w:?}");
                }
            }
        }
    }

    /// The field must be continuous, or it draws contours in smooth content.
    ///
    /// `tests/spike_simplex.rs` died of exactly this: it restricted candidates
    /// to the support of a *freely optimised* mixture and measured its own
    /// support field jumping 0.090 in one step, against its note that "a
    /// smooth field would stay well under 0.1". A fixed fan has no such jump,
    /// because black and white are in every cell and never drop out, and two
    /// neighbouring wedges agree exactly on their shared face.
    ///
    /// **A third of this walk is out of gamut, and that is the point.** This
    /// panel's hull is a sixth of the sRGB cube, so the ramps and hue sweeps
    /// below spend 813 of their 2400 probes outside it — the assertion on
    /// `out_of_hull` below pins that down so it stays deliberate rather than
    /// accidental. Out of gamut is exactly where the specified rule failed:
    /// selecting the wedge whose smallest *extrapolated* barycentric
    /// coordinate is largest made the field jump 0.9137 between sRGB
    /// (78,137,196) and (78,137,197). An in-gamut-only walk would have passed
    /// and shipped a quantiser that contours every smooth saturated area.
    #[test]
    fn the_weight_field_is_continuous() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        let hull = Hull::from_palette(&p);
        let mut worst = 0.0f32;
        let mut worst_at = String::new();
        let mut probes = 0u32;
        let mut out_of_hull = 0u32;

        let mut probe = |a: Srgb, b: Srgb, label: String| {
            let wa = weights_of(&fan, &p, a);
            let wb = weights_of(&fan, &p, b);
            let step = wa
                .iter()
                .zip(&wb)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0f32, f32::max);
            if step > worst {
                worst = step;
                worst_at = label;
            }
            probes += 1;
            if !hull.contains(LinearRgb::from(a)) {
                out_of_hull += 1;
            }
        };

        // Lightness ramps at several hues, one 8-bit step apart.
        for &(dr, dg, db) in &[
            (1.0, 1.0, 1.0),
            (1.0, 0.6, 0.5),
            (0.4, 0.7, 1.0),
            (0.6, 1.0, 0.6),
        ] {
            for v in 8..248u32 {
                let mk = |t: u32| {
                    Srgb::from_u8(
                        (t as f32 * dr) as u8,
                        (t as f32 * dg) as u8,
                        (t as f32 * db) as u8,
                    )
                };
                probe(mk(v), mk(v + 1), format!("ramp {dr},{dg},{db} at {v}"));
            }
        }

        // Hue sweeps at several lightnesses, one degree apart.
        for &l in &[0.25f32, 0.4, 0.55, 0.7] {
            for deg in 0..360u32 {
                let mk = |d: u32| {
                    let rad = (d as f32).to_radians();
                    let c = 0.06f32;
                    let lab = Oklab::new(l, c * rad.cos(), c * rad.sin());
                    // Ruling 5 (see `gamut/mapper.rs`): `linear_to_srgb` carries
                    // an epsilon-free `debug_assert!`, and this synthetic hue
                    // sweep goes out of sRGB gamut for part of the circle, so
                    // conversion rounding must be clamped away before it lands.
                    let lin = LinearRgb::from(lab);
                    Srgb::from(LinearRgb::new(
                        lin.r.clamp(0.0, 1.0),
                        lin.g.clamp(0.0, 1.0),
                        lin.b.clamp(0.0, 1.0),
                    ))
                };
                probe(mk(deg), mk((deg + 1) % 360), format!("hue L={l} at {deg}"));
            }
        }

        println!("largest single-step weight change: {worst:.6} at {worst_at}");
        println!("probes: {probes}, of which outside the hull: {out_of_hull}");
        assert!(
            out_of_hull * 4 > probes,
            "only {out_of_hull} of {probes} probes were outside the hull — \
             this walk must exercise the projection, not just the fan interior"
        );
        // Measured 0.019047, at the (0.4,0.7,1.0) ramp stepping v=148 -> 149.
        // That is not a jump: an 8-bit step there moves about 0.0057 in linear
        // light, and the barycentric coordinates of a tetrahedron whose edges
        // are a few tenths of a unit long vary that fast smoothly.
        assert!(
            worst < 0.020,
            "weight field jumps {worst:.6} in one step at {worst_at} — \
             a discontinuous field draws contours in smooth content"
        );
    }

    /// The premise the out-of-gamut continuity proof rests on.
    ///
    /// The proof that [`WedgeFan::weights`] is continuous outside the hull
    /// needs the four wedges to *tessellate* that hull: their union must be
    /// the whole hull (so the smallest wedge distance is the hull distance,
    /// and the hull is convex, so the nearest point is unique) and their
    /// interiors must be disjoint (so no target is claimed by two wedges that
    /// disagree). For six inks it comes down to the chromatic quadrilateral
    /// being convex as seen along the black-white axis. That is now genuinely
    /// a property of the palette and of nothing else: `from_palette` orders
    /// the inks by their dihedral angle about that very axis, in linear RGB,
    /// so this code cannot introduce a violation by ordering them in the wrong
    /// space; and it refuses to build a fan at all unless the wedge volumes
    /// add up to [`Hull::volume`], so a palette that violates it gets no fan
    /// rather than a silently discontinuous one.
    ///
    /// This test is the independent check on that enforcement: the volume
    /// identity in `from_palette` is one number, and a grid walk is a
    /// different kind of evidence for the same claim. Both shipped palettes
    /// pass both. (`from_palette`'s own figures: a relative volume discrepancy
    /// of 1.2e-7 for `panel_measured` and exactly zero for `six_colour`.)
    #[test]
    fn the_wedges_tessellate_the_hull() {
        /// Barycentric slack, roughly 3e-5 in linear-RGB distance on these
        /// tetrahedra — the same order as `Hull`'s own containment epsilon.
        const TOL: f32 = 1e-4;
        const N: u32 = 48;

        for (name, p) in [
            ("panel_measured", panel_measured()),
            ("six_colour", six_colour()),
        ] {
            let fan = WedgeFan::from_palette(&p).unwrap();
            let hull = Hull::from_palette(&p);

            let mut lo = [f32::MAX; 3];
            let mut hi = [f32::MIN; 3];
            for i in 0..p.len() {
                let c = p.actual_linear(i);
                for (k, v) in [c.r, c.g, c.b].iter().enumerate() {
                    lo[k] = lo[k].min(*v);
                    hi[k] = hi[k].max(*v);
                }
            }

            let mut in_hull = 0u32;
            let mut uncovered = 0u32;
            let mut overlapping = 0u32;
            for ir in 0..N {
                for ig in 0..N {
                    for ib in 0..N {
                        // Half-cell offset, so the grid misses the
                        // axis-aligned faces of `six_colour`'s hull.
                        let at = |i: u32, k: usize| {
                            lo[k] + (hi[k] - lo[k]) * (i as f32 + 0.5) / N as f32
                        };
                        let q = [at(ir, 0), at(ig, 1), at(ib, 2)];

                        let mut covered = false;
                        let mut strictly_inside = 0u32;
                        for w in &fan.wedges {
                            let c = w.coords(q);
                            if c.iter().all(|&x| x >= -TOL) {
                                covered = true;
                            }
                            if c.iter().all(|&x| x > TOL) {
                                strictly_inside += 1;
                            }
                        }
                        if strictly_inside > 1 {
                            overlapping += 1;
                        }
                        if hull.contains(LinearRgb::new(q[0], q[1], q[2])) {
                            in_hull += 1;
                            if !covered {
                                uncovered += 1;
                            }
                        }
                    }
                }
            }

            println!("{name}: {in_hull} grid points in the hull, {uncovered} of them in no wedge, {overlapping} in two");
            assert!(in_hull > 10_000, "{name}: the grid barely met the hull");
            assert_eq!(uncovered, 0, "{name}: the wedges do not fill the hull");
            assert_eq!(overlapping, 0, "{name}: two wedges claim the same volume");
        }
    }

    /// A target inside the hull is reproduced exactly by its own weights.
    ///
    /// This is what keeps the feature inert at `lambda = 0`: inside the hull
    /// the containing wedge wins at distance zero and returns the same affine
    /// coordinates the extrapolating rule returned, so the projection changes
    /// nothing that was already right.
    #[test]
    fn an_interior_target_reconstructs_to_itself() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        let hull = Hull::from_palette(&p);

        // Neutrals, plus mixtures built from the inks themselves so they are
        // interior by construction.
        let mut targets: Vec<LinearRgb> = [64u8, 96, 128, 160, 192]
            .iter()
            .map(|&v| LinearRgb::from(Srgb::from_u8(v, v, v)))
            .collect();
        for mix in [
            [0.30f32, 0.30, 0.20, 0.10, 0.05, 0.05],
            [0.45, 0.15, 0.05, 0.15, 0.10, 0.10],
            [0.10, 0.40, 0.15, 0.05, 0.20, 0.10],
        ] {
            targets.push(reconstruct(&p, &mix));
        }

        let mut w = vec![0.0f32; p.len()];
        for t in targets {
            assert!(hull.contains(t), "target {t:?} was supposed to be in gamut");
            fan.weights(t, &mut w);
            let rec = reconstruct(&p, &w);
            assert!(
                distance(rec, t) < 1e-5,
                "interior target {t:?} came back as {rec:?} from {w:?}"
            );
        }
    }

    /// An out-of-hull target is projected onto the hull, not extrapolated.
    ///
    /// The returned weights must reconstruct a point that lies on the hull's
    /// boundary — inside the hull, but leaving it the moment you step further
    /// towards the target — and that point must be at least as close to the
    /// target as any single ink, since every ink is itself in the hull.
    #[test]
    fn an_out_of_hull_target_projects_onto_the_hull_boundary() {
        let p = panel_measured();
        let fan = WedgeFan::from_palette(&p).unwrap();
        let hull = Hull::from_palette(&p);

        let targets = [
            Srgb::from_u8(0, 255, 0),
            Srgb::from_u8(255, 0, 255),
            Srgb::from_u8(0, 255, 255),
            Srgb::from_u8(255, 128, 0),
            // The two adjacent colours the extrapolating rule jumped between.
            Srgb::from_u8(78, 137, 196),
            Srgb::from_u8(78, 137, 197),
        ];

        for t in targets {
            let lin = LinearRgb::from(t);
            assert!(!hull.contains(lin), "{t:?} was supposed to be out of gamut");

            let w = weights_of(&fan, &p, t);
            let rec = reconstruct(&p, &w);
            let d = distance(rec, lin);

            assert!(
                hull.contains(rec),
                "{t:?} projected to {rec:?}, outside the hull"
            );
            assert!(
                d > 1e-4,
                "{t:?} projected onto itself, so it was not outside"
            );

            // A short step past the projection, towards the target, must leave
            // the hull — that is what makes `rec` a boundary point.
            let past = LinearRgb::new(
                rec.r + (lin.r - rec.r) / d * 1e-3,
                rec.g + (lin.g - rec.g) / d * 1e-3,
                rec.b + (lin.b - rec.b) / d * 1e-3,
            );
            assert!(
                !hull.contains(past),
                "{t:?} projected to {rec:?}, which is not on the hull boundary"
            );

            for i in 0..p.len() {
                let ink = distance(p.actual_linear(i), lin);
                assert!(
                    d <= ink + 1e-5,
                    "{t:?} projected {d:.5} away, further than ink {i} at {ink:.5}"
                );
            }
        }
    }
}
