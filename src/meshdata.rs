use crate::Direction;

use std::mem;

use bevy::{
    prelude::*,
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
};

pub struct MeshData<'a> {
    atlas: &'a TextureAtlasLayout,
    indices: Vec<u32>,
    vertices: Vec<Vec3>,
    normals: Vec<Vec3>,
    uvs: Vec<Vec2>,
}

impl<'a> MeshData<'a> {
    pub fn new(atlas: &'a TextureAtlasLayout) -> Self {
        Self {
            atlas,
            vertices: Vec::new(),
            indices: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
        }
    }

    pub fn build(&mut self) -> Mesh {
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, mem::take(&mut self.vertices))
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, mem::take(&mut self.normals))
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, mem::take(&mut self.uvs))
            .with_inserted_indices(Indices::U32(mem::take(&mut self.indices)))
    }

    pub fn add_quad(&mut self, direction: Direction, origin: Vec3, texture: usize) {
        let start = self.vertices.len() as u32;
        self.indices.extend(quad_indices(start));
        self.vertices.extend(quad_vertices(direction, origin));
        self.normals.extend(quad_normals(direction));
        self.uvs.extend(quad_uvs(self.atlas, texture));
    }
}

/// Returns the four vertices of the quad.
///
/// The quad is defined by the direction of the face and the origin of the voxel.
///
/// ```text
///     (0,1,0)——————————————————(1,1,0)
///          /|                 /|
///         / |                / |
///        /  |               /  |
/// (0,1,1)————————————(1,1,1)   |
///       |   |              |   |
///       |   |              |   |
///       |   |              |   |
///       |  (0,0,0)—————————|——(1,0,0)
///       | /                | /
///       |/                 |/
/// (0,0,1)——————————————————(1,0,1)
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

/// Returns the vertex indices for a quad (two triangles).
///
/// Vertices are ordered as viewed from outside the voxel:
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

fn quad_normals(direction: Direction) -> [Vec3; 4] {
    [direction.vec3(); 4]
}

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
