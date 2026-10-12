use crate::prelude::*;
use bevy::platform::collections::HashMap;
use hexx::{Hex, HexLayout, HexOrientation, OffsetHexMode};
use super::SQRT_3;

const OFFSET_MODE: OffsetHexMode = OffsetHexMode::Even;

/// Tiles and their shared corners, in hex edge lengths. Only playable tiles
/// take pads and trails; the margin around them is scenery.
#[derive(Debug, Clone)]
pub struct HexMesh {
    pub tiles: Vec<Hex>,
    pub centers: Vec<Vec2>,
    /// Each tile's corners, as indices into `positions`.
    pub tile_corners: Vec<[usize; 6]>,
    pub positions: Vec<Vec2>,
    pub playable: Vec<bool>,
    /// The honeycomb: tile sides as corner pairs, lowest index first.
    pub edges: Vec<[usize; 2]>,
    /// Edges between playable corners; only these take trails.
    pub playable_edges: Vec<bool>,
}

impl HexMesh {
    /// A `columns` × `rows` playable board, even-q offset, inside `margin`
    /// extra tiles of scenery on every side.
    pub fn build(columns: u32, rows: u32, margin: u32) -> Self {
        let layout = HexLayout::flat();
        let mut keys = HashMap::<(i32, i32), usize>::default();
        let mut mesh = Self {
            tiles: Vec::new(),
            centers: Vec::new(),
            tile_corners: Vec::new(),
            positions: Vec::new(),
            playable: Vec::new(),
            edges: Vec::new(),
            playable_edges: Vec::new(),
        };
        for column in 0..columns + 2 * margin {
            for row in 0..rows + 2 * margin {
                let hex = Hex::from_offset_coordinates(
                    [column as i32, row as i32],
                    OFFSET_MODE,
                    HexOrientation::Flat,
                );
                // Corners lie on a lattice, so rounding identifies shared ones.
                let corners = layout.hex_corners(hex).map(|corner| {
                    let key = (
                        (2. * corner.x).round() as i32,
                        (2. * corner.y / SQRT_3).round() as i32,
                    );
                    *keys.entry(key).or_insert_with(|| {
                        mesh.positions.push(corner);
                        mesh.positions.len() - 1
                    })
                });
                mesh.tiles.push(hex);
                mesh.centers.push(layout.hex_to_world_pos(hex));
                mesh.tile_corners.push(corners);
                mesh.playable.push(
                    (margin..margin + columns).contains(&column)
                        && (margin..margin + rows).contains(&row),
                );
            }
        }
        let mut edges: Vec<[usize; 2]> = mesh
            .tile_corners
            .iter()
            .flat_map(|corners| {
                (0..6).map(move |i| {
                    let (a, b) = (corners[i], corners[(i + 1) % 6]);
                    [a.min(b), a.max(b)]
                })
            })
            .collect();
        edges.sort();
        edges.dedup();
        let inside = mesh.playable_corners();
        mesh.playable_edges = edges.iter().map(|&[a, b]| inside[a] && inside[b]).collect();
        mesh.edges = edges;
        mesh
    }

    /// Whether each corner belongs to a playable tile.
    pub fn playable_corners(&self) -> Vec<bool> {
        let mut inside = vec![false; self.positions.len()];
        self.tile_corners
            .iter()
            .zip(&self.playable)
            .filter(|(_, playable)| **playable)
            .flat_map(|(corners, _)| corners)
            .for_each(|&corner| inside[corner] = true);
        inside
    }

    /// Each tile as 6 triangles around its centre. Indices count corners
    /// first, then centres.
    pub fn triangles(&self) -> impl Iterator<Item = [usize; 3]> + '_ {
        let first_centre = self.positions.len();
        self.tile_corners
            .iter()
            .enumerate()
            .flat_map(move |(tile, corners)| {
                (0..6).map(move |i| [first_centre + tile, corners[i], corners[(i + 1) % 6]])
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neighbouring_tiles_share_corners() {
        let mesh = HexMesh::build(4, 3, 2);
        let mut tiles_per_corner = vec![0; mesh.positions.len()];
        for corners in &mesh.tile_corners {
            let mut distinct = corners.to_vec();
            distinct.sort();
            distinct.dedup();
            assert_eq!(distinct.len(), 6);
            corners.iter().for_each(|&c| tiles_per_corner[c] += 1);
        }
        assert!(tiles_per_corner.iter().all(|&n| (1..=3).contains(&n)));
        assert!(tiles_per_corner.contains(&3));
    }

    #[test]
    fn edges_form_a_honeycomb() {
        let mesh = HexMesh::build(4, 3, 2);
        let mut degree = vec![0; mesh.positions.len()];
        for &[a, b] in &mesh.edges {
            degree[a] += 1;
            degree[b] += 1;
        }
        assert!(degree.iter().all(|&d| (2..=3).contains(&d)));
        assert!(mesh.playable_edges.contains(&true));
    }

    #[test]
    fn playable_tiles_are_inside_the_margin() {
        let mesh = HexMesh::build(4, 3, 2);
        assert_eq!(mesh.playable.iter().filter(|&&p| p).count(), 12);
        let inside = mesh.playable_corners();
        let (min, max) = mesh
            .positions
            .iter()
            .fold((Vec2::MAX, Vec2::MIN), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        for (position, _) in mesh.positions.iter().zip(&inside).filter(|(_, i)| **i) {
            assert!(position.x > min.x + 1. && position.x < max.x - 1.);
            assert!(position.y > min.y + 1. && position.y < max.y - 1.);
        }
    }
}
