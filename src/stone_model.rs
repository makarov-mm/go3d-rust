//! The stone: a lens, both faces gently domed and meeting in a rounded edge, as Japanese stones are cut.
//! Its middle is the origin; it lies with its axis along y.

use crate::math::{PI, Vec2, Vec3, vec2, vec3};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
}

#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

// The lines of the board are one unit apart.
pub const RADIUS: f32 = 0.475;
pub const HALF_HEIGHT: f32 = 0.19;

pub struct StoneShape {
    pub detailed: Mesh, // for the scene
    pub simple: Mesh,   // for the shadows and the reflections
}

pub fn build_stone() -> StoneShape {
    StoneShape { detailed: build(14, 48), simple: build(5, 20) }
}

/// The height of the face at a distance from the axis: flat in the middle, falling steeply at the edge.
fn face_height(r: f32) -> f32 {
    let t = (r / RADIUS).min(1.0);
    HALF_HEIGHT * (1.0 - t * t).powf(0.75)
}

struct ProfilePoint {
    r: f32,
    y: f32,
    normal: Vec2,
}

/// A point of a face, sign 1 for the upper and -1 for the lower, with its outward normal from the slope.
fn face(r: f32, sign: f32) -> ProfilePoint {
    let d = 0.0005;
    let (a, b) = ((r - d).max(0.0), (r + d).min(RADIUS));
    let slope = (face_height(b) - face_height(a)) / (b - a);
    let length = (1.0 + slope * slope).sqrt();
    ProfilePoint { r, y: sign * face_height(r), normal: vec2(-slope / length, sign / length) }
}

/// The outline from the middle of the lower face round the edge to the middle of the upper face, its points
/// crowded at the edge where it turns fastest, turned round the axis.
fn build(rings: usize, segments: usize) -> Mesh {
    let ring_radius = |ring: usize| RADIUS * (PI / 2.0 * ring as f32 / rings as f32).sin();
    let mut profile: Vec<ProfilePoint> = (0..rings).map(|ring| face(ring_radius(ring), -1.0)).collect();
    profile.push(ProfilePoint { r: RADIUS, y: 0.0, normal: vec2(1.0, 0.0) });
    profile.extend((0..rings).rev().map(|ring| face(ring_radius(ring), 1.0)));

    let mut mesh = Mesh::default();
    for point in &profile {
        for segment in 0..=segments {
            let (s, c) = (2.0 * PI * segment as f32 / segments as f32).sin_cos();
            let normal = vec3(point.normal.x * c, point.normal.y, point.normal.x * s).normalized();
            mesh.vertices.push(Vertex { position: vec3(point.r * c, point.y, point.r * s), normal });
        }
    }
    let index = |ring: usize, segment: usize| (ring * (segments + 1) + segment) as u32;
    for ring in 0..profile.len() - 1 {
        for segment in 0..segments {
            // Next to the axis a quad narrows to a triangle.
            if profile[ring].r > 0.0 {
                mesh.indices.extend([index(ring, segment), index(ring + 1, segment + 1), index(ring, segment + 1)]);
            }
            if profile[ring + 1].r > 0.0 {
                mesh.indices.extend([index(ring, segment), index(ring + 1, segment), index(ring + 1, segment + 1)]);
            }
        }
    }
    mesh
}
