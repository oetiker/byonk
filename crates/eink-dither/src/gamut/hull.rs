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
    facets: Vec<Facet>,
    shape: HullShape,
    l_min: f32,
    l_max: f32,
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
            facets,
            shape,
            l_min: 0.0,
            l_max: 1.0,
        };
        let (l_min, l_max) = hull.compute_lightness_range(&pts);
        hull.l_min = l_min;
        hull.l_max = l_max;
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

    /// The Oklab lightness range reachable on the achromatic axis.
    pub fn lightness_range(&self) -> (f32, f32) {
        (self.l_min, self.l_max)
    }

    /// Binary-search the grey axis for the darkest and lightest neutral inside
    /// the hull. For a degenerate hull, fall back to the palette points' own L
    /// range, which is exactly right for a greyscale ramp.
    fn compute_lightness_range(&self, pts: &[[f32; 3]]) -> (f32, f32) {
        if self.shape != HullShape::Volume {
            let mut lo = f32::MAX;
            let mut hi = f32::MIN;
            for p in pts {
                let l = Oklab::from(LinearRgb::new(p[0], p[1], p[2])).l;
                lo = lo.min(l);
                hi = hi.max(l);
            }
            return (lo, hi);
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
            // No neutral is reachable at all. Degenerate for our purposes.
            return (0.0, 1.0);
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
        (lo_in, hi_in)
    }
}

/// Determine whether the points span a volume, a plane, or a line.
fn classify(pts: &[[f32; 3]]) -> HullShape {
    if pts.len() < 3 {
        return HullShape::Line;
    }
    let p0 = pts[0];
    // First independent direction.
    let Some(u) = pts
        .iter()
        .map(|p| sub(*p, p0))
        .find(|v| norm(*v) > EPS)
    else {
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
    use crate::{LinearRgb, Srgb};

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
        assert!(hull.contains(LinearRgb::new(c[0], c[1], c[2])), "centroid must be inside");
        assert!(!hull.contains(LinearRgb::new(5.0, -3.0, 2.0)), "far exterior must be outside");
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
    }
}
