//! Pointer → node resolution without reading pixels back from the GPU.
//!
//! Every node is projected with the same camera the view uses (and the label
//! layer uses), so "the label that sits on this sphere" and "the node a click
//! hits" are the same answer by construction. A merged CSS-pixel radius makes
//! small nodes still clickable; ties go to the nearer node (smaller depth), so
//! a sphere in front of another wins.

use crate::camera::Camera;
use crate::force::Body;

/// Extra reach around a node's drawn sphere, in CSS pixels.
///
/// The balls are 2.5–3.7 px in radius (the DSL's default `link_width_px` ×
/// `node_size_ratio`), so a hit test against the sphere alone asks the pointer
/// to land within a 6 px dot: measured on the running panel, hovering a node's
/// centre raised its card and 4 px off it did not. That is indistinguishable
/// from "hovering does nothing", and hovering is the only way to read a node's
/// details. The slack is a *hit tolerance*, not a drawn size — the picture is
/// unchanged — and both the hover probe and the press share it, so the node a
/// card names is the node a press grabs.
pub const PICK_SLACK_PX: f32 = 12.0;

pub fn pick(
    camera: &Camera,
    viewport: (f32, f32),
    bodies: &[Body],
    radius_px: &[f32],
    point: (f32, f32),
) -> Option<usize> {
    let mut best: Option<(usize, f32, f32)> = None;
    for (index, body) in bodies.iter().enumerate() {
        let Some(screen) = camera.project(viewport, body.pos) else {
            continue;
        };
        let radius = radius_px.get(index).copied().unwrap_or(4.0).max(2.0) + PICK_SLACK_PX;
        let dx = screen.x - point.0;
        let dy = screen.y - point.1;
        let dist_sq = dx * dx + dy * dy;
        if dist_sq > radius * radius {
            continue;
        }
        let better = match best {
            None => true,
            Some((_, best_dist, best_depth)) => {
                dist_sq < best_dist || (dist_sq == best_dist && screen.depth < best_depth)
            }
        };
        if better {
            best = Some((index, dist_sq, screen.depth));
        }
    }
    best.map(|(index, _, _)| index)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::math::Vec3;

    fn bodies_at(xs: &[f32]) -> Vec<Body> {
        xs.iter()
            .map(|x| Body {
                pos: Vec3::new(*x, 0.0, 0.0),
                weight: 1.0,
                ..Body::default()
            })
            .collect()
    }

    fn flat_camera() -> Camera {
        Camera {
            distance: 100.0,
            forward: Vec3::new(0.0, 0.0, -1.0),
            up: Vec3::UP,
            ..Camera::default()
        }
    }

    #[test]
    fn clicking_a_node_returns_it() {
        let cam = flat_camera();
        let bodies = bodies_at(&[-10.0, 0.0, 10.0]);
        let target = cam.project((800.0, 600.0), bodies[2].pos).unwrap();
        let hit = pick(&cam, (800.0, 600.0), &bodies, &[6.0, 6.0, 6.0], (target.x, target.y));
        assert_eq!(hit, Some(2));
    }

    #[test]
    fn empty_space_hits_nothing() {
        let cam = flat_camera();
        let bodies = bodies_at(&[0.0]);
        let hit = pick(&cam, (800.0, 600.0), &bodies, &[4.0], (5.0, 5.0));
        assert_eq!(hit, None);
    }

    #[test]
    fn a_hit_does_not_need_pixel_precision() {
        // The drawn sphere is ~3 px; a pointer 8 px off its centre is still
        // "on" the node (the slack), and 20 px off is not.
        let cam = flat_camera();
        let bodies = bodies_at(&[0.0]);
        let target = cam.project((800.0, 600.0), bodies[0].pos).unwrap();
        assert_eq!(
            pick(&cam, (800.0, 600.0), &bodies, &[3.0], (target.x + 8.0, target.y)),
            Some(0)
        );
        assert_eq!(
            pick(&cam, (800.0, 600.0), &bodies, &[3.0], (target.x + 20.0, target.y)),
            None
        );
    }

    #[test]
    fn the_nearest_node_wins_when_two_overlap() {
        let cam = flat_camera();
        // Same screen position, one closer to the eye (larger +z).
        let bodies = vec![
            Body {
                pos: Vec3::new(0.0, 0.0, 40.0),
                weight: 1.0,
                ..Body::default()
            },
            Body {
                pos: Vec3::new(0.0, 0.0, 60.0),
                weight: 1.0,
                ..Body::default()
            },
        ];
        let target = cam.project((800.0, 600.0), bodies[0].pos).unwrap();
        let hit = pick(
            &cam,
            (800.0, 600.0),
            &bodies,
            &[40.0, 40.0],
            (target.x, target.y),
        );
        assert_eq!(hit, Some(1));
    }
}