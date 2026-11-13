use crate::*;

use std::marker::PhantomData;

use bevy::{
    prelude::*,
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
};

#[cfg_attr(until06, derive(Default))]
pub struct MeshData<'a> {
    #[cfg(since07)]
    atlas: &'a TextureAtlasLayout,
    indices: Vec<u32>,
    vertices: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<Vec2>,
    _marker: PhantomData<&'a ()>,
}

impl<'a> MeshData<'a> {
    #[cfg(since07)]
    pub fn new(atlas: &'a TextureAtlasLayout) -> Self {
        Self {
            atlas,
            vertices: Vec::new(),
            indices: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            _marker: PhantomData,
        }
    }

    pub fn build(&mut self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, std::mem::take(&mut self.vertices))
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, std::mem::take(&mut self.normals))
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, std::mem::take(&mut self.uvs))
            .with_inserted_indices(Indices::U32(std::mem::take(&mut self.indices)))
    }

    #[cfg(until05)]
    pub fn add_quad(&mut self, direction: Direction, origin: Vec3) {
        let start = self.vertices.len() as u32;
        self.indices.extend(quad_indices(start));
        self.vertices.extend(quad_vertices(direction, origin));
        self.normals.extend(quad_normals(direction));
        self.uvs.extend(quad_uvs());
    }

    #[cfg(only06)]
    pub fn add_quad(&mut self, direction: Direction, origin: Vec3, uvs: [Vec2; 4]) {
        let start = self.vertices.len() as u32;
        self.indices.extend(quad_indices(start));
        self.vertices.extend(quad_vertices(direction, origin));
        self.normals.extend(quad_normals(direction));
        self.uvs.extend(uvs);
    }

    #[cfg(since07)]
    pub fn add_quad(&mut self, direction: Direction, origin: Vec3, texture: usize) {
        let start = self.vertices.len() as u32;
        self.indices.extend(quad_indices(start));
        self.vertices.extend(quad_vertices(direction, origin));
        self.normals.extend(quad_normals(direction));
        self.uvs.extend(quad_uvs(self.atlas, texture));
    }

    /// Appends a triangle with mesh-local positions and atlas-space UVs.
    #[cfg(since20)]
    pub fn add_triangle(&mut self, vertices: [Vec3; 3], normals: [Vec3; 3], uvs: [Vec2; 3]) {
        let start = self.vertices.len() as u32;
        self.indices.extend([start, start + 1, start + 2]);
        self.vertices.extend(vertices);
        self.normals.extend(normals);
        self.uvs.extend(uvs);
    }

    /// Polygonizes a cell's mesh-local corner positions and densities at 0.5.
    /// Corners wind around X/Z on the lower layer, followed by the upper layer:
    /// (0,0,0), (1,0,0), (1,0,1), (0,0,1), then the same order at y = 1.
    /// `texture` resolves the material of a solid corner for a face direction.
    /// Solid corners without a material are allowed and do not produce triangles.
    #[cfg(since20)]
    pub fn add_marching_cube(
        &mut self,
        corners: [(Vec3, Density); 8],
        texture: impl Fn(usize, Direction) -> Option<usize>,
    ) {
        const EDGES: [(usize, usize); 12] = [
            (0, 1), (1, 2), (2, 3), (3, 0),
            (4, 5), (5, 6), (6, 7), (7, 4),
            (0, 4), (1, 5), (2, 6), (3, 7),
        ];
        // Each face lists corners and edges in the same cyclic order.
        const FACES: [([usize; 4], [usize; 4]); 6] = [
            ([0, 1, 2, 3], [0, 1, 2, 3]),
            ([4, 5, 6, 7], [4, 5, 6, 7]),
            ([0, 1, 5, 4], [0, 9, 4, 8]),
            ([3, 2, 6, 7], [2, 10, 6, 11]),
            ([0, 3, 7, 4], [3, 11, 7, 8]),
            ([1, 2, 6, 5], [1, 10, 5, 9]),
        ];
        let solid = corners.map(|(_, density)| density != Density::EMPTY);
        if solid.iter().all(|&value| value) || solid.iter().all(|&value| !value) {
            return;
        }
        let mut points = [Vec3::ZERO; 12];
        let mut links = [[usize::MAX; 2]; 12];
        for (i, &(a, b)) in EDGES.iter().enumerate() {
            if solid[a] != solid[b] {
                let da: f32 = corners[a].1.into();
                let db: f32 = corners[b].1.into();
                let t = (0.5 - da) / (db - da);
                points[i] = corners[a].0.lerp(corners[b].0, t);
            }
        }
        let mut connect = |a: usize, b: usize| {
            for (from, to) in [(a, b), (b, a)] {
                let slot = links[from].iter_mut().find(|link| **link == usize::MAX)
                    .expect("A marching-cube edge must have at most two connections");
                *slot = to;
            }
        };
        for (face, edges) in FACES {
            let crossings: Vec<_> = (0..4)
                .filter(|&i| solid[face[i]] != solid[face[(i + 1) % 4]])
                .collect();
            match crossings.as_slice() {
                [] => {}
                [a, b] => connect(edges[*a], edges[*b]),
                [_, _, _, _] => {
                    // Pair edges around solid corners; both cells sharing a
                    // face make the same choice for an ambiguous face.
                    for i in 0..4 {
                        if solid[face[i]] {
                            connect(edges[(i + 3) % 4], edges[i]);
                        }
                    }
                }
                _ => unreachable!("A cube face has an even number of crossings"),
            }
        }
        let mut visited = [false; 12];
        for start in 0..12 {
            if links[start][0] == usize::MAX || visited[start] {
                continue;
            }
            let mut contour = Vec::new();
            let mut previous = usize::MAX;
            let mut current = start;
            loop {
                assert!(!visited[current], "Invalid marching-cube contour");
                visited[current] = true;
                contour.push(points[current]);
                let next = if links[current][0] != previous { links[current][0] } else { links[current][1] };
                assert_ne!(next, usize::MAX, "Open marching-cube contour");
                if next == start {
                    break;
                }
                previous = current;
                current = next;
            }
            let center = contour.iter().copied().sum::<Vec3>() / contour.len() as f32;
            for i in 0..contour.len() {
                let mut vertices = [center, contour[i], contour[(i + 1) % contour.len()]];
                let mut normal = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]).normalize_or_zero();
                if normal == Vec3::ZERO {
                    continue;
                }
                let centroid = (vertices[0] + vertices[1] + vertices[2]) / 3.0;
                let solid_corner = (0..8)
                    .filter(|&corner| solid[corner])
                    .min_by(|&a, &b| {
                        corners[a].0.distance_squared(centroid).total_cmp(&corners[b].0.distance_squared(centroid))
                    })
                    .expect("A surface has a solid corner");
                let p = (centroid - corners[0].0) / (corners[6].0 - corners[0].0);
                let d: [f32; 8] = corners.map(|(_, density)| density.into());
                let gradient = vec3(
                    (d[1] - d[0]) * (1.0 - p.y) * (1.0 - p.z)
                        + (d[2] - d[3]) * (1.0 - p.y) * p.z
                        + (d[5] - d[4]) * p.y * (1.0 - p.z)
                        + (d[6] - d[7]) * p.y * p.z,
                    (d[4] - d[0]) * (1.0 - p.x) * (1.0 - p.z)
                        + (d[5] - d[1]) * p.x * (1.0 - p.z)
                        + (d[7] - d[3]) * (1.0 - p.x) * p.z
                        + (d[6] - d[2]) * p.x * p.z,
                    (d[3] - d[0]) * (1.0 - p.x) * (1.0 - p.y)
                        + (d[2] - d[1]) * p.x * (1.0 - p.y)
                        + (d[7] - d[4]) * (1.0 - p.x) * p.y
                        + (d[6] - d[5]) * p.x * p.y,
                );
                let outward = if gradient.length_squared() > 0.0 {
                    -gradient
                } else {
                    centroid - corners[solid_corner].0
                };
                if normal.dot(outward) < 0.0 {
                    vertices.swap(1, 2);
                    normal = -normal;
                }
                let direction = normal_direction(normal);
                let Some((_, texture)) = (0..8)
                    .filter(|&corner| solid[corner])
                    .min_by(|&a, &b| {
                        corners[a].0.distance_squared(centroid).total_cmp(&corners[b].0.distance_squared(centroid))
                    })
                    .and_then(|corner| texture(corner, direction).map(|texture| (corner, texture)))
                else {
                    continue;
                };
                let uv_quad = quad_uvs(self.atlas, texture);
                let uvs = vertices.map(|position| {
                    let local = position - corners[0].0;
                    let (u, v) = match direction {
                        Direction::Right => (1.0 - local.z, 1.0 - local.y),
                        Direction::Left => (local.z, 1.0 - local.y),
                        Direction::Top => (local.x, local.z),
                        Direction::Bottom => (local.x, 1.0 - local.z),
                        Direction::Front => (local.x, 1.0 - local.y),
                        Direction::Back => (1.0 - local.x, 1.0 - local.y),
                    };
                    vec2(uv_quad[0].x + u * (uv_quad[1].x - uv_quad[0].x),
                         uv_quad[2].y + v * (uv_quad[0].y - uv_quad[2].y))
                });
                self.add_triangle(vertices, [normal; 3], uvs);
            }
        }
    }
}

/// The vertices for a quad in the given direction.
///
/// ```text
///     (0,1,0)—————————————(1,1,0)
///         / |            / |
///        /  |           /  |
/// (0,1,1)————————(1,1,1)   |
///       |   |          |   |
///       |   |          |   |
///       |  (0,0,0)—————|——(1,0,0)
///       | /            | /
///       |/             |/
/// (0,0,1)——————————————(1,0,1)
/// ```
pub fn quad_vertices(direction: Direction, origin: Vec3) -> [Vec3; 4] {
    match direction {
        Direction::Right => [
            origin + Vec3::X + Vec3::Z,
            origin + Vec3::X,
            origin + Vec3::X + Vec3::Y + Vec3::Z,
            origin + Vec3::X + Vec3::Y,
        ],
        Direction::Left => [
            origin,
            origin + Vec3::Z,
            origin + Vec3::Y,
            origin + Vec3::Y + Vec3::Z,
        ],
        Direction::Top => [
            origin + Vec3::Y + Vec3::Z,
            origin + Vec3::X + Vec3::Y + Vec3::Z,
            origin + Vec3::Y,
            origin + Vec3::X + Vec3::Y,
        ],
        Direction::Bottom => [
            origin,
            origin + Vec3::X,
            origin + Vec3::Z,
            origin + Vec3::X + Vec3::Z,
        ],
        Direction::Front => [
            origin + Vec3::Z,
            origin + Vec3::X + Vec3::Z,
            origin + Vec3::Y + Vec3::Z,
            origin + Vec3::X + Vec3::Y + Vec3::Z,
        ],
        Direction::Back => [
            origin + Vec3::X,
            origin,
            origin + Vec3::X + Vec3::Y,
            origin + Vec3::Y,
        ],
    }
}

/// The vertex indices for a quad (two triangles).
///
/// ```text
/// 2————3
/// | \  |
/// |  \ |
/// 0————1
/// ```
fn quad_indices(start: u32) -> [u32; 6] {
    [
        // First triangle
        start + 0, // Bottom left
        start + 1, // Bottom right
        start + 2, // Top left
        // Second triangle
        start + 2, // Top left
        start + 1, // Bottom right
        start + 3, // Top right
    ]
}

/// The normals of the corners of a quad.
fn quad_normals(direction: Direction) -> [Vec3; 4] {
    let normal = match direction {
        Direction::Right  => Vec3::X,
        Direction::Left   => Vec3::NEG_X,
        Direction::Top    => Vec3::Y,
        Direction::Bottom => Vec3::NEG_Y,
        Direction::Front  => Vec3::Z,
        Direction::Back   => Vec3::NEG_Z,
    };
    [normal; 4]
}

/// The UV coordinates of the corners of a quad.
///
/// Note that the y-axis is flipped in UV coordinates;
/// the bottom left voxel has $y = 1$ in UV coordinates.
#[cfg(until05)]
fn quad_uvs() -> [Vec2; 4] {
    [
        vec2(0.0, 1.0), // Bottom left
        vec2(1.0, 1.0), // Bottom right
        vec2(0.0, 0.0), // Top left
        vec2(1.0, 0.0), // Top right
    ]
}

#[cfg(since07)]
fn quad_uvs(atlas: &TextureAtlasLayout, index: usize) -> [Vec2; 4] {
    let rect = atlas.textures[index];
    let size = atlas.size.as_vec2();
    // Inset a little bit so that we don't sample neighboring tiles, causing thin white lines
    let inset = Vec2::splat(0.01);
    let min = (rect.min.as_vec2() + inset) / size;
    let max = (rect.max.as_vec2() - inset) / size;
    [
        vec2(min.x, max.y), // Bottom left
        vec2(max.x, max.y), // Bottom right
        vec2(min.x, min.y), // Top left
        vec2(max.x, min.y), // Top right
    ]
}

#[cfg(since20)]
fn normal_direction(normal: Vec3) -> Direction {
    let abs = normal.abs();
    if abs.y >= abs.x && abs.y >= abs.z {
        if normal.y >= 0.0 { Direction::Top } else { Direction::Bottom }
    } else if abs.x >= abs.z {
        if normal.x >= 0.0 { Direction::Right } else { Direction::Left }
    } else if normal.z >= 0.0 {
        Direction::Front
    } else {
        Direction::Back
    }
}
