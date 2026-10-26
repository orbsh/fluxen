//! 3D force layout: charge repulsion, link springs, centring, damping.
//!
//! Deliberately plain: pairwise O(n²) repulsion, one pass of springs, a weak
//! pull towards the origin, velocity damping, then semi-implicit Euler. The
//! budget this is written for is tens of visible nodes (a 3D view turns to fog
//! long before it turns slow); an octree stays out until a measurement asks
//! for it.
//!
//! Two rules borrowed from the module-era implementation because they were
//! measured, not guessed: the simulation never stops by itself (a layout that
//! freezes mid-relaxation looks broken on the next append), and the loop's
//! `dt` is clamped so a background tab's catch-up frame cannot fling nodes out
//! of the scene.

use crate::math::Vec3;

/// Golden angle (pi * (3 - sqrt(5))), written out because `sqrt` is not const:
/// seeds spread evenly over a sphere without clumping.
const GOLDEN_ANGLE: f32 = 2.399_963_2;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Body {
    pub pos: Vec3,
    pub vel: Vec3,
    /// Relative weight, used for node radius (not for charge).
    pub weight: f32,
    /// Pinned bodies keep their position: the "new nodes settle in place"
    /// rule needs the existing ones to stay put.
    pub pinned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Link {
    /// Endpoint indices, `a < b` by construction (see `Scene::apply`).
    pub a: usize,
    pub b: usize,
    pub strength: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Negative = repel. World units cubed over seconds squared, roughly.
    pub charge: f32,
    /// Rest length of a link spring, in world units.
    pub link_distance: f32,
    /// Spring stiffness per unit of stretch.
    pub spring: f32,
    /// Pull towards the origin, per second.
    pub center: f32,
    /// Velocity decay per second (applied as a per-step factor).
    pub damping: f32,
    /// Speed ceiling, world units per second.
    pub max_speed: f32,
    /// Repulsion is clamped below this distance, so coincident nodes separate
    /// instead of producing infinite impulses.
    pub min_distance: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            charge: -4000.0,
            link_distance: 60.0,
            spring: 2.0,
            center: 0.05,
            damping: 1.6,
            max_speed: 400.0,
            min_distance: 2.0,
        }
    }
}

/// Seed positions for `n` nodes: a golden-angle spiral on a sphere of the
/// given radius. Deterministic, so a fresh scene is reproducible.
pub fn seed_positions(n: usize, radius: f32) -> Vec<Vec3> {
    (0..n)
        .map(|i| {
            let t = (i as f32 + 0.5) / n.max(1) as f32;
            let y = 1.0 - 2.0 * t;
            let ring = (1.0 - y * y).max(0.0).sqrt();
            let theta = i as f32 * GOLDEN_ANGLE;
            Vec3::new(ring * theta.cos(), y, ring * theta.sin()) * radius
        })
        .collect()
}

/// Cooling schedule. Forces are scaled by `alpha`, which decays towards a small
/// floor: a layout that keeps pushing at full strength forever is a layout that
/// oscillates forever. The floor is not zero — the graph must still be able to
/// nudge itself after settling — and any data change reheats it (see `reheat`).
pub const ALPHA_DECAY: f32 = 0.9778;
pub const ALPHA_FLOOR: f32 = 0.02;
pub const ALPHA_START: f32 = 1.0;
/// Reheat on new data / a dragged node: enough to re-settle, not a full bang.
pub const ALPHA_REHEAT: f32 = 0.55;

/// Advance the layout by `dt` seconds with force scale `alpha` (0..1).
pub fn step(bodies: &mut [Body], links: &[Link], params: &Params, dt: f32, alpha: f32) {
    let dt = dt.clamp(0.0, 0.05);
    let alpha = alpha.clamp(0.0, 1.0);
    if dt == 0.0 || bodies.len() < 2 || alpha < 0.001 {
        return;
    }

    // Charge: every pair pushes every other pair apart.
    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            let Some((left, right)) = pair_mut(bodies, i, j) else {
                continue;
            };
            let delta = right.pos - left.pos;
            let dist_sq = delta.length_sq().max(params.min_distance * params.min_distance);
            let Some(dir) = delta.unit() else {
                continue;
            };
            // charge < 0 → negative acceleration along `dir` → i moves away.
            let accel = params.charge * alpha / dist_sq;
            let impulse = dir * (accel * dt);
            if !left.pinned {
                left.vel += impulse;
            }
            if !right.pinned {
                right.vel -= impulse;
            }
        }
    }

    // Links: springs towards the rest length.
    for link in links {
        let Some((left, right)) = pair_mut(bodies, link.a.min(link.b), link.a.max(link.b)) else {
            continue;
        };
        let delta = right.pos - left.pos;
        let Some(dir) = delta.unit() else {
            continue;
        };
        let stretch = delta.len() - params.link_distance;
        let impulse = dir * (stretch * params.spring * link.strength * alpha * dt);
        if !left.pinned {
            left.vel += impulse;
        }
        if !right.pinned {
            right.vel -= impulse;
        }
    }

    // Centring, damping, speed ceiling, integrate.
    let decay = (1.0 - params.damping * dt).clamp(0.0, 1.0);
    let centroid = bodies
        .iter()
        .filter(|b| !b.pinned)
        .fold((Vec3::ZERO, 0_usize), |(sum, n), b| (sum + b.pos, n + 1));
    let anchor = if centroid.1 > 0 {
        centroid.0 * (1.0 / centroid.1 as f32)
    } else {
        Vec3::ZERO
    };
    for body in bodies.iter_mut() {
        if body.pinned {
            body.vel = Vec3::ZERO;
            continue;
        }
        body.vel -= (body.pos - anchor) * (params.center * alpha * dt);
        body.vel *= decay;
        let speed = body.vel.len();
        if speed > params.max_speed {
            body.vel *= params.max_speed / speed;
        }
        body.pos += body.vel * dt;
    }
}

/// Two mutable bodies by index, whatever their order.
fn pair_mut(bodies: &mut [Body], i: usize, j: usize) -> Option<(&mut Body, &mut Body)> {
    if i == j || i >= bodies.len() || j >= bodies.len() {
        return None;
    }
    let (lo, hi) = (i.min(j), i.max(j));
    let (left, right) = bodies.split_at_mut(hi);
    let a = left.get_mut(lo)?;
    let b = right.first_mut()?;
    if i < j { Some((a, b)) } else { Some((b, a)) }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn two_bodies(distance: f32) -> Vec<Body> {
        vec![
            Body {
                pos: Vec3::new(-distance * 0.5, 0.0, 0.0),
                weight: 1.0,
                ..Body::default()
            },
            Body {
                pos: Vec3::new(distance * 0.5, 0.0, 0.0),
                weight: 1.0,
                ..Body::default()
            },
        ]
    }

    #[test]
    fn charge_only_pushes_nodes_apart() {
        let mut bodies = two_bodies(50.0);
        let params = Params {
            center: 0.0,
            ..Params::default()
        };
        let before = bodies[0].pos.x - bodies[1].pos.x;
        for _ in 0..30 {
            step(&mut bodies, &[], &params, 1.0 / 60.0, 1.0);
        }
        let after = bodies[0].pos.x - bodies[1].pos.x;
        assert!(after.abs() > before.abs(), "{before} -> {after}");
    }

    #[test]
    fn springs_pull_stretched_links_to_rest_length() {
        let mut bodies = two_bodies(400.0);
        let links = [Link {
            a: 0,
            b: 1,
            strength: 1.0,
        }];
        let params = Params {
            charge: 0.0,
            center: 0.0,
            damping: 2.5,
            ..Params::default()
        };
        for _ in 0..2400 {
            step(&mut bodies, &links, &params, 1.0 / 60.0, 1.0);
        }
        let gap = (bodies[0].pos.x - bodies[1].pos.x).abs();
        assert!(
            (gap - params.link_distance).abs() < params.link_distance * 0.1,
            "gap {gap}"
        );
    }

    #[test]
    fn damping_bleeds_off_velocity() {
        let mut bodies = two_bodies(50.0);
        bodies[0].vel = Vec3::new(200.0, 0.0, 0.0);
        let params = Params {
            charge: 0.0,
            center: 0.0,
            ..Params::default()
        };
        for _ in 0..60 {
            step(&mut bodies, &[], &params, 1.0 / 60.0, 1.0);
        }
        assert!(bodies[0].vel.len() < 200.0);
    }

    #[test]
    fn pinned_bodies_stay_put() {
        let mut bodies = two_bodies(20.0);
        bodies[1].pinned = true;
        let pinned = bodies[1].pos;
        for _ in 0..120 {
            step(&mut bodies, &[], &Params::default(), 1.0 / 60.0, 1.0);
        }
        assert_eq!(bodies[1].pos, pinned);
    }

    #[test]
    fn seeds_are_deterministic_and_on_the_sphere() {
        let a = seed_positions(12, 100.0);
        let b = seed_positions(12, 100.0);
        assert_eq!(a, b);
        for p in &a {
            assert!((p.len() - 100.0).abs() < 0.01, "{p:?}");
        }
    }

    #[test]
    fn a_catch_up_frame_cannot_explode_the_layout() {
        let mut bodies = two_bodies(10.0);
        step(&mut bodies, &[], &Params::default(), 30.0, 1.0);
        assert!(bodies.iter().all(|b| b.pos.len().is_finite()));
    }
}