//! The board, the table, and the bowls with their lids. The lines of the board are one unit apart and its
//! surface is the plane y = 0; Black looks from +z, where row 1 is.

use crate::go::{self, Point};
use crate::math::{PI, Vec2, Vec3, vec2, vec3};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: Vec2,
}

#[derive(Default)]
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Where everything stands for a size of board.
#[derive(Clone)]
pub struct Layout {
    pub size: usize,
    pub half: f32,       // from the middle to the outer lines
    pub board_half: f32, // to the edge of the board
    pub thickness: f32,
    pub bowl_radius: f32,
    pub bowl_height: f32,
    pub bowls: [Vec3; 2], // Black's and White's, on the table
    pub lids: [Vec3; 2],  // turned over beside them, for the stones taken
}

impl Layout {
    pub fn new(size: usize) -> Layout {
        let half = (size - 1) as f32 / 2.0;
        let board_half = half + 0.95; // room for the coordinates round the lines
        let thickness = 0.9;
        let bowl_radius = (0.28 * board_half).max(2.2);
        // Each player's bowl on the right, toward the player; the lid beside it.
        let x = board_half + 0.8 + bowl_radius;
        let z = (board_half * 0.45).min(board_half - bowl_radius);
        let lid_z = z - 2.0 * bowl_radius - 0.4;
        let y = -thickness;
        Layout {
            size,
            half,
            board_half,
            thickness,
            bowl_radius,
            bowl_height: bowl_radius * 0.78,
            bowls: [vec3(x, y, z), vec3(-x, y, -z)],
            lids: [vec3(x, y, lid_z), vec3(-x, y, -lid_z)],
        }
    }

    pub fn table_height(&self) -> f32 {
        -self.thickness
    }

    pub fn point_center(&self, point: Point) -> Vec3 {
        vec3(go::x_of(point) as f32 - self.half, 0.0, go::y_of(point) as f32 - self.half)
    }

    /// The point nearest a point of the surface, or nothing off the board.
    pub fn point_near(&self, position: Vec3) -> Option<Point> {
        let (x, y) = ((position.x + self.half).round(), (position.z + self.half).round());
        let on_board = |coordinate: f32| coordinate >= 0.0 && coordinate < self.size as f32;
        (on_board(x) && on_board(y)).then(|| go::point_at(x as usize, y as usize))
    }

    /// How far the scene reaches from the middle, for the camera.
    pub fn extent(&self) -> f32 {
        self.board_half + self.bowl_radius * 1.2
    }
}

/// The height of the floor of a lid of this radius, where the stones taken lie.
pub const fn lid_floor(radius: f32) -> f32 {
    radius * 0.07
}

/// The height of the top of the heap of stones in a bowl.
pub const fn heap_height(height: f32) -> f32 {
    height * 0.78
}

const BEVEL: f32 = 0.06; // the rounded upper edge of the board

/// Adds a quad, wound counterclockwise as seen from the side its normal faces.
fn add_quad(mesh: &mut Mesh, corners: [Vertex; 4]) {
    let base = mesh.vertices.len() as u32;
    let [a, b, c, d] = corners;
    let facing = (b.position - a.position).cross(c.position - a.position);
    let normal = a.normal + b.normal + c.normal + d.normal;
    mesh.vertices.extend(corners);
    if facing.dot(normal) >= 0.0 {
        mesh.indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    } else {
        mesh.indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
    }
}

/// An outline turned round the y axis, given as radius and height. Normals come from the neighbouring points,
/// the outline running with the outside on its right; uv is the distance along the outline and the turn.
fn lathe(profile: &[(f32, f32)], segments: usize) -> Mesh {
    let mut mesh = Mesh::default();
    let mut along = 0.0;
    for (i, &(r, y)) in profile.iter().enumerate() {
        let (before, after) = (profile[i.saturating_sub(1)], profile[(i + 1).min(profile.len() - 1)]);
        along += (r - before.0).hypot(y - before.1);
        let (tangent_r, tangent_y) = (after.0 - before.0, after.1 - before.1);
        let length = tangent_r.hypot(tangent_y).max(1e-6);
        let (normal_r, normal_y) = (tangent_y / length, -tangent_r / length);
        for segment in 0..=segments {
            let turn = segment as f32 / segments as f32;
            let (s, c) = (2.0 * PI * turn).sin_cos();
            mesh.vertices.push(Vertex {
                position: vec3(r * c, y, r * s),
                normal: vec3(normal_r * c, normal_y, normal_r * s).normalized(),
                uv: vec2(along, turn),
            });
        }
    }
    let index = |ring: usize, segment: usize| (ring * (segments + 1) + segment) as u32;
    for ring in 0..profile.len() - 1 {
        for segment in 0..segments {
            // Next to the axis a quad narrows to a triangle.
            if profile[ring].0 > 0.0 {
                mesh.indices.extend([index(ring, segment), index(ring + 1, segment + 1), index(ring, segment + 1)]);
            }
            if profile[ring + 1].0 > 0.0 {
                mesh.indices.extend([index(ring, segment), index(ring + 1, segment), index(ring + 1, segment + 1)]);
            }
        }
    }
    mesh
}

pub fn build_board_top(layout: &Layout) -> Mesh {
    let mut mesh = Mesh::default();
    let e = layout.board_half - BEVEL;
    let vertex = |x: f32, z: f32| Vertex { position: vec3(x, 0.0, z), normal: Vec3::Y, uv: vec2(x, z) };
    add_quad(&mut mesh, [vertex(-e, e), vertex(e, e), vertex(e, -e), vertex(-e, -e)]);
    mesh
}

/// The rounded edge and the sides, swept round the top with mitred corners.
pub fn build_board_sides(layout: &Layout) -> Mesh {
    struct SidePoint {
        // Outward from the edge of the flat top, and height.
        out: f32,
        y: f32,
        // The normal.
        n_out: f32,
        n_y: f32,
    }
    let mut profile: Vec<SidePoint> = (0..=4)
        .map(|i| {
            let (s, c) = (PI / 2.0 * i as f32 / 4.0).sin_cos();
            SidePoint { out: BEVEL * s, y: -BEVEL * (1.0 - c), n_out: s, n_y: c }
        })
        .collect();
    profile.push(SidePoint { out: BEVEL, y: -layout.thickness, n_out: 1.0, n_y: 0.0 });

    let inner = layout.board_half - BEVEL;
    let mut mesh = Mesh::default();
    for outward in [vec3(0.0, 0.0, 1.0), vec3(1.0, 0.0, 0.0), vec3(0.0, 0.0, -1.0), vec3(-1.0, 0.0, 0.0)] {
        let along = vec3(-outward.z, 0.0, outward.x);
        for pair in profile.windows(2) {
            let (p, q) = (&pair[0], &pair[1]);
            let (p_extent, q_extent) = (inner + p.out, inner + q.out);
            let vertex = |point: &SidePoint, extent: f32, s: f32| Vertex {
                position: outward * extent + along * s + vec3(0.0, point.y, 0.0),
                normal: (outward * point.n_out + vec3(0.0, point.n_y, 0.0)).normalized(),
                uv: vec2(s, point.y),
            };
            add_quad(&mut mesh, [vertex(p, p_extent, -p_extent), vertex(p, p_extent, p_extent), vertex(q, q_extent, q_extent), vertex(q, q_extent, -q_extent)]);
        }
    }
    mesh
}

pub fn build_table(radius: f32, height: f32) -> Mesh {
    const SEGMENTS: u32 = 96;
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex { position: vec3(0.0, height, 0.0), normal: Vec3::Y, uv: vec2(0.5, 0.5) });
    for i in 0..SEGMENTS {
        let (s, c) = (2.0 * PI * i as f32 / SEGMENTS as f32).sin_cos();
        mesh.vertices.push(Vertex { position: vec3(c * radius, height, s * radius), normal: Vec3::Y, uv: vec2(c * 0.5 + 0.5, s * 0.5 + 0.5) });
    }
    for i in 0..SEGMENTS {
        // Counterclockwise from above: from +x toward -z.
        mesh.indices.extend([0, 1 + (i + 1) % SEGMENTS, 1 + i]);
    }
    mesh
}

/// A bowl standing on y = 0 round the y axis.
pub fn build_bowl(radius: f32, height: f32) -> Mesh {
    let (r, h) = (radius, height);
    let mut profile = vec![(0.0, 0.0), (0.58 * r, 0.0), (0.62 * r, 0.03 * h)];
    // The belly: widest a little below the middle, drawing in to the mouth.
    for i in 1..=24 {
        let t = 0.03 + 0.97 * i as f32 / 24.0;
        let swell = if t < 0.42 { 0.62 + 0.38 * (PI / 2.0 * t / 0.42).sin() } else { 1.0 - 0.17 * ((t - 0.42) / 0.58).powi(2) };
        profile.push((swell * r, t * h));
    }
    // The lip, and the inside down to the heap of stones.
    let heap = heap_height(h) - 0.05;
    profile.extend([(0.815 * r, 1.012 * h), (0.79 * r, 1.012 * h), (0.77 * r, h), (0.74 * r, 0.9 * h), (0.72 * r, heap), (0.4 * r, heap), (0.0, heap)]);
    lathe(&profile, 72)
}

/// A lid turned over, standing on y = 0 round the y axis.
pub fn build_lid(radius: f32) -> Mesh {
    let r = radius;
    let floor = lid_floor(r);
    let profile = [
        (0.0, 0.0),
        (0.84 * r, 0.0),
        (0.9 * r, 0.03 * r),
        (0.96 * r, 0.1 * r),
        (r, 0.19 * r),
        (0.99 * r, 0.22 * r),
        (0.96 * r, 0.225 * r),
        (0.935 * r, 0.2 * r),
        (0.9 * r, 0.11 * r),
        (0.84 * r, floor),
        (0.42 * r, floor),
        (0.0, floor),
    ];
    lathe(&profile, 72)
}
