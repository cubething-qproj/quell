//! Shared hex geometry for ground sampling, creek barriers, and trail routing.
//!
//! [`HexMesh::build`] deduplicates neighbouring tiles' corners and sides,
//! creating a corner graph for [`Layout`](super::super::layout::Layout). Tiles and
//! centres also provide the dual graph used by [`Creeks`](super::window::Creeks).
//! [`HexMesh::shared_side`] converts a centre-to-centre creek link into the
//! trail edge it blocks. Playable masks separate the trail board from scenery.
//!
//! Positions are in hex edge lengths on a 2D plane. [`Terrain`](super::Terrain) supplies
//! original corner heights; a layout supplies a levelled copy. The preview
//! combines either height array with [`HexMesh::triangles`] to draw the surface.

use super::SQRT_3;
use crate::prelude::*;
use bevy::platform::collections::HashMap;
use hexx::{Hex, HexLayout, HexOrientation, OffsetHexMode};

const OFFSET_MODE: OffsetHexMode = OffsetHexMode::Even;

/// The indexed geometry shared by [`Terrain`](super::Terrain) and its layouts.
///
/// Tile indices address centres and six-corner pads; corner indices address
/// shared positions; edge indices address shared hex sides. Trails route on
/// the corner graph, while [`Creeks`](super::window::Creeks) link tile centres.
/// [`Self::shared_side`] translates creek links into trail barriers.
///
/// [`Self::build`] surrounds the playable board with a scenery margin. Pad
/// placement uses interior tiles, while trail routing uses playable edges.
/// All planar coordinates are in hex edge lengths; heights live in terrain
/// and layout data rather than this geometry.
#[derive(Debug, Clone)]
pub struct HexMesh {
    /// Hex coordinates in tile-index order, including the scenery margin.
    pub tiles: Vec<Hex>,
    /// Board-plane centres corresponding to [`Self::tiles`], in edge lengths.
    pub centers: Vec<Vec2>,
    /// Each tile's corners, as indices into `positions`.
    pub tile_corners: Vec<[usize; 6]>,
    /// Deduplicated corner positions, in edge lengths; indexed by terrain heights.
    pub positions: Vec<Vec2>,
    /// Per tile: whether it belongs to the playable board rather than scenery.
    pub playable: Vec<bool>,
    /// Distance from each tile centre to the playable board's edge, in hexes:
    /// 0.5 for a playable tile on the edge, negative in the margin.
    pub border_distance: Vec<f32>,
    /// Tile index of each hex.
    pub index: HashMap<Hex, usize>,
    /// The honeycomb: tile sides as corner pairs, lowest index first.
    pub edges: Vec<[usize; 2]>,
    /// Edges between playable corners; only these take trails.
    pub playable_edges: Vec<bool>,
    /// Each edge's index, by its corners (lowest first).
    pub edge_index: HashMap<[usize; 2], usize>,
    /// Each corner's edges, as (other corner, edge).
    pub corner_edges: Vec<Vec<(usize, usize)>>,
}

impl HexMesh {
    /// Builds a flat-top `columns × rows` board with `margin` scenery tiles per side.
    ///
    /// Uses even-q offset coordinates and deduplicates lattice corners and sides,
    /// then builds edge lookup and corner adjacency for routing. Callers should
    /// use an even margin to preserve the playable board's column parity, as
    /// checked by [`HexTerrainTuning::validate`](super::super::data::HexTerrainTuning::validate).
    pub fn build(columns: u32, rows: u32, margin: u32) -> Self {
        let layout = HexLayout::flat();
        let mut keys = HashMap::<(i32, i32), usize>::default();
        let mut mesh = Self {
            tiles: Vec::new(),
            centers: Vec::new(),
            tile_corners: Vec::new(),
            positions: Vec::new(),
            playable: Vec::new(),
            border_distance: Vec::new(),
            index: HashMap::default(),
            edges: Vec::new(),
            playable_edges: Vec::new(),
            edge_index: HashMap::default(),
            corner_edges: Vec::new(),
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
                mesh.index.insert(hex, mesh.tiles.len());
                mesh.tiles.push(hex);
                mesh.centers.push(layout.hex_to_world_pos(hex));
                mesh.tile_corners.push(corners);
                mesh.playable.push(
                    (margin..margin + columns).contains(&column)
                        && (margin..margin + rows).contains(&row),
                );
                let (c, r) = (column as i32 - margin as i32, row as i32 - margin as i32);
                let edge = [c, columns as i32 - 1 - c, r, rows as i32 - 1 - r];
                mesh.border_distance
                    .push(edge.into_iter().min().unwrap_or(0) as f32 + 0.5);
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
        mesh.corner_edges = vec![Vec::new(); mesh.positions.len()];
        for (e, &[a, b]) in edges.iter().enumerate() {
            mesh.edge_index.insert([a, b], e);
            mesh.corner_edges[a].push((b, e));
            mesh.corner_edges[b].push((a, e));
        }
        mesh.edges = edges;
        mesh
    }

    /// Looks up a tile using offset coordinates relative to the playable board.
    ///
    /// `margin` must match [`Self::build`]. Negative or oversized coordinates can
    /// reach scenery tiles; returns `None` if the resulting hex is absent.
    pub fn tile_at(&self, column: i32, row: i32, margin: u32) -> Option<usize> {
        let hex = Hex::from_offset_coordinates(
            [column + margin as i32, row + margin as i32],
            OFFSET_MODE,
            HexOrientation::Flat,
        );
        self.index.get(&hex).copied()
    }

    /// Finds the hex containing `point`, including scenery; `None` outside the mesh.
    ///
    /// Uses hex-coordinate rounding instead of snapping to the nearest tile that
    /// exists. Points beyond the jagged outer boundary can therefore be skipped
    /// when [`window`](super::window) traces creeks.
    pub fn tile_containing(&self, point: Vec2) -> Option<usize> {
        self.index
            .get(&HexLayout::flat().world_pos_to_hex(point))
            .copied()
    }

    /// The edge between corners `a` and `b`, if they are neighbours.
    pub fn edge(&self, a: usize, b: usize) -> Option<usize> {
        self.edge_index.get(&[a.min(b), a.max(b)]).copied()
    }

    /// Returns the six edge indices around a tile, in corner order.
    ///
    /// Edge `i` joins corners `(i + 5) % 6` and `i`. Crossing-bank splitting relies
    /// on this ordering. `tile` must be a valid index into [`Self::tile_corners`].
    pub fn perimeter(&self, tile: usize) -> [usize; 6] {
        let corners = self.tile_corners[tile];
        std::array::from_fn(|i| {
            self.edge(corners[(i + 5) % 6], corners[i])
                .expect("a tile's consecutive corners share an edge")
        })
    }

    /// Returns the shared edge index if tiles `a` and `b` share exactly two corners.
    ///
    /// [`Terrain::creek_sides`](super::Terrain::creek_sides) uses this to locate a trail barrier. Tiles
    /// that are not neighbours have no shared side and return `None`.
    pub fn shared_side(&self, a: usize, b: usize) -> Option<usize> {
        let shared: Vec<usize> = self.tile_corners[a]
            .into_iter()
            .filter(|c| self.tile_corners[b].contains(c))
            .collect();
        match shared[..] {
            [x, y] => self.edge(x, y),
            _ => None,
        }
    }

    /// Tiles sharing a side with `tile`.
    pub fn neighbours(&self, tile: usize) -> impl Iterator<Item = usize> + '_ {
        self.tiles[tile]
            .all_neighbors()
            .into_iter()
            .filter_map(|hex| self.index.get(&hex).copied())
    }

    /// Playable tiles at least one tile in from the playable edge.
    pub fn interior(&self) -> Vec<bool> {
        self.border_distance.iter().map(|&d| d >= 1.5).collect()
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

    /// Triangulates every tile as six triangles around its centre.
    ///
    /// Indices refer to shared corners first, then centres in tile order, matching
    /// [`point_heights`](super::point_heights). The renderer corrects winding when converting
    /// the board plane to world coordinates.
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
            .fold((Vec2::MAX, Vec2::MIN), |(lo, hi), &p| {
                (lo.min(p), hi.max(p))
            });
        for (position, _) in mesh.positions.iter().zip(&inside).filter(|(_, i)| **i) {
            assert!(position.x > min.x + 1. && position.x < max.x - 1.);
            assert!(position.y > min.y + 1. && position.y < max.y - 1.);
        }
    }
}
