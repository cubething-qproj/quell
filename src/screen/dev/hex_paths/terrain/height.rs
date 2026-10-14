//! Raster sampling and the noise utilities used before layout generation.
//!
//! [`Region::simulate`](super::erosion::Region::simulate) uses [`standard_normal`] and [`blur`]
//! to create its initial broad rolls and roughness. [`Window`](super::window::Window)
//! stores the chosen crop as a [`Raster`], sampled by [`Terrain`](super::Terrain) for
//! corner heights and by [`views`](super::sites::views) for bench sight lines.
//! The raster uses board edge lengths; the region itself uses metres.

use crate::prelude::*;
use rand::{Rng, RngExt};
use std::f32::consts::TAU;

/// The fine height crop used to construct [`Terrain`](super::Terrain).
///
/// [`Window::choose`](super::window::Window::choose) samples an eroded region into this board
/// raster. [`Self::sample`] supplies hex-corner heights and ground along bench
/// sight lines. Layout tease visibility instead samples the levelled hex mesh.
/// Coordinates and heights use hex edge lengths; entries are column-major,
/// with cell `(x, y)` stored at `data[x * height + y]`.
#[derive(Debug, Clone)]
pub struct Raster {
    /// Board-plane position of raster cell `(0, 0)`, in edge lengths.
    pub origin: Vec2,
    /// Cells per hex edge length.
    pub res: f32,
    /// Cells along the board-plane X axis.
    pub width: usize,
    /// Cells along the board-plane Y axis.
    pub height: usize,
    /// Column-major heights in edge lengths, with `width × height` entries.
    pub data: Vec<f32>,
}

impl Raster {
    /// Samples a board-plane point bilinearly, returning height in edge lengths.
    ///
    /// Converts `point` using [`Self::origin`] and [`Self::res`], then clamps to the
    /// raster boundary. This is the common sampler for terrain corners and the
    /// ground sight lines in [`views`](super::sites::views).
    pub fn sample(&self, point: Vec2) -> f32 {
        let at = ((point - self.origin) * self.res)
            .clamp(Vec2::ZERO, Vec2::new(self.width as f32 - 1., self.height as f32 - 1.));
        let (x0, y0) = (at.x.floor() as usize, at.y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (at.x.fract(), at.y.fract());
        let cell = |x: usize, y: usize| self.data[x * self.height + y];
        let near = cell(x0, y0) * (1. - fy) + cell(x0, y1) * fy;
        let far = cell(x1, y0) * (1. - fy) + cell(x1, y1) * fy;
        near * (1. - fx) + far * fx
    }
}

/// Blurs a column-major raster with a separable Gaussian kernel.
///
/// `sigma` is in cells and must be positive. Each axis wraps at its boundaries,
/// and the kernel is truncated at four sigma. Used by
/// [`Region::simulate`](super::erosion::Region::simulate) to turn white noise into broad rolls.
pub fn blur(data: Vec<f32>, width: usize, height: usize, sigma: f32) -> Vec<f32> {
    let radius = (4. * sigma + 0.5) as isize;
    let kernel: Vec<f32> = (-radius..=radius)
        .map(|i| (-(i * i) as f32 / (2. * sigma * sigma)).exp())
        .collect();
    let sum: f32 = kernel.iter().sum();
    let pass = |data: &[f32], stride: usize, len: usize, lines: usize, step: usize| {
        let mut out = vec![0.; data.len()];
        for line in 0..lines {
            for i in 0..len {
                out[line * step + i * stride] = kernel
                    .iter()
                    .enumerate()
                    .map(|(k, w)| {
                        let j = (i as isize + k as isize - radius).rem_euclid(len as isize);
                        w * data[line * step + j as usize * stride]
                    })
                    .sum::<f32>()
                    / sum;
            }
        }
        out
    };
    // Along y within each column, then along x within each row.
    let columns = pass(&data, 1, height, width, height);
    pass(&columns, height, width, height, 1)
}

/// A standard normal sample, by [`Box–Muller.`](https://en.wikipedia.org/wiki/Box%E2%80%93Muller_transform)
///
/// Transforms two uniform draws into one zero-mean, unit-variance value.
/// [`Region::simulate`](super::erosion::Region::simulate) scales these samples for rolls and
/// roughness.
pub fn standard_normal(rng: &mut impl Rng) -> f32 {
    let u: f32 = 1. - rng.random::<f32>();
    let v: f32 = rng.random();
    (-2. * u.ln()).sqrt() * (TAU * v).cos()
}
