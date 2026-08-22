//! Convex hull of the palette's actual colours in linear RGB.
//!
//! A dithered patch's average is by construction a convex combination of the
//! palette's actual colours **in linear RGB** — that is where light adds. So
//! the convex hull of those colours bounds what any error-diffusion algorithm
//! can reproduce. The set is *not* convex in Oklab, which is why the hull
//! cannot be computed in perceptual space.
//!
//! With at most 16 palette entries, enumerating all point triples and keeping
//! those whose plane has every other point on one side is exact and costs
//! under a millisecond. It runs once when the palette resolves.

use crate::{LinearRgb, Oklab, Palette};

/// Tolerance for plane-side tests, in linear-RGB units.
const EPS: f32 = 1e-5;

/// Dimensionality of the palette's point set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HullShape {
    /// Full 3-D body — the normal case for a chromatic panel.
    Volume,
    /// All points collinear — a greyscale palette. No chroma is reachable.
    Line,
    /// Coplanar but not collinear. Vanishingly unlikely in practice; callers
    /// treat it as "do not map" rather than guessing.
    Plane,
}

/// An outward-oriented facet: every palette point satisfies `n · p <= d`.
#[derive(Debug, Clone, Copy)]
struct Facet {
    n: [f32; 3],
    d: f32,
}

/// The convex hull of a palette's actual colours in linear RGB.
#[derive(Debug, Clone)]
pub struct Hull {
    /// The palette's actual colours in linear RGB — the points the hull was
    /// built from. Kept so [`Hull::volume`] can measure each facet's extent.
    pts: Vec<[f32; 3]>,
    facets: Vec<Facet>,
    shape: HullShape,
    l_min: f32,
    l_max: f32,
    neutral_found: bool,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn norm(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

impl Hull {
    /// Build the hull from the colours the ditherer actually targets.
    pub fn from_palette(palette: &Palette) -> Self {
        let pts: Vec<[f32; 3]> = (0..palette.len())
            .map(|i| {
                let c = palette.actual_linear(i);
                [c.r, c.g, c.b]
            })
            .collect();

        let shape = classify(&pts);
        let facets = if shape == HullShape::Volume {
            enumerate_facets(&pts)
        } else {
            Vec::new()
        };

        let mut hull = Self {
            pts: pts.clone(),
            facets,
            shape,
            l_min: 0.0,
            l_max: 1.0,
            neutral_found: false,
        };
        let (l_min, l_max, neutral_found) = hull.compute_lightness_range(&pts);
        hull.l_min = l_min;
        hull.l_max = l_max;
        hull.neutral_found = neutral_found;
        hull
    }

    /// Dimensionality of the point set.
    pub fn shape(&self) -> HullShape {
        self.shape
    }

    /// Is this colour inside the hull?
    ///
    /// Always false for a degenerate hull: a point has measure zero against a
    /// line or plane, so no useful membership question can be asked of it.
    /// Callers branch on [`Hull::shape`] before relying on this.
    pub fn contains(&self, p: LinearRgb) -> bool {
        if self.shape != HullShape::Volume {
            return false;
        }
        let q = [p.r, p.g, p.b];
        self.facets.iter().all(|f| dot(f.n, q) <= f.d + EPS)
    }

    /// The volume the hull encloses, in linear-RGB units. Zero for a
    /// degenerate hull, which encloses nothing.
    ///
    /// A convex polytope decomposes into one pyramid per facet, apex at any
    /// interior point: `V = sum over facets of area * height / 3`. The
    /// centroid of the palette's points is a convex combination of them and so
    /// is interior, and `enumerate_facets` already keeps one entry per
    /// *plane*, so each facet is counted once. A facet's area is the area of
    /// the convex hull of the palette points lying on its plane — the 2-D hull
    /// rather than a fan, because a palette point can sit inside a facet
    /// without being one of its corners.
    ///
    /// Callers use this to check that a decomposition of the gamut they built
    /// themselves actually accounts for all of it; see
    /// `WedgeFan::from_palette`.
    pub fn volume(&self) -> f32 {
        if self.shape != HullShape::Volume {
            return 0.0;
        }
        let n_pts = self.pts.len() as f32;
        let mut centre = [0.0f32; 3];
        for p in &self.pts {
            centre[0] += p[0] / n_pts;
            centre[1] += p[1] / n_pts;
            centre[2] += p[2] / n_pts;
        }

        let mut volume = 0.0f32;
        for f in &self.facets {
            let (u, v) = plane_basis(f.n);
            let on_plane: Vec<[f32; 2]> = self
                .pts
                .iter()
                .filter(|p| (dot(f.n, **p) - f.d).abs() <= EPS)
                .map(|p| [dot(u, *p), dot(v, *p)])
                .collect();
            // Normals point outward, so no interior point is ever above a
            // facet and the height cannot come out negative.
            let height = f.d - dot(f.n, centre);
            volume += hull_area_2d(on_plane) * height / 3.0;
        }
        volume
    }

    /// The Oklab lightness range reachable on the achromatic axis.
    pub fn lightness_range(&self) -> (f32, f32) {
        (self.l_min, self.l_max)
    }

    /// Can chroma be mapped through this hull at all?
    ///
    /// True only for a full 3-D hull in which a reachable neutral was
    /// actually found. A degenerate hull, or one whose grey axis lies
    /// entirely outside it, cannot support chroma compression: mapping
    /// through it would crush content onto a lightness the panel cannot
    /// render. Callers decline to map rather than guess.
    pub fn is_mappable(&self) -> bool {
        self.shape == HullShape::Volume && self.neutral_found
    }

    /// Binary-search the grey axis for the darkest and lightest neutral inside
    /// the hull. For a degenerate hull, fall back to the palette points' own L
    /// range, which is exactly right for a greyscale ramp; `neutral_found` is
    /// always false in that case, since a degenerate hull cannot map chroma.
    fn compute_lightness_range(&self, pts: &[[f32; 3]]) -> (f32, f32, bool) {
        if self.shape != HullShape::Volume {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for p in pts {
                let l = Oklab::from(LinearRgb::new(p[0], p[1], p[2])).l;
                lo = lo.min(l);
                hi = hi.max(l);
            }
            return (lo, hi, false);
        }

        let grey_inside = |l: f32| self.contains(LinearRgb::from(Oklab::new(l, 0.0, 0.0)));

        // Find any interior neutral to bracket from.
        let mut seed = None;
        for i in 0..=64 {
            let l = i as f32 / 64.0;
            if grey_inside(l) {
                seed = Some(l);
                break;
            }
        }
        let Some(seed) = seed else {
            // No neutral is reachable: `neutral_found` is false, so
            // `is_mappable()` returns false and callers decline to map. The
            // range returned here is never relied upon; it is a harmless
            // placeholder, not a claim about reachability.
            return (0.0, 1.0, false);
        };

        // Walk down, then up, by bisection.
        let (mut lo_out, mut lo_in) = (0.0f32, seed);
        if grey_inside(0.0) {
            lo_in = 0.0;
        } else {
            for _ in 0..24 {
                let mid = 0.5 * (lo_out + lo_in);
                if grey_inside(mid) {
                    lo_in = mid;
                } else {
                    lo_out = mid;
                }
            }
        }
        let (mut hi_out, mut hi_in) = (1.0f32, seed);
        if grey_inside(1.0) {
            hi_in = 1.0;
        } else {
            for _ in 0..24 {
                let mid = 0.5 * (hi_out + hi_in);
                if grey_inside(mid) {
                    hi_in = mid;
                } else {
                    hi_out = mid;
                }
            }
        }
        (lo_in, hi_in, true)
    }
}

/// An orthonormal basis of the plane orthogonal to the unit vector `n`.
///
/// Cross `n` with whichever coordinate axis it leans on least, so the result
/// is never near-degenerate.
pub(super) fn plane_basis(n: [f32; 3]) -> ([f32; 3], [f32; 3]) {
    let axis = if n[0].abs() <= n[1].abs() && n[0].abs() <= n[2].abs() {
        [1.0, 0.0, 0.0]
    } else if n[1].abs() <= n[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let u = cross(n, axis);
    let len = norm(u);
    let u = [u[0] / len, u[1] / len, u[2] / len];
    (u, cross(n, u))
}

/// Area of the convex hull of a set of 2-D points.
///
/// Andrew's monotone chain: sort lexicographically, sweep once forward for the
/// lower chain and once back for the upper, then close it with the shoelace
/// formula. Points inside the hull are discarded by the sweep, so a palette
/// colour that happens to lie inside a facet does not distort its area.
fn hull_area_2d(mut pts: Vec<[f32; 2]>) -> f32 {
    fn turn(o: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    }
    if pts.len() < 3 {
        return 0.0;
    }
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));

    let mut chain: Vec<[f32; 2]> = Vec::with_capacity(2 * pts.len());
    for &p in pts.iter() {
        while chain.len() >= 2 && turn(chain[chain.len() - 2], chain[chain.len() - 1], p) <= 0.0 {
            chain.pop();
        }
        chain.push(p);
    }
    let lower = chain.len() + 1;
    for &p in pts.iter().rev().skip(1) {
        while chain.len() >= lower && turn(chain[chain.len() - 2], chain[chain.len() - 1], p) <= 0.0
        {
            chain.pop();
        }
        chain.push(p);
    }
    chain.pop(); // the start point, repeated to close the loop

    let mut twice_area = 0.0f32;
    for i in 0..chain.len() {
        let j = (i + 1) % chain.len();
        twice_area += chain[i][0] * chain[j][1] - chain[j][0] * chain[i][1];
    }
    (twice_area * 0.5).abs()
}

/// Determine whether the points span a volume, a plane, or a line.
fn classify(pts: &[[f32; 3]]) -> HullShape {
    if pts.len() < 3 {
        return HullShape::Line;
    }
    let p0 = pts[0];
    // First independent direction.
    let Some(u) = pts.iter().map(|p| sub(*p, p0)).find(|v| norm(*v) > EPS) else {
        return HullShape::Line;
    };
    // Second independent direction: a point off the line through p0 + u.
    let Some(n) = pts
        .iter()
        .map(|p| cross(u, sub(*p, p0)))
        .find(|c| norm(*c) > EPS)
    else {
        return HullShape::Line;
    };
    // Third: a point off that plane.
    let off_plane = pts
        .iter()
        .any(|p| dot(n, sub(*p, p0)).abs() > EPS * norm(n).max(1.0));
    if off_plane {
        HullShape::Volume
    } else {
        HullShape::Plane
    }
}

/// Every triple whose plane has all other points on one side is a hull facet.
/// Normals are oriented outward so that `n · p <= d` holds for every point.
fn enumerate_facets(pts: &[[f32; 3]]) -> Vec<Facet> {
    let n_pts = pts.len();
    let mut facets: Vec<Facet> = Vec::new();

    for i in 0..n_pts {
        for j in (i + 1)..n_pts {
            for k in (j + 1)..n_pts {
                let mut n = cross(sub(pts[j], pts[i]), sub(pts[k], pts[i]));
                let len = norm(n);
                if len < EPS {
                    continue; // collinear triple, no plane
                }
                n = [n[0] / len, n[1] / len, n[2] / len];
                let mut d = dot(n, pts[i]);

                let mut above = false;
                let mut below = false;
                for p in pts {
                    let s = dot(n, *p) - d;
                    if s > EPS {
                        above = true;
                    } else if s < -EPS {
                        below = true;
                    }
                }
                if above && below {
                    continue; // plane cuts through the body
                }
                if above {
                    n = [-n[0], -n[1], -n[2]];
                    d = -d;
                }

                // Skip a plane we already have (coplanar triples repeat).
                let dup = facets.iter().any(|f| {
                    (f.n[0] - n[0]).abs() < 1e-4
                        && (f.n[1] - n[1]).abs() < 1e-4
                        && (f.n[2] - n[2]).abs() < 1e-4
                        && (f.d - d).abs() < 1e-4
                });
                if !dup {
                    facets.push(Facet { n, d });
                }
            }
        }
    }

    facets
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamut::test_support::{four_grey, six_colour};
    use crate::{LinearRgb, Palette, Srgb};

    #[test]
    fn palette_vertices_are_inside_their_own_hull() {
        let p = six_colour();
        let hull = Hull::from_palette(&p);
        assert_eq!(hull.shape(), HullShape::Volume);
        for i in 0..p.len() {
            assert!(
                hull.contains(p.actual_linear(i)),
                "palette entry {i} must lie in its own hull"
            );
        }
    }

    #[test]
    fn centroid_is_inside_and_far_exterior_is_outside() {
        let p = six_colour();
        let hull = Hull::from_palette(&p);
        let mut c = [0.0f32; 3];
        for i in 0..p.len() {
            let e = p.actual_linear(i);
            c[0] += e.r / p.len() as f32;
            c[1] += e.g / p.len() as f32;
            c[2] += e.b / p.len() as f32;
        }
        assert!(
            hull.contains(LinearRgb::new(c[0], c[1], c[2])),
            "centroid must be inside"
        );
        assert!(
            !hull.contains(LinearRgb::new(5.0, -3.0, 2.0)),
            "far exterior must be outside"
        );
    }

    #[test]
    fn cyan_is_outside_a_palette_that_lacks_it() {
        // Pure cyan is not producible by mixing black/white/R/G/B/Y additively
        // at the intensity of full cyan: it sits outside the hull.
        let hull = Hull::from_palette(&six_colour());
        assert!(!hull.contains(LinearRgb::from(Srgb::from_u8(0, 255, 255))));
    }

    #[test]
    fn greyscale_palette_collapses_to_a_line() {
        let hull = Hull::from_palette(&four_grey());
        assert_eq!(hull.shape(), HullShape::Line);
    }

    #[test]
    fn lightness_range_spans_black_to_white() {
        let hull = Hull::from_palette(&six_colour());
        let (lo, hi) = hull.lightness_range();
        assert!(lo < 0.02, "black must be reachable, got {lo}");
        assert!(hi > 0.98, "white must be reachable, got {hi}");
        assert!(hull.is_mappable(), "six_colour hull must be mappable");
    }

    #[test]
    fn a_volume_hull_that_misses_the_grey_axis_is_not_mappable() {
        // All-red inks: every convex combination keeps r far above g and b,
        // so no neutral is reachable even though the hull is a full volume.
        let p = Palette::new(
            &[
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(255, 51, 0),
                Srgb::from_u8(255, 0, 51),
                Srgb::from_u8(204, 26, 26),
            ],
            None,
        )
        .unwrap();
        let hull = Hull::from_palette(&p);
        assert_eq!(
            hull.shape(),
            HullShape::Volume,
            "fixture must be a real volume"
        );
        assert!(
            !hull.is_mappable(),
            "no neutral is reachable, so it must decline"
        );
    }

    /// `volume()` against closed forms, so the pyramid decomposition is
    /// checked and not just self-consistent.
    #[test]
    fn the_hull_volume_matches_the_closed_form() {
        // The unit cube itself: eight corners, volume 1.
        let cube = Palette::new(
            &[
                Srgb::from_u8(0, 0, 0),
                Srgb::from_u8(255, 0, 0),
                Srgb::from_u8(0, 255, 0),
                Srgb::from_u8(0, 0, 255),
                Srgb::from_u8(255, 255, 0),
                Srgb::from_u8(255, 0, 255),
                Srgb::from_u8(0, 255, 255),
                Srgb::from_u8(255, 255, 255),
            ],
            None,
        )
        .unwrap();
        let v = Hull::from_palette(&cube).volume();
        assert!((v - 1.0).abs() < 1e-4, "unit cube measured {v}");

        // `six_colour` is the cube with the cyan and magenta corners removed.
        // Cutting one corner off a unit cube takes 1/6 of it, so 2/3 is left.
        let v = Hull::from_palette(&six_colour()).volume();
        assert!(
            (v - 2.0 / 3.0).abs() < 1e-4,
            "six_colour measured {v}, closed form 0.666667"
        );

        // A degenerate hull encloses nothing.
        assert_eq!(Hull::from_palette(&four_grey()).volume(), 0.0);
    }
}
