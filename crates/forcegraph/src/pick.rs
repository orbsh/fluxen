//! Pointer → node resolution without reading pixels back from the GPU.
//!
//! Every node is projected with the same camera the view uses (and the label
//! layer uses), so "the label that sits on this sphere" and "the node a click
//! hits" are the same answer by construction. A merged CSS-pixel radius makes
//! small nodes still clickable; ties go to the nearer node (smaller depth), so
//! a sphere in front of another wins.

use crate::camera::Camera;
use crate::force::Body;

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
        let radius = radius_px.get(index).copied().unwrap_or(4.0).max(2.0);
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
            pitch: 0.0,
            yaw: 0.0,
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