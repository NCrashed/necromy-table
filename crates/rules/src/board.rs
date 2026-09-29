//! The hex board: five god regions around Ahamar's Table (docs/design.md §3).
//!
//! The board is a set of hexes, not a fixed disc: a world being created
//! (§21.1) starts small and grows at its rim, and land can sink into the
//! mist and come back out of it. A hex's region is the god's wedge its
//! angle falls in, reckoned in integers so every machine agrees.

use std::collections::BTreeMap;

use hexx::Hex;
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
    /// Land gone into the mist (§21.1): nobody walks it, nothing lies there,
    /// until it comes back out.
    Mist,
    /// A river (§21.8): crossing it ends the walk, going along it is easy.
    River,
    /// Still water: nobody walks it, nothing lies there; not the mist.
    Lake,
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

    /// Ground to stand on: anything but the mist and still water.
    pub const fn is_land(self) -> bool {
        !matches!(self, Terrain::Mist | Terrain::Lake)
    }

    /// Water: a river or a lake.
    pub const fn is_water(self) -> bool {
        matches!(self, Terrain::River | Terrain::Lake)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corpse {
    /// Rounds since it appeared.
    pub age: u8,
    /// A champion fell here: what grows of it is theirs to tell (a World
    /// Tree, §21.7).
    pub hero: bool,
}

impl Corpse {
    /// A body just laid, not a champion's.
    pub const fn fresh() -> Corpse {
        Corpse {
            age: 0,
            hero: false,
        }
    }
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
    /// Keyed by axial coordinates: `Hex` has no order, and iteration must be
    /// the same everywhere (the rules draw from it).
    tiles: BTreeMap<(i32, i32), Tile>,
    /// The farthest ring any hex of the board lies on.
    extent: u32,
    starts: [Hex; 5],
    temples: [Hex; 5],
    /// What lies under each hex gone into the mist, to come back as.
    veiled: BTreeMap<(i32, i32), Terrain>,
}

fn key(hex: Hex) -> (i32, i32) {
    (hex.x(), hex.y())
}

impl Board {
    /// A full world (§21): the whole board at once, radius `BOARD_RADIUS`.
    pub fn generate(rng: &mut Rng) -> Self {
        let mut board = Board::disc(BOARD_RADIUS, |_| Terrain::Plains);
        // Regional flavour first, so the landmarks below overwrite it.
        let hexes: Vec<Hex> = board.tiles().map(|(h, _)| h).collect();
        for &hex in &hexes {
            let tile = board.tile_mut(hex).expect("hex from the board");
            tile.terrain = match tile.region {
                None => Terrain::Table,
                Some(god) => flavour_terrain(god, rng),
            };
        }

        let starts = God::ALL.map(|god| closest_to_bisector(god, BOARD_RADIUS));
        // Temples a little past halfway out, between the start and the Table.
        let temples = God::ALL.map(|god| closest_to_bisector(god, 4));
        board.starts = starts;
        board.temples = temples;
        for god in God::ALL {
            board.set(starts[god.index()], Terrain::Plains);
            board.set(temples[god.index()], Terrain::Temple);

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
                    board.set(spot, Terrain::Settlement);
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
                board.set(spot, Terrain::Stones);
            }
        }
        board
    }

    /// A small world to create (§21.1): radius `radius`, the Table in the
    /// middle, the temples halfway out, the starts on the rim, and the rest
    /// of each region drawn from `kinds` (plains first, then the others).
    pub fn seed_world(rng: &mut Rng, radius: u32, kinds: &[Terrain]) -> Self {
        let radius = radius.max(2);
        let mut board = Board::plain(radius);
        let hexes: Vec<Hex> = board.tiles().map(|(h, _)| h).collect();
        // Standing stones are rare: two rings on the whole board, set below.
        let common: Vec<Terrain> = kinds
            .iter()
            .copied()
            .filter(|&t| t != Terrain::Stones)
            .collect();
        for &hex in &hexes {
            if hex == Hex::ZERO {
                continue;
            }
            // Mostly plains; each other kind of land here and there.
            let terrain = match rng.below(3) {
                0 => *rng.pick(&common).unwrap_or(&Terrain::Plains),
                _ => Terrain::Plains,
            };
            board.set(hex, terrain);
        }
        if kinds.contains(&Terrain::Stones) {
            let mut spots: Vec<Hex> = hexes.iter().copied().filter(|h| h.ulength() >= 2).collect();
            rng.shuffle(&mut spots);
            for hex in spots.into_iter().take(2) {
                board.set(hex, Terrain::Stones);
            }
        }
        for god in God::ALL {
            board.set(board.start_of(god), Terrain::Plains);
            board.set(board.temple_of(god), Terrain::Temple);
        }
        board
    }

    /// `per_region` settlements in each god's land, off the Table, the
    /// temples and the starts, two hexes apart at least.
    pub(crate) fn settle_regions(&mut self, rng: &mut Rng, per_region: usize) {
        let hexes: Vec<Hex> = self.land().map(|(h, _)| h).collect();
        for god in God::ALL {
            let mut spots: Vec<Hex> = hexes
                .iter()
                .copied()
                .filter(|&h| region_of(h) == Some(god) && h.ulength() < self.extent)
                .filter(|&h| h != self.temple_of(god) && h != self.start_of(god))
                .collect();
            rng.shuffle(&mut spots);
            let mut placed: Vec<Hex> = Vec::new();
            for spot in spots {
                if placed.len() == per_region {
                    break;
                }
                let clear = self
                    .land()
                    .filter(|(_, t)| t.terrain == Terrain::Settlement)
                    .all(|(h, _)| h.unsigned_distance_to(spot) >= 2);
                if clear {
                    self.set(spot, Terrain::Settlement);
                    placed.push(spot);
                }
            }
        }
    }

    /// A small plain board for a scripted scene (a tutorial chapter): all
    /// plains around the Table, regions as usual, starts on the rim and
    /// temple spots halfway (plain until the scene sets them).
    pub fn plain(radius: u32) -> Self {
        let radius = radius.max(2);
        let mut board = Board::disc(radius, |hex| {
            if hex == Hex::ZERO {
                Terrain::Table
            } else {
                Terrain::Plains
            }
        });
        board.starts = God::ALL.map(|god| closest_to_bisector(god, radius));
        board.temples = God::ALL.map(|god| closest_to_bisector(god, radius.div_ceil(2)));
        board
    }

    fn disc(radius: u32, terrain: impl Fn(Hex) -> Terrain) -> Self {
        let tiles = Hex::ZERO
            .range(radius)
            .map(|hex| {
                let tile = Tile {
                    terrain: terrain(hex),
                    region: region_of(hex),
                    corpse: None,
                };
                (key(hex), tile)
            })
            .collect();
        Board {
            tiles,
            extent: radius,
            starts: [Hex::ZERO; 5],
            temples: [Hex::ZERO; 5],
            veiled: BTreeMap::new(),
        }
    }

    /// The farthest ring any hex lies on: rings past it hold nothing.
    pub fn extent(&self) -> u32 {
        self.extent
    }

    /// Land one can stand on: a hex of the board, not in the mist.
    pub fn contains(&self, hex: Hex) -> bool {
        self.tile(hex).is_some_and(|t| t.terrain.is_land())
    }

    pub(crate) fn set_start(&mut self, god: God, hex: Hex) {
        self.starts[god.index()] = hex;
    }

    /// Every hex of the board, the mist too.
    pub fn tile(&self, hex: Hex) -> Option<&Tile> {
        self.tiles.get(&key(hex))
    }

    pub(crate) fn tile_mut(&mut self, hex: Hex) -> Option<&mut Tile> {
        self.tiles.get_mut(&key(hex))
    }

    /// Every hex of the board, the mist too, in axial order.
    pub fn tiles(&self) -> impl Iterator<Item = (Hex, &Tile)> {
        self.tiles.iter().map(|(&(x, y), t)| (Hex::new(x, y), t))
    }

    /// The land: hexes out of the mist.
    pub fn land(&self) -> impl Iterator<Item = (Hex, &Tile)> {
        self.tiles().filter(|(_, t)| t.terrain.is_land())
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

    fn set(&mut self, hex: Hex, terrain: Terrain) {
        if let Some(tile) = self.tile_mut(hex) {
            tile.terrain = terrain;
        }
    }

    // ---- Growth and mist (§21.1) ----

    /// Hexes where new land may rise: off the board, next to it, in `god`'s
    /// region, nearest to `near` first.
    pub fn frontier(&self, god: God, near: Hex) -> Vec<Hex> {
        let mut out: Vec<Hex> = Vec::new();
        for (hex, _) in self.tiles() {
            for n in hex.all_neighbors() {
                if self.tile(n).is_none() && region_of(n) == Some(god) && !out.contains(&n) {
                    out.push(n);
                }
            }
        }
        out.sort_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()));
        out
    }

    /// New land on `hex`, which must be off the board and next to it.
    pub(crate) fn raise(&mut self, hex: Hex, terrain: Terrain) -> bool {
        let touches = hex.all_neighbors().iter().any(|&n| self.tile(n).is_some());
        if self.tile(hex).is_some() || !touches {
            return false;
        }
        self.tiles.insert(
            key(hex),
            Tile {
                terrain,
                region: region_of(hex),
                corpse: None,
            },
        );
        self.extent = self.extent.max(hex.ulength());
        true
    }

    /// Whether `hex` can go into the mist: land, but not a temple, the
    /// Table or anyone's start.
    pub fn can_veil(&self, hex: Hex) -> bool {
        self.tile(hex).is_some_and(|t| {
            t.terrain.is_land() && !matches!(t.terrain, Terrain::Temple | Terrain::Table)
        }) && !self.starts.contains(&hex)
    }

    /// The land on `hex` goes into the mist; what lay there is remembered.
    pub(crate) fn veil(&mut self, hex: Hex) -> bool {
        if !self.can_veil(hex) {
            return false;
        }
        let tile = self.tiles.get_mut(&key(hex)).expect("checked above");
        self.veiled.insert(key(hex), tile.terrain);
        tile.terrain = Terrain::Mist;
        tile.corpse = None;
        true
    }

    /// The mist on `hex` lifts: the land comes back as it was.
    pub(crate) fn unveil(&mut self, hex: Hex) -> Option<Terrain> {
        let tile = self.tiles.get_mut(&key(hex))?;
        if tile.terrain != Terrain::Mist {
            return None;
        }
        let terrain = self.veiled.remove(&key(hex)).unwrap_or(Terrain::Plains);
        tile.terrain = terrain;
        Some(terrain)
    }
}

/// Which god's wedge `hex` lies in: god `k` owns 72k°..72(k+1)° around the
/// Table, in the flat layout the client renders with. `None` for the Table.
///
/// In integers, so every machine agrees: the hex sits at
/// (3q, √3(q + 2r)) / 2, and each wedge border is a ray at a multiple of
/// 72°. Which side of a ray a hex lies on is the sign of a cross product;
/// the irrational constants are fixed point, 2^52, far finer than any
/// board needs.
pub fn region_of(hex: Hex) -> Option<God> {
    if hex == Hex::ZERO {
        return None;
    }
    let (q, r) = (i128::from(hex.x()), i128::from(hex.y()));
    // Twice the position, without the √3 on y: (3q, q + 2r).
    let (x, y) = (3 * q, q + 2 * r);
    // Past the ray at `deg` degrees, turning counterclockwise:
    // cos·√3·y − sin·x > 0, with √3·cos and sin in fixed point.
    let past = |ray: usize| {
        let [s3cos, sin] = RAYS[ray];
        s3cos * y - sin * x > 0
    };
    let upper = y > 0 || (y == 0 && x > 0);
    let sector = if upper {
        if past(1) {
            2
        } else if past(0) {
            1
        } else {
            0
        }
    } else if past(3) {
        4
    } else if past(2) {
        3
    } else {
        2
    };
    Some(God::from_index(sector))
}

/// √3·cos and sin of 72°, 144°, 216°, 288°, times 2^52.
const RAYS: [[i128; 2]; 4] = [
    [2_410_475_745_809_474, 4_283_177_772_395_136],
    [-6_310_707_431_586_455, 2_647_149_443_198_255],
    [-6_310_707_431_586_455, -2_647_149_443_198_255],
    [2_410_475_745_809_474, -4_283_177_772_395_136],
];

/// The hex of `ring` closest to the middle of `god`'s wedge: the smallest
/// angle to its bisector, i.e. the largest cos² with the sign kept; ties go
/// to the lowest axial coordinates.
fn closest_to_bisector(god: God, ring: u32) -> Hex {
    let [s3cos, sin] = BISECTORS[god.index()];
    Hex::ZERO
        .ring(ring)
        .filter(|&h| region_of(h) == Some(god))
        .max_by_key(|h| {
            let (q, r) = (i128::from(h.x()), i128::from(h.y()));
            let (x, y) = (3 * q, q + 2 * r);
            // Twice the position is (x, √3·y): its dot product with the
            // bisector is (x·√3cos + 3y·sin) / √3, its length² x² + 3y².
            let dot = x * s3cos + 3 * y * sin;
            let len2 = x * x + 3 * y * y;
            let cos2 = dot.signum() * (dot * dot / len2);
            (cos2, -i128::from(h.x()), -i128::from(h.y()))
        })
        .expect("every region reaches every ring")
}

/// √3·cos and sin of 36°, 108°, 180°, 252°, 324°, times 2^52.
const BISECTORS: [[i128; 2]; 5] = [
    [6_310_707_431_586_455, 2_647_149_443_198_255],
    [-2_410_475_745_809_474, 4_283_177_772_395_136],
    [-7_800_463_371_553_962, 0],
    [-2_410_475_745_809_474, -4_283_177_772_395_136],
    [6_310_707_431_586_455, -2_647_149_443_198_255],
];

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

    /// The old reckoning, in floating point, as the client lays hexes out.
    fn region_by_angle(hex: Hex) -> Option<God> {
        if hex == Hex::ZERO {
            return None;
        }
        let p = hexx::HexLayout::flat().hex_to_world_pos(hex);
        let angle = f64::from(p.y)
            .atan2(f64::from(p.x))
            .to_degrees()
            .rem_euclid(360.0);
        Some(God::from_index((angle / 72.0).floor() as usize))
    }

    #[test]
    fn regions_in_integers_match_the_angles() {
        for hex in Hex::ZERO.range(40) {
            assert_eq!(region_of(hex), region_by_angle(hex), "{hex:?}");
        }
    }

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

    #[test]
    fn land_grows_at_the_rim_of_its_region() {
        let mut board = Board::plain(3);
        let near = board.start_of(God::Maya);
        let rim = board.frontier(God::Maya, near);
        assert!(!rim.is_empty());
        for &hex in &rim {
            assert!(board.tile(hex).is_none());
            assert_eq!(region_of(hex), Some(God::Maya));
        }
        assert!(board.raise(rim[0], Terrain::Swamp));
        assert_eq!(board.tile(rim[0]).unwrap().region, Some(God::Maya));
        assert_eq!(board.extent(), 4);
        // Not twice, and not off in the void.
        assert!(!board.raise(rim[0], Terrain::Swamp));
        assert!(!board.raise(Hex::new(20, 0), Terrain::Plains));
    }

    #[test]
    fn mist_takes_the_land_and_gives_it_back() {
        let mut board = Board::plain(3);
        let hex = Hex::new(1, 1);
        board.set(hex, Terrain::Forest);
        assert!(board.veil(hex));
        assert!(!board.contains(hex));
        assert!(board.tile(hex).is_some());
        assert_eq!(board.unveil(hex), Some(Terrain::Forest));
        assert!(board.contains(hex));
        // Temples, the Table and the starts stay out of the mist.
        assert!(!board.veil(Hex::ZERO));
        assert!(!board.veil(board.start_of(God::Bhava)));
    }
}
