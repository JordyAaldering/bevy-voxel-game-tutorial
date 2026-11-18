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
    #[cfg(since19)]
    pub fn add_triangle(&mut self, vertices: [Vec3; 3], normals: [Vec3; 3], uvs: [Vec2; 3]) {
        let start = self.vertices.len() as u32;
        self.indices.extend([start, start + 1, start + 2]);
        self.vertices.extend(vertices);
        self.normals.extend(normals);
        self.uvs.extend(uvs);
    }

    /// Polygonizes binary terrain samples at voxel centers.
    #[cfg(since19)]
    pub fn add_binary_marching_cube(
        &mut self,
        corners: [(Vec3, bool); 8],
        texture: impl Fn(usize, Direction) -> Option<usize>,
    ) {
        let solid = corners.map(|(_, solid)| solid);
        let values = corners.map(|(position, solid)| (position, if solid { 1.0 } else { 0.0 }));
        self.add_marching_cube_surface(values, solid, true, texture);
    }

    #[cfg(since19)]
    fn add_marching_cube_surface(
        &mut self,
        corners: [(Vec3, f32); 8],
        solid: [bool; 8],
        connect_diagonals: bool,
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

        if solid.iter().all(|&value| value) || solid.iter().all(|&value| !value) {
            return;
        }

        let mut points = [Vec3::ZERO; 12];
        let mut links = [[usize::MAX; 2]; 12];
        for (i, &(a, b)) in EDGES.iter().enumerate() {
            if solid[a] != solid[b] {
                let da = corners[a].1;
                let db = corners[b].1;
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
                    // Both cells sharing a face must resolve its ambiguity identically.
                    for i in 0..4 {
                        if solid[face[i]] != connect_diagonals {
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

            for [a, b, c] in triangulate_contour(&contour, corners[0].0, corners[6].0) {
                let mut vertices = [contour[a], contour[b], contour[c]];
                let mut normal = (vertices[1] - vertices[0]).cross(vertices[2] - vertices[0]).normalize_or_zero();
                assert_ne!(normal, Vec3::ZERO, "Degenerate marching-cube triangle");

                let centroid = (vertices[0] + vertices[1] + vertices[2]) / 3.0;
                let solid_corner = (0..8)
                    .filter(|&corner| solid[corner])
                    .min_by(|&a, &b| {
                        corners[a].0
                            .distance_squared(centroid)
                            .total_cmp(&corners[b].0.distance_squared(centroid))
                    })
                    .expect("A surface has a solid corner");

                let p = (centroid - corners[0].0) / (corners[6].0 - corners[0].0);
                let d = corners.map(|(_, density)| density);
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
                let Some(texture) = texture(solid_corner, direction) else {
                    continue;
                };

                let uv_quad = quad_uvs(self.atlas, texture);
                let uvs = vertices.map(|position| {
                    let local = position - corners[0].0;
                    let (u, v) = match direction {
                        Direction::Right  => (1.0 - local.z, 1.0 - local.y),
                        Direction::Left   => (      local.z, 1.0 - local.y),
                        Direction::Top    => (      local.x,       local.z),
                        Direction::Bottom => (      local.x, 1.0 - local.z),
                        Direction::Front  => (      local.x, 1.0 - local.y),
                        Direction::Back   => (1.0 - local.x, 1.0 - local.y),
                    };

                    vec2(uv_quad[0].x + u * (uv_quad[1].x - uv_quad[0].x),
                         uv_quad[2].y + v * (uv_quad[0].y - uv_quad[2].y))
                });

                self.add_triangle(vertices, [normal; 3], uvs);
            }
        }
    }
}

#[cfg(since19)]
fn triangulate_contour(contour: &[Vec3], min: Vec3, max: Vec3) -> Vec<[usize; 3]> {
    type Best = Option<(usize, f32, Vec<[usize; 3]>)>;

    fn allowed_edge(contour: &[Vec3], min: Vec3, max: Vec3, a: usize, b: usize) -> bool {
        if b == a + 1 || (a == 0 && b == contour.len() - 1) {
            return true;
        }
        // An internal edge on a cube face would change the shared face contour.
        !(0..3).any(|axis| {
            contour[a][axis] == contour[b][axis]
                && (contour[a][axis] == min[axis] || contour[a][axis] == max[axis])
        })
    }

    fn search(
        contour: &[Vec3],
        min: Vec3,
        max: Vec3,
        pending: &mut Vec<(usize, usize)>,
        triangles: &mut Vec<[usize; 3]>,
        best: &mut Best,
    ) {
        let Some((a, b)) = pending.pop() else {
            let normals: Vec<_> = triangles.iter().map(|&[a, b, c]| {
                (contour[b] - contour[a]).cross(contour[c] - contour[a]).normalize()
            }).collect();
            let mut coplanar = 0;
            let mut bending = 0.0;
            for i in 0..triangles.len() {
                for j in 0..i {
                    if triangles[i].iter().filter(|vertex| triangles[j].contains(vertex)).count() != 2 {
                        continue;
                    }
                    let dot = normals[i].dot(normals[j]).clamp(-1.0, 1.0);
                    coplanar += usize::from(dot > 1.0 - 1e-6);
                    bending += 1.0 - dot;
                }
            }
            // Favor larger flat patches, then the least bending between the remaining faces.
            if best.as_ref().is_none_or(|(best_coplanar, best_bending, _)| {
                coplanar > *best_coplanar
                    || (coplanar == *best_coplanar && bending < *best_bending - 1e-6)
            }) {
                *best = Some((coplanar, bending, triangles.clone()));
            }
            return;
        };

        if b > a + 1 {
            for middle in a + 1..b {
                if !allowed_edge(contour, min, max, a, middle)
                    || !allowed_edge(contour, min, max, middle, b)
                    || (contour[middle] - contour[a]).cross(contour[b] - contour[a]).length_squared() < 1e-10
                {
                    continue;
                }
                triangles.push([a, middle, b]);
                pending.push((a, middle));
                pending.push((middle, b));
                search(contour, min, max, pending, triangles, best);
                pending.pop();
                pending.pop();
                triangles.pop();
            }
        } else {
            search(contour, min, max, pending, triangles, best);
        }
        pending.push((a, b));
    }

    if contour.len() == 3 {
        return vec![[0, 1, 2]];
    }
    let mut best = None;
    search(
        contour,
        min,
        max,
        &mut vec![(0, contour.len() - 1)],
        &mut Vec::new(),
        &mut best,
    );
    best.expect("Marching-cube contour has no valid triangulation").2
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

#[cfg(since19)]
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

#[cfg(all(test, since19))]
mod binary_marching_tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    use std::collections::BTreeSet;

    const CORNERS: [Vec3; 8] = [
        Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 0.0, 0.0),
        Vec3::new(1.0, 0.0, 1.0), Vec3::new(0.0, 0.0, 1.0),
        Vec3::new(0.0, 1.0, 0.0), Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(1.0, 1.0, 1.0), Vec3::new(0.0, 1.0, 1.0),
    ];

    fn geometry(mask: u8, origin: Vec3) -> Mesh {
        let atlas = TextureAtlasLayout::from_grid(UVec2::splat(16), 1, 1, None, None);
        let mut data = MeshData::new(&atlas);
        let corners = std::array::from_fn(|i| (CORNERS[i] + origin, mask & (1 << i) != 0));
        data.add_binary_marching_cube(corners, |_, _| Some(0));
        data.build()
    }

    fn positions(mesh: &Mesh) -> &[[f32; 3]] {
        let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
            panic!("missing positions")
        };
        positions
    }

    #[test]
    fn all_binary_cases_produce_nondegenerate_triangles() {
        for mask in 0u8..=255 {
            let mesh = geometry(mask, Vec3::ZERO);
            let positions = positions(&mesh);
            let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else {
                panic!("missing normals")
            };
            assert_eq!(positions.is_empty(), mask == 0 || mask == 255, "mask {mask}");
            for (vertices, normals) in positions.chunks_exact(3).zip(normals.chunks_exact(3)) {
                let cross = (Vec3::from_array(vertices[1]) - Vec3::from_array(vertices[0]))
                    .cross(Vec3::from_array(vertices[2]) - Vec3::from_array(vertices[0]));
                assert!(cross.length_squared() > 1e-10, "mask {mask}");
                assert!(cross.normalize().dot(Vec3::from_array(normals[0])) > 0.999, "mask {mask}");
                assert!(vertices.iter().flatten().all(|v| (0.0..=1.0).contains(v)), "mask {mask}");
                assert!(vertices.iter().all(|p| p.iter().filter(|&&v| v == 0.5).count() == 1
                    && p.iter().all(|&v| v == 0.0 || v == 0.5 || v == 1.0)), "mask {mask}");
            }
        }
    }

    #[test]
    fn adjacent_triangles_have_consistent_winding() {
        for mask in 0u8..=255 {
            let mesh = geometry(mask, Vec3::ZERO);
            let triangles: Vec<_> = positions(&mesh).chunks_exact(3).collect();
            for (i, first) in triangles.iter().enumerate() {
                for second in &triangles[..i] {
                    for a in 0..3 {
                        for b in 0..3 {
                            assert!(
                                first[a] != second[b] || first[(a + 1) % 3] != second[(b + 1) % 3],
                                "same directed edge in mask {mask}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn diagonal_surface_is_planar() {
        let mesh = geometry(0b1011_1011, Vec3::ZERO);
        assert!(positions(&mesh).iter().all(|p| (p[0] + p[2] - 1.5).abs() < 1e-6));
    }

    #[test]
    fn l_contours_form_flat_and_planar_faces_without_center_vertices() {
        for missing in 0..4 {
            for upper in [false, true] {
                let mask = (0b1111 ^ (1 << missing)) << if upper { 4 } else { 0 };
                let mesh = geometry(mask, Vec3::ZERO);
                let positions = positions(&mesh);
                assert_eq!(positions.len(), 9, "mask {mask}");
                assert!(positions.iter().all(|p| {
                    p.iter().filter(|&&v| v == 0.5).count() == 1
                        && p.iter().all(|&v| v == 0.0 || v == 0.5 || v == 1.0)
                }), "mask {mask}");
                let triangles: Vec<_> = positions.chunks_exact(3).collect();
                assert_eq!(triangles.iter().filter(|triangle| triangle.iter().all(|p| p[1] == 0.5)).count(), 1, "mask {mask}");
                let slopes: Vec<_> = triangles.iter()
                    .filter(|triangle| !triangle.iter().all(|p| p[1] == 0.5))
                    .map(|triangle| {
                        let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(Vec3::from_array);
                        (b - a).cross(c - a).normalize()
                    })
                    .collect();
                assert_eq!(slopes.len(), 2);
                assert!(slopes[0].dot(slopes[1]) > 0.99999, "mask {mask}");
            }
        }
    }

    #[test]
    fn diagonally_touching_solids_are_connected() {
        let mesh = geometry(0b0000_0101, Vec3::ZERO);
        let triangles: Vec<_> = positions(&mesh).chunks_exact(3).collect();
        assert!(!triangles.is_empty());
        let mut reached = vec![false; triangles.len()];
        reached[0] = true;
        loop {
            let mut changed = false;
            for i in 0..triangles.len() {
                if reached[i] {
                    continue;
                }
                if (0..triangles.len()).any(|j| {
                    reached[j] && triangles[i].iter().filter(|vertex| triangles[j].contains(vertex)).count() >= 2
                }) {
                    reached[i] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        assert!(reached.into_iter().all(|value| value));
    }

    #[test]
    fn samples_without_material_do_not_produce_triangles() {
        let atlas = TextureAtlasLayout::from_grid(UVec2::splat(16), 1, 1, None, None);
        let mut data = MeshData::new(&atlas);
        let corners = std::array::from_fn(|i| (CORNERS[i], i == 0 || i == 1));
        data.add_binary_marching_cube(corners, |corner, _| (corner == 0).then_some(0));
        let mesh = data.build();
        let positions = positions(&mesh);
        assert!(!positions.is_empty());
        assert!(positions.chunks_exact(3).all(|triangle| {
            (triangle[0][0] + triangle[1][0] + triangle[2][0]) / 3.0 <= 0.5
        }));
    }

    fn face_segments(mesh: &Mesh) -> BTreeSet<((i32, i32), (i32, i32))> {
        let mut segments = BTreeSet::new();
        for triangle in positions(mesh).chunks_exact(3) {
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let (a, b) = (triangle[a], triangle[b]);
                if (a[0] - 1.0).abs() < 1e-6 && (b[0] - 1.0).abs() < 1e-6 {
                    let point = |p: [f32; 3]| (
                        (p[1] * 1_000_000.0).round() as i32,
                        (p[2] * 1_000_000.0).round() as i32,
                    );
                    let (a, b) = (point(a), point(b));
                    if a != b {
                        segments.insert((a, b));
                    }
                }
            }
        }
        segments
    }

    #[test]
    fn adjacent_cells_resolve_all_shared_faces_consistently() {
        let left_meshes: Vec<_> = (0u8..=255).map(|mask| face_segments(&geometry(mask, Vec3::ZERO))).collect();
        let right_meshes: Vec<_> = (0u8..=255).map(|mask| {
            face_segments(&geometry(mask, Vec3::X)).into_iter().map(|(a, b)| (b, a)).collect::<BTreeSet<_>>()
        }).collect();
        for face in 0u8..16 {
            for left_other in 0u8..16 {
                for right_other in 0u8..16 {
                    let mut left = 0u8;
                    let mut right = 0u8;
                    for (i, (l, r)) in [(1, 0), (2, 3), (6, 7), (5, 4)].into_iter().enumerate() {
                        if face & (1 << i) != 0 {
                            left |= 1 << l;
                            right |= 1 << r;
                        }
                    }
                    for (i, (l, r)) in [(0, 1), (3, 2), (7, 6), (4, 5)].into_iter().enumerate() {
                        if left_other & (1 << i) != 0 {
                            left |= 1 << l;
                        }
                        if right_other & (1 << i) != 0 {
                            right |= 1 << r;
                        }
                    }
                    assert_eq!(
                        left_meshes[left as usize],
                        right_meshes[right as usize],
                        "face {face}, left {left_other}, right {right_other}",
                    );
                }
            }
        }
    }
}
