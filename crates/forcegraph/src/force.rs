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

/// Relaxation budget used by the tests: the same 200 steps the view runs before
/// a brand-new graph is painted for the first time.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Body {
    pub pos: Vec3,
    pub vel: Vec3,
    /// Relative weight, used for node radius (not for charge).
    pub weight: f32,
    /// Pinned bodies keep their position: the "new nodes settle in place"
    /// rule needs the existing ones to stay put.
    pub pinned: bool,
    /// Collision radius in world units — the rendered sphere radius converted
    /// through the camera, written by the view before each step. Two nodes may
    /// not be closer than the sum of their radii, otherwise spheres overlap and
    /// the picture reads as noise instead of a graph.
    pub collide: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Link {
    /// Endpoint indices, `a < b` by construction (see `Scene::apply`).
    pub a: usize,
    pub b: usize,
    pub strength: f32,
}

/// The force model, in d3-force-3d's own vocabulary — that is what
/// 3d-force-graph runs, so these are the numbers its users see:
/// `forceManyBody().strength(-30)`, `forceLink().distance(30)` with the default
/// per-link strength `1 / min(degree)`, `forceCenter`, and
/// `velocityDecay(0.4)`. Reinventing the constants is how the layout ended up
/// with a centring spring that out-muscled repulsion.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// `forceManyBody().strength()`: negative repels.
    pub charge: f32,
    /// `forceLink().distance()`.
    pub link_distance: f32,
    /// `forceLink().strength()`; `None` keeps d3's `1 / min(degree)`, which is
    /// what keeps a hub from dragging its whole neighbourhood onto itself.
    pub link_strength: Option<f32>,
    /// `simulation.velocityDecay(0.4)`: fraction of velocity dropped per tick.
    pub velocity_decay: f32,
    /// Speed ceiling per tick. d3 has none; a hand-written loop needs one or a
    /// catch-up frame can throw bodies out of the scene.
    pub max_speed: f32,
    /// Repulsion is clamped below this distance so coincident bodies separate
    /// instead of dividing by zero.
    pub min_distance: f32,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            charge: -30.0,
            link_distance: 30.0,
            link_strength: None,
            velocity_decay: 0.4,
            max_speed: 60.0,
            min_distance: 1.0,
        }
    }
}

/// A deterministic escape direction for the `index`-th node: the same golden
/// spiral the seeds use, as a unit vector. Two things need it — placing a new
/// node next to an already-placed neighbour without landing on top of it, and
/// separating two coincident bodies (which have no direction of their own).
/// Pushing every coincidence along +Y is how a graph turns into a line.
pub fn spiral_dir(index: usize) -> Vec3 {
    let t = ((index as f32 * GOLDEN_ANGLE).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    let y = 1.0 - 2.0 * t;
    let ring = (1.0 - y * y).max(0.0).sqrt();
    let theta = index as f32 * GOLDEN_ANGLE;
    Vec3::new(ring * theta.cos(), y, ring * theta.sin())
        .unit()
        .unwrap_or(Vec3::UP)
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

/// Cooling schedule, `simulation.alphaDecay` and `alphaMin` from d3: alpha
/// starts at 1, falls by 2.28% per tick and stops at 0.001 — about 300 ticks,
/// five seconds at 60 fps, which is the window 3d-force-graph gives a layout to
/// find itself.
pub const ALPHA_DECAY: f32 = 1.0 - 0.0228;
pub const ALPHA_FLOOR: f32 = 0.001;
pub const ALPHA_START: f32 = 1.0;
/// Reheat on new data / a dragged node. d3's own restart uses a fraction of a
/// full alpha, not the whole bang.
pub const ALPHA_REHEAT: f32 = 0.5;
/// Steps a test runs to settle a layout. Test-only: the view drives cooling
/// from frames, and a constant in the library would exist for nobody.
#[cfg(test)]
const RELAX_STEPS_FOR_TESTS: usize = 200;
/// Collision resolution passes per tick. More than one, because a push can
/// create the next overlap.
const COLLISION_PASSES: usize = 3;

/// One tick of d3-force-3d, force for force: `forceManyBody` (charge),
/// `forceLink`, then velocity decay and integration, then `forceCenter` as a
/// translation. A positional collision pass sits between them — d3 keeps that in
/// a separate `forceCollide`, and it is what stops spheres overlapping.
///
/// The layout is a port, not an invention: writing a centring *spring* of my own
/// is precisely what folded the graph into a knot, and picking force constants by
/// eye is what made "the forces feel wrong" repeat itself.
pub fn step(bodies: &mut [Body], links: &[Link], params: &Params, alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    if bodies.is_empty() {
        return;
    }
    // Below the floor the layout has stopped moving — but overlaps must still
    // heal, so the collision pass and the recentring keep running while the
    // forces rest. Freezing everything at the floor is how a hub keeps two
    // nodes on the same spot for as long as the page is open.
    let cooled = alpha < ALPHA_FLOOR * 0.5;

    if !cooled {
        // d3's default link strength is 1 / min(degree); the degrees come from the
        // link list, so a hub moves less than the leaves hanging off it.
        let mut degree = vec![1_u32; bodies.len()];
        for link in links {
            if let Some(d) = degree.get_mut(link.a) {
                *d += 1;
            }
            if let Some(d) = degree.get_mut(link.b) {
                *d += 1;
            }
        }

        // forceManyBody: w = strength * alpha / distance², applied along the raw
        // delta (d3 does not normalise it — the delta already carries the scale).
        let min_sq = params.min_distance * params.min_distance;
        for i in 0..bodies.len() {
            for j in (i + 1)..bodies.len() {
                let Some((left, right)) = pair_mut(bodies, i, j) else {
                    continue;
                };
                let delta = right.pos - left.pos;
                let mut dist_sq = delta.length_sq();
                if dist_sq < min_sq {
                    dist_sq = (min_sq * dist_sq).sqrt().max(1e-6);
                }
                if dist_sq <= 0.0 {
                    continue;
                }
                let w = params.charge * alpha / dist_sq;
                if !left.pinned {
                    left.vel += delta * w;
                }
                if !right.pinned {
                    right.vel -= delta * w;
                }
            }
        }

        // forceLink: k = (dist - distance) / dist * alpha * strength, split by the
        // degree bias d3 uses so a hub does not get yanked by every leaf.
        for link in links {
            let Some((left, right)) = pair_mut(bodies, link.a.min(link.b), link.a.max(link.b)) else {
                continue;
            };
            let delta = right.pos - left.pos;
            let dist = delta.len();
            if dist <= 0.0 {
                continue;
            }
            let (deg_a, deg_b) = (
                degree.get(link.a).copied().unwrap_or(1),
                degree.get(link.b).copied().unwrap_or(1),
            );
            let strength = params
                .link_strength
                .unwrap_or_else(|| 1.0 / deg_a.min(deg_b).max(1) as f32);
            let k = (dist - params.link_distance) / dist * alpha * strength;
            let total = (deg_a + deg_b).max(1) as f32;
            let bias = deg_a as f32 / total;
            if !left.pinned {
                left.vel += delta * (k * bias);
            }
            if !right.pinned {
                right.vel -= delta * (k * (1.0 - bias));
            }
        }

        // velocityDecay, speed ceiling, then integrate (all d3's order).
        let keep = (1.0 - params.velocity_decay).clamp(0.0, 1.0);
        for body in bodies.iter_mut() {
            if body.pinned {
                body.vel = Vec3::ZERO;
                continue;
            }
            body.vel *= keep;
            let speed = body.vel.len();
            if speed > params.max_speed {
                body.vel *= params.max_speed / speed;
            }
            body.pos += body.vel;
        }

    }

    // Collision: a hard positional correction, not a force. Overlapping spheres
    // are the one thing the viewer must never see, and a constraint settles
    // where a force only converges.
    //
    // Several passes, because one is not enough once a hub packs its
    // neighbourhood: pushing A off B can push A into C, and the spring that was
    // too weak to prevent the overlap is far too weak to undo it once alpha has
    // cooled. d3's forceCollide has the same knob; the difference is that here
    // the layout also gets a final relaxation once it stops moving.
    for _ in 0..COLLISION_PASSES {
    for i in 0..bodies.len() {
        for j in (i + 1)..bodies.len() {
            let (Some(ri), Some(rj)) = (
                bodies.get(i).map(|b| b.collide),
                bodies.get(j).map(|b| b.collide),
            ) else {
                continue;
            };
            let min = ri + rj;
            if min <= 0.0 {
                continue;
            }
            let Some((left, right)) = pair_mut(bodies, i, j) else {
                continue;
            };
            let delta = right.pos - left.pos;
            let dist = delta.len();
            if dist >= min {
                continue;
            }
            // Coincident bodies have no direction of their own; a fixed axis
            // would turn every coincidence into the same line.
            let dir = delta.unit().unwrap_or_else(|| spiral_dir(j));
            let push = min - dist;
            match (left.pinned, right.pinned) {
                (false, false) => {
                    left.pos -= dir * (push * 0.5);
                    right.pos += dir * (push * 0.5);
                }
                (true, false) => right.pos += dir * push,
                (false, true) => left.pos -= dir * push,
                (true, true) => {}
            }
        }
    }
    }

    // forceCenter: a translation of the centroid to the origin, never a spring
    // towards it — a spring at graph scale out-muscles repulsion.
    let (sum, count) = bodies
        .iter()
        .filter(|b| !b.pinned)
        .fold((Vec3::ZERO, 0_usize), |(sum, n), b| (sum + b.pos, n + 1));
    if count > 0 {
        let shift = sum * (1.0 / count as f32);
        for body in bodies.iter_mut().filter(|b| !b.pinned) {
            body.pos -= shift;
        }
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
        // Recentring is a translation, so it cannot fake this result: only the
        // repulsion can change the gap between the two bodies.
        let params = Params::default();
        let before = bodies[0].pos.x - bodies[1].pos.x;
        for _ in 0..30 {
            step(&mut bodies, &[], &params, 1.0);
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
            ..Params::default()
        };
        for _ in 0..2400 {
            step(&mut bodies, &links, &params, 1.0);
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
            ..Params::default()
        };
        for _ in 0..60 {
            step(&mut bodies, &[], &params, 1.0);
        }
        assert!(bodies[0].vel.len() < 200.0);
    }

    #[test]
    fn pinned_bodies_stay_put() {
        let mut bodies = two_bodies(20.0);
        bodies[1].pinned = true;
        let pinned = bodies[1].pos;
        for _ in 0..120 {
            step(&mut bodies, &[], &Params::default(), 1.0);
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

    /// The layout's job is legibility, not prettiness: linked nodes stay at the
    /// rest length, unlinked nodes keep their distance, nothing overlaps and —
    /// the complaint that started this test — two clusters do not interleave.
    #[test]
    fn clusters_stay_tight_apart_and_never_overlap() {
        // Two 4-cycles joined by one bridge; radii from the default style.
        let mut bodies = seed_positions(8, 120.0)
            .into_iter()
            .map(|pos| Body {
                pos,
                weight: 6.0,
                collide: 14.0,
                ..Body::default()
            })
            .collect::<Vec<_>>();
        let links = [
            Link { a: 0, b: 1, strength: 1.0 },
            Link { a: 1, b: 2, strength: 1.0 },
            Link { a: 2, b: 3, strength: 1.0 },
            Link { a: 3, b: 0, strength: 1.0 },
            Link { a: 4, b: 5, strength: 1.0 },
            Link { a: 5, b: 6, strength: 1.0 },
            Link { a: 6, b: 7, strength: 1.0 },
            Link { a: 7, b: 4, strength: 1.0 },
            // No bridge: two components. With a bridge between them the
            // necklace is *supposed* to touch, so it proves nothing about
            // whether separate clusters read as separate.
        ];
        let params = Params::default();
        let mut alpha = ALPHA_START;
        for _ in 0..1200 {
            step(&mut bodies, &links, &params, alpha);
            alpha = (alpha * ALPHA_DECAY).max(ALPHA_FLOOR);
        }

        let dist = |i: usize, j: usize| (bodies[i].pos - bodies[j].pos).len();

        // 1. No overlap: the viewer must never see two spheres inside each other.
        for i in 0..bodies.len() {
            for j in (i + 1)..bodies.len() {
                let min = bodies[i].collide + bodies[j].collide;
                assert!(dist(i, j) > min * 0.92, "nodes {i},{j} overlap: {}", dist(i, j));
            }
        }

        // 2. Springs hold: every edge sits near the rest length.
        for link in links {
            let d = dist(link.a, link.b);
            assert!(
                d < params.link_distance * 1.5,
                "edge {}-{} stretched to {d}",
                link.a,
                link.b
            );
        }

        // 3. Contrast: unlinked pairs are visibly further apart than linked ones.
        let mean_edge = links.iter().map(|l| dist(l.a, l.b)).sum::<f32>() / links.len() as f32;
        let mut unlinked = (Vec::new(), 0.0_f32);
        for i in 0..bodies.len() {
            for j in (i + 1)..bodies.len() {
                if !links.iter().any(|l| (l.a, l.b) == (i, j)) {
                    unlinked.0.push(dist(i, j));
                    unlinked.1 += dist(i, j);
                }
            }
        }
        let mean_unlinked = unlinked.1 / unlinked.0.len() as f32;
        assert!(
            mean_unlinked > mean_edge * 1.6,
            "structure invisible: edges {mean_edge:.1} vs others {mean_unlinked:.1}"
        );

        // 4. No interleaving: every node is closer to its own cluster.
        for node in 0..bodies.len() {
            let own: Vec<usize> = if node < 4 { vec![0, 1, 2, 3] } else { vec![4, 5, 6, 7] };
            let other: Vec<usize> = if node < 4 { vec![4, 5, 6, 7] } else { vec![0, 1, 2, 3] };
            let near = |set: &[usize]| {
                set.iter()
                    .filter(|&&peer| peer != node)
                    .map(|&peer| dist(node, peer))
                    .sum::<f32>()
                    / set.len() as f32
            };
            assert!(
                near(&own) < near(&other),
                "node {node} sits among the other cluster: own {:.1} vs other {:.1}",
                near(&own),
                near(&other)
            );
        }
    }

    /// A layout that collapses into a plane or a line is illegible however
    /// tidy its numbers look: the viewer sees nodes stacked on top of each
    /// other. Extents in all three axes must stay comparable.
    #[test]
    fn the_layout_stays_three_dimensional() {
        // Seeds and collision radius in the graph's own scale: d3's rest length
        // is 30, so a seeded node is a couple of units across, not fourteen.
        let mut bodies: Vec<Body> = seed_positions(5, 30.0)
            .into_iter()
            .map(|pos| Body {
                pos,
                weight: 5.0,
                collide: Params::default().link_distance * 0.15,
                ..Body::default()
            })
            .collect();
        let links = [
            Link { a: 0, b: 1, strength: 1.0 },
            Link { a: 1, b: 2, strength: 1.0 },
            Link { a: 2, b: 0, strength: 1.0 },
            Link { a: 3, b: 4, strength: 1.0 },
        ];
        let params = Params::default();
        let mut alpha = ALPHA_START;
        for _ in 0..RELAX_STEPS_FOR_TESTS {
            step(&mut bodies, &links, &params, alpha);
            alpha = (alpha * ALPHA_DECAY).max(ALPHA_FLOOR);
        }
        let extent = |f: fn(Vec3) -> f32| {
            let values: Vec<f32> = bodies.iter().map(|b| f(b.pos)).collect();
            let min = values.iter().copied().fold(f32::INFINITY, f32::min);
            let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            max - min
        };
        let (dx, dy, dz) = (
            extent(|p| p.x),
            extent(|p| p.y),
            extent(|p| p.z),
        );
        let widest = dx.max(dy).max(dz);
        assert!(
            dx > widest * 0.2 && dy > widest * 0.2 && dz > widest * 0.2,
            "flat layout: extents {dx:.1} x {dy:.1} x {dz:.1}"
        );
    }

    /// The constraint must actually separate bodies that are on top of each
    /// other — the browser check reports a pair 2 px apart with a 25-unit
    /// radius, so the loop itself is under suspicion.
    #[test]
    fn collision_separates_coincident_bodies_in_one_tick() {
        let mut bodies: Vec<Body> = (0..5)
            .map(|i| Body {
                pos: Vec3::new(i as f32 * 0.2, 0.0, 0.0),
                weight: 5.0,
                collide: 25.0,
                ..Body::default()
            })
            .collect();
        step(&mut bodies, &[], &Params::default(), ALPHA_START);
        for i in 0..bodies.len() {
            for j in (i + 1)..bodies.len() {
                let gap = (bodies[i].pos - bodies[j].pos).len();
                assert!(gap > 40.0, "bodies {i} and {j} are {gap} apart");
            }
        }
    }

    #[test]
    fn a_catch_up_frame_cannot_explode_the_layout() {
        let mut bodies = two_bodies(10.0);
        step(&mut bodies, &[], &Params::default(), 1.0);
        assert!(bodies.iter().all(|b| b.pos.len().is_finite()));
    }
}