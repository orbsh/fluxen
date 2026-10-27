//! Orbit camera and the world→pixel projection the view is built on.
//!
//! The camera orbits a target at a distance, with `yaw` around world up and
//! `pitch` as elevation. Everything the graph needs to size things on screen
//! comes from one number: `px_per_unit` at the point's depth. Node radii, link
//! widths and label offsets are all expressed in CSS pixels and divided by
//! that scale, so the promise "this node is 6 px" holds at any zoom level.

use std::f32::consts::PI;

use crate::math::Vec3;

/// Vertical field of view used when none is configured. 50° is the value the
/// previous module-based graph was measured against (PLAN §26: at camera
/// distance 1000 and a 460 px tall canvas the scale came out at 0.4932
/// px/unit), so existing frames keep their look.
pub const DEFAULT_FOV_Y: f32 = 50.0 * PI / 180.0;

/// Near and far planes. Shared with the renderer, which needs the same values
/// the projection matrix was built with.
pub const Z_NEAR: f32 = 0.1;
pub const Z_FAR: f32 = 100_000.0;

/// Orientation is stored as a frame, not as yaw/pitch angles.
///
/// Two angles describe every orientation except the poles, and a pole is exactly
/// where a vertical drag ends up: that is why the old camera had to clamp its
/// pitch at ±83° and why, near the limit, a horizontal drag stopped matching the
/// pointer. A frame has no poles: turning about the screen's own axes works at
/// any orientation, and it can keep going past vertical (the view simply ends up
/// upside down, which is what unbounded rotation means).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub target: Vec3,
    /// Unit vector from the eye towards the target.
    pub forward: Vec3,
    /// Screen up, unit and perpendicular to `forward`.
    pub up: Vec3,
    /// Eye distance from `target`, in world units.
    pub distance: f32,
    /// Vertical field of view, in radians.
    pub fov_y: f32,
}

impl Default for Camera {
    fn default() -> Self {
        let pitch = 0.35_f32;
        let (sin, cos) = pitch.sin_cos();
        Self {
            target: Vec3::ZERO,
            forward: Vec3::new(0.0, -sin, -cos),
            up: Vec3::new(0.0, cos, -sin),
            distance: 1000.0,
            fov_y: DEFAULT_FOV_Y,
        }
    }
}

/// Orthonormal camera frame, right-handed, `forward` pointing at the target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Basis {
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
}

/// A projected world point: viewport pixels (origin top-left), depth along the
/// view axis, and the local world→pixel scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
    pub px_per_unit: f32,
}

impl Camera {
    pub fn eye(&self) -> Vec3 {
        self.target - self.forward * self.distance
    }

    pub fn basis(&self) -> Basis {
        Basis {
            forward: self.forward,
            right: self.forward.cross(self.up),
            up: self.up,
        }
    }

    /// Turn the camera by the drag's angles: `yaw` about the screen's vertical
    /// axis, `pitch` about its horizontal one.
    ///
    /// Both are unbounded and both rotate the frame the drag is measured in, so
    /// content follows the pointer horizontally no matter how far the view has
    /// been tipped, and a vertical drag never jams. Re-orthonormalised each time
    /// so a long drag cannot drift the frame.
    pub fn orbit(&mut self, yaw: f32, pitch: f32) {
        if yaw != 0.0 {
            self.forward = self.forward.rotate(self.up, yaw);
        }
        if pitch != 0.0 {
            let right = self.forward.cross(self.up);
            self.forward = self.forward.rotate(right, pitch).unit().unwrap_or(self.forward);
            self.up = self.up.rotate(right, pitch).unit().unwrap_or(self.up);
        }
        // Re-derive up from the new forward and the old right, which is cheaper
        // and steadier than trusting two rotated vectors to stay perpendicular.
        let right = self.forward.cross(self.up).unit().unwrap_or(Vec3::new(1.0, 0.0, 0.0));
        self.up = right.cross(self.forward).unit().unwrap_or(Vec3::UP);
    }

    /// Project a world point. `None` means behind the near plane — callers
    /// skip such points instead of clamping them onto the screen edge.
    pub fn project(&self, viewport: (f32, f32), point: Vec3) -> Option<Screen> {
        let basis = self.basis();
        let to_point = point - self.eye();
        let depth = to_point.dot(basis.forward);
        if depth <= f32::EPSILON {
            return None;
        }
        let focal = viewport.1 / (2.0 * (self.fov_y * 0.5).tan());
        let px_per_unit = focal / depth;
        Some(Screen {
            x: viewport.0 * 0.5 + to_point.dot(basis.right) * px_per_unit,
            y: viewport.1 * 0.5 - to_point.dot(basis.up) * px_per_unit,
            depth,
            px_per_unit,
        })
    }

    /// Frame a point cloud: centre on the bounding box and pull back far
    /// enough that the farthest point clears `padding_px`.
    pub fn fit(&mut self, points: &[Vec3], padding_px: f32, viewport: (f32, f32)) {
        let Some(first) = points.first().copied() else {
            return;
        };
        let mut min = first;
        let mut max = first;
        for p in points.iter().skip(1) {
            min = Vec3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
            max = Vec3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
        }
        let center = (min + max) * 0.5;
        let radius = points
            .iter()
            .map(|p| (*p - center).len())
            .fold(0.0_f32, f32::max);

        let half_h = viewport.1 * 0.5;
        let usable = (half_h - padding_px).max(1.0);
        let tan = (self.fov_y * 0.5).tan();
        self.target = center;
        self.distance = (radius * half_h / (tan * usable)).max(1.0);
    }

    /// Column-major view matrix: world → eye space. The renderer's viewer wants
    /// view and projection separately, while picking wants their product, so the
    /// matrix is built in two pieces and composed once here rather than written
    /// out twice.
    pub fn view_matrix(&self) -> [f32; 16] {
        let eye = self.eye();
        let basis = self.basis();
        let rows = [
            [
                basis.right.x,
                basis.right.y,
                basis.right.z,
                -basis.right.dot(eye),
            ],
            [basis.up.x, basis.up.y, basis.up.z, -basis.up.dot(eye)],
            [
                -basis.forward.x,
                -basis.forward.y,
                -basis.forward.z,
                basis.forward.dot(eye),
            ],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mut out = [0.0_f32; 16];
        for (row, cells) in rows.iter().enumerate() {
            for (col, cell) in cells.iter().enumerate() {
                out[col * 4 + row] = *cell;
            }
        }
        out
    }

    /// Column-major perspective matrix: eye space → clip space.
    pub fn projection_matrix(&self, viewport: (f32, f32)) -> [f32; 16] {
        let aspect = (viewport.0 / viewport.1.max(1.0)).max(f32::EPSILON);
        let focal = 1.0 / (self.fov_y * 0.5).tan();
        let (near, far) = (Z_NEAR, Z_FAR);
        let nf = 1.0 / (near - far);
        [
            focal / aspect,
            0.0,
            0.0,
            0.0,
            0.0,
            focal,
            0.0,
            0.0,
            0.0,
            0.0,
            (far + near) * nf,
            -1.0,
            0.0,
            0.0,
            2.0 * far * near * nf,
            0.0,
        ]
    }

    /// Column-major view-projection matrix. Composed from the two matrices
    /// above, so the GPU, the CPU projection and the labels cannot disagree
    /// about where a node is.
    pub fn view_proj(&self, viewport: (f32, f32)) -> [f32; 16] {
        let view = self.view_matrix();
        let projection = self.projection_matrix(viewport);
        let mut out = [0.0_f32; 16];
        for col in 0..4 {
            for row in 0..4 {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += projection[k * 4 + row] * view[col * 4 + k];
                }
                out[col * 4 + row] = sum;
            }
        }
        out
    }

    /// Screen radius in pixels for a world-space radius at `depth`.
    pub fn px_for(&self, viewport: (f32, f32), world_radius: f32, depth: f32) -> f32 {
        let focal = viewport.1 / (2.0 * (self.fov_y * 0.5).tan());
        world_radius * focal / depth.max(f32::EPSILON)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// Anchored to PLAN §26's measurement: camera distance 1000, 460 px tall
    /// canvas, 50° fov → 0.4932 px per world unit at the target.
    #[test]
    fn px_per_unit_matches_the_module_era_reference() {
        let cam = Camera {
            distance: 1000.0,
            ..Camera::default()
        };
        let screen = cam
            .project((1384.0, 460.0), cam.target)
            .expect("target is in front of the camera");
        assert!((screen.px_per_unit - 0.4932).abs() < 0.001, "{screen:?}");
        assert!((screen.x - 692.0).abs() < 1e-3);
        assert!((screen.y - 230.0).abs() < 1e-3);
    }

    #[test]
    fn points_behind_the_camera_are_not_projected() {
        let mut cam = Camera::default();
        cam.forward = Vec3::new(0.0, 0.0, -1.0);
        cam.up = Vec3::UP;
        let behind = cam.eye() + Vec3::new(0.0, 0.0, 10.0);
        assert!(cam.project((800.0, 600.0), behind).is_none());
    }

    #[test]
    fn right_of_target_lands_right_of_centre() {
        let mut cam = Camera::default();
        cam.forward = Vec3::new(0.0, 0.0, -1.0);
        cam.up = Vec3::UP;
        cam.distance = 100.0;
        let right = cam.target + Vec3::new(10.0, 0.0, 0.0);
        let screen = cam.project((800.0, 600.0), right).unwrap();
        assert!(screen.x > 400.0, "{screen:?}");
    }

    #[test]
    fn fit_keeps_every_point_inside_the_viewport() {
        let points: Vec<Vec3> = (0..24)
            .map(|i| {
                let a = i as f32 * 0.6;
                Vec3::new(a.cos() * 100.0, (i as f32 * 0.3).sin() * 60.0, a.sin() * 100.0)
            })
            .collect();
        let mut cam = Camera::default();
        cam.fit(&points, 24.0, (900.0, 500.0));
        for p in &points {
            let s = cam.project((900.0, 500.0), *p).unwrap();
            assert!((0.0..=900.0).contains(&s.x), "{s:?}");
            assert!((0.0..=500.0).contains(&s.y), "{s:?}");
        }
    }

    /// A horizontal drag must move content horizontally — that was the whole
    /// point of turning about the screen's own axis instead of world up, which
    /// tilted everything whenever the view was pitched.
    #[test]
    fn a_horizontal_drag_moves_content_horizontally() {
        let mut cam = Camera::default();
        let viewport = (800.0, 600.0);
        let basis = cam.basis();
        // A point on the pivot plane cannot show direction — turning either way
        // only foreshortens it towards the centre — so probe the near side,
        // which is the surface a viewer is actually grabbing.
        let probe = cam.target + basis.right * 60.0 - basis.forward * 80.0;
        let before = cam.project(viewport, probe).expect("in front");
        // What a rightward drag does (see the orbit call in the view).
        cam.orbit(-0.25, 0.0);
        let after = cam.project(viewport, probe).expect("in front");
        assert!(
            (after.y - before.y).abs() < 0.5,
            "horizontal drag tilted content: {before:?} -> {after:?}"
        );
        assert!(after.x > before.x, "content should follow the drag right");
    }

    /// No pole, no clamp: two full turns of vertical drag leave a usable frame,
    /// and a quarter turn past vertical is just an upside-down view.
    #[test]
    fn vertical_rotation_is_unbounded_and_keeps_the_frame_orthonormal() {
        let mut cam = Camera::default();
        for _ in 0..8 {
            cam.orbit(0.0, std::f32::consts::FRAC_PI_4);
            let b = cam.basis();
            for v in [b.forward, b.up, b.right] {
                assert!((v.len() - 1.0).abs() < 1e-3, "not unit: {v:?}");
                assert!(v.x.is_finite() && v.y.is_finite() && v.z.is_finite());
            }
            assert!(b.forward.dot(b.up).abs() < 1e-3, "not perpendicular");
            assert!(cam.project((800.0, 600.0), cam.target).is_some());
        }
    }

    /// The GPU path uses `view_proj`; labels, picking and labels sizing use
    /// `project`. If they disagree, spheres and labels drift apart.
    #[test]
    fn view_proj_agrees_with_project() {
        let mut camera = Camera {
            distance: 300.0,
            ..Camera::default()
        };
        camera.orbit(0.4, 0.3);
        let viewport = (800.0, 600.0);
        let m = camera.view_proj(viewport);
        let points = [
            Vec3::new(10.0, 20.0, -30.0),
            Vec3::new(-100.0, 0.0, 50.0),
            camera.target,
        ];
        for point in points {
            let screen = camera.project(viewport, point).expect("in front");
            let clip = [
                m[0] * point.x + m[4] * point.y + m[8] * point.z + m[12],
                m[1] * point.x + m[5] * point.y + m[9] * point.z + m[13],
                m[2] * point.x + m[6] * point.y + m[10] * point.z + m[14],
                m[3] * point.x + m[7] * point.y + m[11] * point.z + m[15],
            ];
            let x = (clip[0] / clip[3] * 0.5 + 0.5) * viewport.0;
            let y = (0.5 - clip[1] / clip[3] * 0.5) * viewport.1;
            assert!((x - screen.x).abs() < 0.5, "{point:?}: gpu {x} vs cpu {}", screen.x);
            assert!((y - screen.y).abs() < 0.5, "{point:?}: gpu {y} vs cpu {}", screen.y);
        }
    }

    #[test]
    fn fit_on_empty_cloud_keeps_the_camera_alone() {
        let mut cam = Camera::default();
        let before = cam;
        cam.fit(&[], 20.0, (800.0, 600.0));
        assert_eq!(cam, before);
    }
}