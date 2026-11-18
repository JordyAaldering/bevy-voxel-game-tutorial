use bevy::prelude::*;

pub const OFFSETS: [(usize, usize, usize); 8] = [
    (0, 0, 0),
    (1, 0, 0),
    (1, 0, 1),
    (0, 0, 1),
    (0, 1, 0),
    (1, 1, 0),
    (1, 1, 1),
    (0, 1, 1),
];

pub struct Triangle {
    pub vertices: [Vec3; 3],
    pub normal: Vec3,
    pub solid_corner: usize,
}

/// Visits the triangles of a binary terrain cell, with outward normals and a nearby solid corner.
/// Corner positions and occupancy must be supplied in [`OFFSETS`] order.
pub fn triangles(corners: [(Vec3, bool); 8], mut visit: impl FnMut(Triangle)) {
    let solid = corners.map(|(_, solid)| solid);
    const EDGES: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
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
            points[i] = corners[a].0.lerp(corners[b].0, 0.5);
        }
    }

    let mut connect = |a: usize, b: usize| {
        for (from, to) in [(a, b), (b, a)] {
            let slot = links[from]
                .iter_mut()
                .find(|link| **link == usize::MAX)
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
                    if !solid[face[i]] {
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
            let next = if links[current][0] != previous {
                links[current][0]
            } else {
                links[current][1]
            };
            assert_ne!(next, usize::MAX, "Open marching-cube contour");
            if next == start {
                break;
            }
            previous = current;
            current = next;
        }

        for [a, b, c] in triangulate_contour(&contour, corners[0].0, corners[6].0) {
            let mut vertices = [contour[a], contour[b], contour[c]];
            let mut normal = (vertices[1] - vertices[0])
                .cross(vertices[2] - vertices[0])
                .normalize_or_zero();
            assert_ne!(normal, Vec3::ZERO, "Degenerate marching-cube triangle");

            let centroid = (vertices[0] + vertices[1] + vertices[2]) / 3.0;
            let solid_corner = (0..8)
                .filter(|&corner| solid[corner])
                .min_by(|&a, &b| {
                    corners[a]
                        .0
                        .distance_squared(centroid)
                        .total_cmp(&corners[b].0.distance_squared(centroid))
                })
                .expect("A surface has a solid corner");

            let p = (centroid - corners[0].0) / (corners[6].0 - corners[0].0);
            let d = solid.map(|value| if value { 1.0 } else { 0.0 });
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

            visit(Triangle {
                vertices,
                normal,
                solid_corner,
            });
        }
    }
}

/// Triangulates a contour using its edge crossings, preferring coplanar adjacent faces.
/// `min` and `max` are the cell's opposite corners; returned indices refer to `contour`.
fn triangulate_contour(contour: &[Vec3], min: Vec3, max: Vec3) -> Vec<[usize; 3]> {
    type Best = Option<(usize, f32, Vec<[usize; 3]>)>;

    /// Allows contour edges and internal diagonals that do not lie on a cell face.
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

    /// Enumerates valid contour triangulations and retains the flattest candidate.
    fn search(
        contour: &[Vec3],
        min: Vec3,
        max: Vec3,
        pending: &mut Vec<(usize, usize)>,
        triangles: &mut Vec<[usize; 3]>,
        best: &mut Best,
    ) {
        let Some((a, b)) = pending.pop() else {
            let normals: Vec<_> = triangles
                .iter()
                .map(|&[a, b, c]| {
                    (contour[b] - contour[a])
                        .cross(contour[c] - contour[a])
                        .normalize()
                })
                .collect();
            let mut coplanar = 0;
            let mut bending = 0.0;
            for i in 0..triangles.len() {
                for j in 0..i {
                    if triangles[i]
                        .iter()
                        .filter(|vertex| triangles[j].contains(vertex))
                        .count()
                        != 2
                    {
                        continue;
                    }
                    let dot = normals[i].dot(normals[j]).clamp(-1.0, 1.0);
                    coplanar += usize::from(dot > 1.0 - 1e-6);
                    bending += 1.0 - dot;
                }
            }
            // Favor larger flat patches, then the least bending between the remaining faces.
            if best
                .as_ref()
                .is_none_or(|(best_coplanar, best_bending, _)| {
                    coplanar > *best_coplanar
                        || (coplanar == *best_coplanar && bending < *best_bending - 1e-6)
                })
            {
                *best = Some((coplanar, bending, triangles.clone()));
            }
            return;
        };

        if b > a + 1 {
            for middle in a + 1..b {
                if !allowed_edge(contour, min, max, a, middle)
                    || !allowed_edge(contour, min, max, middle, b)
                    || (contour[middle] - contour[a])
                        .cross(contour[b] - contour[a])
                        .length_squared()
                        < 1e-10
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
    best.expect("Marching-cube contour has no valid triangulation")
        .2
}
