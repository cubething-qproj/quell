use super::{super::data::*, HexMesh};
use crate::prelude::*;
use rand::{Rng, RngExt};
use super::SQRT_3;
use std::f32::consts::TAU;

/// Fine height raster the corners are sampled from; also used for line of
/// sight. Stored column-major: `data[x * height + y]`.
#[derive(Debug, Clone)]
pub struct Raster {
    pub origin: Vec2,
    /// Cells per hex edge length.
    pub res: f32,
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Raster {
    /// Fractal noise (octaves of blurred white noise), blended with a ridged
    /// copy, then reshaped so lowlands flatten and scaled to `relief`.
    pub fn heightmap(
        mesh: &HexMesh,
        settings: &HexPathsSettings,
        tuning: &HexTerrainTuning,
        rng: &mut impl Rng,
    ) -> Self {
        let res = tuning.resolution;
        let (min, max) = mesh
            .positions
            .iter()
            .fold((Vec2::MAX, Vec2::MIN), |(lo, hi), &p| (lo.min(p), hi.max(p)));
        let origin = min - 2.;
        let size = ((max + 2. - origin) * res).ceil();
        let (width, height) = (size.x as usize, size.y as usize);
        let mut total = vec![0.; width * height];
        let mut amplitude = 1.;
        for octave in 0..tuning.octaves {
            let size = settings.scale / 2f32.powi(octave as i32) * SQRT_3;
            let white = (0..width * height).map(|_| standard_normal(rng)).collect();
            let noise = blur(white, width, height, size * res / 2.);
            let deviation = std_dev(&noise);
            total
                .iter_mut()
                .zip(&noise)
                .for_each(|(t, n)| *t += amplitude * n / deviation);
            amplitude *= settings.roughness;
        }
        let deviation = std_dev(&total);
        total.iter_mut().for_each(|t| *t /= deviation);
        let ridged = norm01(total.iter().map(|t| 1. - t.abs()).collect());
        let blended = norm01(
            norm01(total)
                .iter()
                .zip(&ridged)
                .map(|(t, r)| (1. - settings.ridges) * t + settings.ridges * r)
                .collect(),
        );
        Self {
            origin,
            res,
            width,
            height,
            data: blended
                .into_iter()
                .map(|h| h.powf(tuning.valleys) * settings.relief)
                .collect(),
        }
    }

    /// Bilinear height at `point`, clamped to the raster's edge.
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

/// Separable Gaussian blur that wraps at the edges, truncated at 4 sigma.
fn blur(data: Vec<f32>, width: usize, height: usize, sigma: f32) -> Vec<f32> {
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

/// Population standard deviation.
fn std_dev(data: &[f32]) -> f32 {
    let mean = data.iter().sum::<f32>() / data.len() as f32;
    (data.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / data.len() as f32).sqrt()
}

/// Rescales to span exactly 0..=1.
fn norm01(data: Vec<f32>) -> Vec<f32> {
    let (min, max) = data
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    data.into_iter().map(|x| (x - min) / (max - min)).collect()
}

/// A standard normal sample, by Box–Muller.
fn standard_normal(rng: &mut impl Rng) -> f32 {
    let u: f32 = 1. - rng.random::<f32>();
    let v: f32 = rng.random();
    (-2. * u.ln()).sqrt() * (TAU * v).cos()
}
