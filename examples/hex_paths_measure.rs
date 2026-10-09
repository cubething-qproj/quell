//! Samples the prototype's actual generator without starting Bevy.
//!
//! From the org root: `cargo run -p quell --example hex_paths_measure -- 1000 0`.
//! Arguments are the sample count per configuration, the starting seed, and an
//! optional comma-separated list of case names to sample.
//! CSV output includes failed attempts, so they cannot improve the statistics.

use data::{BoardStats, HexPathsSettings, ShapeWeights, SpawnPlacement};
use generate::BoardLayout;
use rand::{SeedableRng, rngs::StdRng};

// Keep prototype types local; these are the same modules used by the screen.
#[allow(dead_code)]
#[path = "../src/screen/dev/hex_paths/data.rs"]
mod data;
#[allow(dead_code)]
#[path = "../src/screen/dev/hex_paths/generate.rs"]
mod generate;

mod prelude {
    pub use bevy::prelude::*;
}

/// A named settings variant, with defaults preserved for every omitted field.
struct Case {
    name: String,
    settings: HexPathsSettings,
}

impl Case {
    fn new(name: impl Into<String>, settings: HexPathsSettings) -> Self {
        Self {
            name: name.into(),
            settings,
        }
    }

    /// One-factor comparisons, then a small junction/length/noise sweep.
    fn variants() -> Vec<Self> {
        let mut cases = vec![Self::new("default", HexPathsSettings::default())];
        for noise in [0., 0.5, 5., 10.] {
            cases.push(Self::new(
                format!("noise_{noise}"),
                HexPathsSettings {
                    noise,
                    ..Default::default()
                },
            ));
        }
        for junction_chance in [0., 0.1, 0.5, 0.75, 1.] {
            cases.push(Self::new(
                format!("junction_{junction_chance}"),
                HexPathsSettings {
                    junction_chance,
                    ..Default::default()
                },
            ));
        }
        for length in [0, 1, 2, 3, 4, 6] {
            cases.push(Self::new(
                format!("length_{length}"),
                HexPathsSettings {
                    min_branch_length: length,
                    max_branch_length: length,
                    ..Default::default()
                },
            ));
        }
        for branch_join_chance in [0., 1.] {
            cases.push(Self::new(
                format!("join_{branch_join_chance}"),
                HexPathsSettings {
                    branch_join_chance,
                    ..Default::default()
                },
            ));
        }
        for (name, shape_weights) in [
            (
                "straight_2",
                ShapeWeights {
                    sharp: 1.,
                    wide: 1.,
                    straight: 2.,
                },
            ),
            (
                "straight_4",
                ShapeWeights {
                    sharp: 1.,
                    wide: 1.,
                    straight: 4.,
                },
            ),
            (
                "wide_2",
                ShapeWeights {
                    sharp: 1.,
                    wide: 2.,
                    straight: 1.,
                },
            ),
            (
                "sharp_2",
                ShapeWeights {
                    sharp: 2.,
                    wide: 1.,
                    straight: 1.,
                },
            ),
        ] {
            cases.push(Self::new(
                name,
                HexPathsSettings {
                    shape_weights,
                    ..Default::default()
                },
            ));
        }
        cases.push(Self::new(
            "random_spawn",
            HexPathsSettings {
                spawn: SpawnPlacement::Random,
                ..Default::default()
            },
        ));
        for spokes in [2, 3, 4] {
            cases.push(Self::new(
                format!("spawn_{spokes}"),
                HexPathsSettings {
                    min_spawn_spokes: spokes,
                    max_spawn_spokes: spokes,
                    ..Default::default()
                },
            ));
        }
        for noise in [0., 2.] {
            for junction_chance in [0.1, 0.25, 0.5, 0.75, 1.] {
                for length in [1, 2, 3, 4] {
                    cases.push(Self::new(
                        format!("j{junction_chance}_len{length}_n{noise}"),
                        HexPathsSettings {
                            noise,
                            junction_chance,
                            min_branch_length: length,
                            max_branch_length: length,
                            ..Default::default()
                        },
                    ));
                }
            }
        }
        cases
    }

    /// Writes every sample, including errors, using the supplied seed interval.
    fn sample(&self, count: u64, start: u64) {
        for seed in start
            ..start
                .checked_add(count)
                .expect("seed interval must fit u64")
        {
            match BoardLayout::generate(&self.settings, &mut StdRng::seed_from_u64(seed)) {
                Ok(layout) => {
                    let stats = BoardStats::from_layout(seed, &layout);
                    println!(
                        "{},{seed},ok,{},{},{},{},{},{},{}",
                        self.name,
                        stats.two_spoke + stats.three_spoke,
                        stats.two_spoke,
                        stats.three_spoke,
                        stats.shapes[0],
                        stats.shapes[1],
                        stats.shapes[2],
                        stats.attempts
                    );
                }
                Err(error) => println!("{},{seed},{error:?},,,,,,,", self.name),
            }
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let count = args
        .next()
        .map(|arg| arg.parse::<u64>().expect("sample count must be u64"))
        .unwrap_or(1000);
    let start = args
        .next()
        .map(|arg| arg.parse::<u64>().expect("starting seed must be u64"))
        .unwrap_or(0);
    assert!(count > 0, "sample count must be positive");
    let selected = args.next();
    println!("case,seed,status,paths,two_spoke,three_spoke,sharp,wide,straight,attempts");
    for case in Case::variants().into_iter().filter(|case| {
        selected
            .as_ref()
            .is_none_or(|names| names.split(',').any(|name| name == case.name))
    }) {
        case.sample(count, start);
    }
}
