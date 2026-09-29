//! The hex board: five god regions around Ahamar's Table (docs/design.md §3).

use hexx::storage::{HexStore, HexagonalMap};
use hexx::{Hex, HexLayout};
use serde::{Deserialize, Serialize};

use crate::gods::God;
use crate::rng::Rng;

pub const BOARD_RADIUS: u32 = 7;
/// Settlements in each god's region.
pub const SETTLEMENTS_PER_REGION: usize = 2;

/// Rounds a corpse lies untouched before it sprouts into a grove (Bhava's
/// offering: docs/design.md §9, cards on bodies).
pub const GROVE_AGE: u8 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Terrain {
    Plains,
    Forest,
    Mountain,
    Swamp,
    Settlement,
    Temple,
    Ruins,
    Stones,
    /// A forest that grew from an untouched corpse.
    Grove,
    /// Ahamar's Table, the centre of the board.
    Table,
}

impl Terrain {
    /// Movement points to step onto this tile.
    pub const fn move_cost(self) -> u32 {
        match self {
            Terrain::Forest | Terrain::Grove | Terrain::Mountain => 2,
            _ => 1,
        }
    }

    /// Whether a corpse left here can sprout a grove.
    pub const fn can_grow_grove(self) -> bool {
        matches!(
            self,
            Terrain::Plains | Terrain::Forest | Terrain::Swamp | Terrain::Ruins | Terrain::Grove
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corpse {
    /// Rounds since it appeared.
    pub age: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    pub terrain: Terrain,
    /// Owning region; `None` only for the Table.
    pub region: Option<God>,
    pub corpse: Option<Corpse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Board {
    tiles: HexagonalMap<Tile>,
    radius: u32,
    starts: [Hex; 5],
    temples: [Hex; 5],
}

impl Board {
    pub fn generate(rng: &mut Rng) -> Self {
        let mut tiles = HexagonalMap::new(Hex::ZERO, BOARD_RADIUS, |hex| Tile {
            terrain: Terrain::Plains,
            region: region_of(hex),
            corpse: None,
        });

        // Regional flavour first, so the landmarks below overwrite it.
        let hexes: Vec<Hex> = tiles.iter().map(|(h, _)| h).collect();
        for &hex in &hexes {
            let tile = tiles.get_mut(hex).expect("hex from the map");
            tile.terrain = match tile.region {
                None => Terrain::Table,
                Some(god) => flavour_terrain(god, rng),
            };
        }

        let starts = God::ALL.map(|god| closest_to_bisector(god, BOARD_RADIUS));
        // Temples a little past halfway out, between the start and the Table.
        let temples = God::ALL.map(|god| closest_to_bisector(god, 4));
        for god in God::ALL {
            set(&mut tiles, starts[god.index()], Terrain::Plains);
            set(&mut tiles, temples[god.index()], Terrain::Temple);

            // Settlements per region, off the bisector.
            let mut spots: Vec<Hex> = hexes
                .iter()
                .copied()
                .filter(|&h| region_of(h) == Some(god))
                .filter(|&h| (2..=BOARD_RADIUS - 1).contains(&h.ulength()))
                .filter(|&h| h != temples[god.index()] && h != starts[god.index()])
                .collect();
            rng.shuffle(&mut spots);
            let mut placed: Vec<Hex> = Vec::new();
            for spot in spots {
                if placed.len() == SETTLEMENTS_PER_REGION {
                    break;
                }
                if placed.iter().all(|p| p.unsigned_distance_to(spot) >= 2) {
                    set(&mut tiles, spot, Terrain::Settlement);
                    placed.push(spot);
                }
            }

            // One ring of standing stones per region, a rare sight apart from
            // the settlements, the temple and the start.
            let stones = hexes
                .iter()
                .copied()
                .filter(|&h| region_of(h) == Some(god))
                .filter(|&h| (3..=BOARD_RADIUS - 1).contains(&h.ulength()))
                .filter(|&h| h != temples[god.index()])
                .filter(|&h| placed.iter().all(|p| p.unsigned_distance_to(h) >= 2))
                .collect::<Vec<_>>();
            if let Some(&spot) = rng.pick(&stones) {
                set(&mut tiles, spot, Terrain::Stones);
            }
        }

        Board {
            tiles,
            radius: BOARD_RADIUS,
            starts,
            temples,
        }
    }

    /// A small plain board for a scripted scene (a tutorial chapter): all
    /// plains around the Table, regions as usual, starts on the rim and
    /// temple spots halfway (plain until the scene sets them).
    pub fn plain(radius: u32) -> Self {
        let radius = radius.max(2);
        let tiles = HexagonalMap::new(Hex::ZERO, radius, |hex| Tile {
            terrain: if hex == Hex::ZERO {
                Terrain::Table
            } else {
                Terrain::Plains
            },
            region: region_of(hex),
            corpse: None,
        });
        Board {
            tiles,
            radius,
            starts: God::ALL.map(|god| closest_to_bisector(god, radius)),
            temples: God::ALL.map(|god| closest_to_bisector(god, radius.div_ceil(2))),
        }
    }

    pub fn radius(&self) -> u32 {
        self.radius
    }

    pub fn contains(&self, hex: Hex) -> bool {
        hex.ulength() <= self.radius
    }

    pub(crate) fn set_start(&mut self, god: God, hex: Hex) {
        self.starts[god.index()] = hex;
    }

    pub fn tile(&self, hex: Hex) -> Option<&Tile> {
        self.tiles.get(hex)
    }

    pub(crate) fn tile_mut(&mut self, hex: Hex) -> Option<&mut Tile> {
        self.tiles.get_mut(hex)
    }

    pub fn tiles(&self) -> impl Iterator<Item = (Hex, &Tile)> {
        self.tiles.iter()
    }

    pub fn start_of(&self, god: God) -> Hex {
        self.starts[god.index()]
    }

    pub fn temple_of(&self, god: God) -> Hex {
        self.temples[god.index()]
    }

    pub fn corpses(&self) -> impl Iterator<Item = (Hex, Corpse)> + '_ {
        self.tiles().filter_map(|(h, t)| t.corpse.map(|c| (h, c)))
    }
}

fn set(tiles: &mut HexagonalMap<Tile>, hex: Hex, terrain: Terrain) {
    if let Some(tile) = tiles.get_mut(hex) {
        tile.terrain = terrain;
    }
}

/// Angle of a hex around the centre in degrees, 0..360, in the same flat
/// layout the client renders with.
fn angle_of(hex: Hex) -> f32 {
    let p = HexLayout::flat().hex_to_world_pos(hex);
    p.y.atan2(p.x).to_degrees().rem_euclid(360.0)
}

/// Each god owns a 72° wedge; god `k` is centred on `72k + 36`.
fn region_of(hex: Hex) -> Option<God> {
    if hex == Hex::ZERO {
        return None;
    }
    let sector = (angle_of(hex) / 72.0).floor() as usize;
    Some(God::from_index(sector))
}

fn closest_to_bisector(god: God, ring: u32) -> Hex {
    let bisector = 72.0 * god.index() as f32 + 36.0;
    Hex::ZERO
        .ring(ring)
        .filter(|&h| region_of(h) == Some(god))
        .min_by(|a, b| {
            let da = angle_gap(angle_of(*a), bisector);
            let db = angle_gap(angle_of(*b), bisector);
            da.total_cmp(&db)
        })
        .expect("every region reaches every ring")
}

fn angle_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Placeholder terrain mix per region until the level design pass.
fn flavour_terrain(god: God, rng: &mut Rng) -> Terrain {
    use Terrain::*;
    let table: &[Terrain] = match god {
        God::Bhava => &[Forest, Forest, Forest, Plains, Plains, Swamp],
        God::Trishna => &[Plains, Plains, Plains, Plains, Forest, Plains],
        God::Zaga => &[Mountain, Mountain, Plains, Plains, Ruins, Plains],
        God::Ahamar => &[Plains, Plains, Plains, Mountain, Plains, Ruins],
        God::Maya => &[Swamp, Swamp, Plains, Plains, Ruins, Forest],
    };
    *rng.pick(table).expect("non-empty table")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_region_gets_a_fair_share() {
        let board = Board::generate(&mut Rng::new(1));
        for god in God::ALL {
            let n = board.tiles().filter(|(_, t)| t.region == Some(god)).count();
            // 3r(r+1) non-centre tiles over 5 wedges; a hex grid is 6-fold,
            // so the split is only roughly even.
            let r = BOARD_RADIUS as f32;
            let share = 3.0 * r * (r + 1.0) / 5.0;
            let gap = (n as f32 - share).abs() / share;
            assert!(gap < 0.15, "{god:?}: {n} of about {share}");
        }
    }

    #[test]
    fn landmarks_sit_in_their_regions() {
        let board = Board::generate(&mut Rng::new(2));
        for god in God::ALL {
            let start = board.start_of(god);
            let temple = board.temple_of(god);
            assert_eq!(start.ulength(), BOARD_RADIUS);
            assert_eq!(board.tile(start).unwrap().region, Some(god));
            assert_eq!(board.tile(temple).unwrap().terrain, Terrain::Temple);
            assert_eq!(board.tile(temple).unwrap().region, Some(god));
        }
        assert_eq!(board.tile(Hex::ZERO).unwrap().terrain, Terrain::Table);
    }
}
