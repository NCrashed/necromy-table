//! The five gods and the wu-xing ring of elements (docs/design.md §4).
//!
//! The index order is canon from the lore codex and must not change: every
//! relation is derived from it by arithmetic, never from a lookup table.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum God {
    Bhava,
    Trishna,
    Zaga,
    Ahamar,
    Maya,
}

impl God {
    pub const ALL: [God; 5] = [God::Bhava, God::Trishna, God::Zaga, God::Ahamar, God::Maya];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn from_index(i: usize) -> God {
        God::ALL[i % 5]
    }

    pub const fn element(self) -> Element {
        Element::ALL[self.index()]
    }

    pub const fn name(self) -> &'static str {
        match self {
            God::Bhava => "Bhava",
            God::Trishna => "Trishna",
            God::Zaga => "Zaga",
            God::Ahamar => "Ahamar",
            God::Maya => "Maya",
        }
    }

    /// Page accent colour from the lore codex, as sRGB bytes.
    pub const fn accent(self) -> [u8; 3] {
        match self {
            God::Bhava => [0x73, 0xcc, 0x66],
            God::Trishna => [0xd9, 0x51, 0x2c],
            God::Zaga => [0x7c, 0x5c, 0x9e],
            God::Ahamar => [0xc9, 0xa2, 0x4b],
            God::Maya => [0x2b, 0xb6, 0xa8],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Element {
    Wood,
    Fire,
    Earth,
    Metal,
    Water,
}

impl Element {
    pub const ALL: [Element; 5] = [
        Element::Wood,
        Element::Fire,
        Element::Earth,
        Element::Metal,
        Element::Water,
    ];

    pub const fn index(self) -> usize {
        self as usize
    }

    const fn step(self, n: usize) -> Element {
        Element::ALL[(self.index() + n) % 5]
    }

    /// One step along the rim: wood feeds fire, fire makes earth, …
    pub const fn generates(self) -> Element {
        self.step(1)
    }

    /// Along the star: water quenches fire, fire melts metal, …
    pub const fn quenches(self) -> Element {
        self.step(2)
    }

    /// The only element that breaks a ward of this element.
    pub const fn quenched_by(self) -> Element {
        self.step(3)
    }

    /// The element that feeds this one: a heal of it feeds its poison.
    pub const fn generated_by(self) -> Element {
        self.step(4)
    }

    pub const fn is_yang(self) -> bool {
        matches!(self, Element::Wood | Element::Fire | Element::Earth)
    }

    pub const fn is_yin(self) -> bool {
        matches!(self, Element::Metal | Element::Water | Element::Earth)
    }

    /// Earth sits on both sides; a pair keeps the rhythm only when one side
    /// is purely yang and the other purely yin.
    /// Both purely yang or both purely yin: a chain between them costs a surge
    /// of qi. Of the generation pairs only wood→fire and metal→water do.
    pub const fn breaks_rhythm_with(self, other: Element) -> bool {
        (self.is_pure_yang() && other.is_pure_yang()) || (self.is_pure_yin() && other.is_pure_yin())
    }

    pub const fn keeps_rhythm_with(self, other: Element) -> bool {
        (self.is_pure_yang() && other.is_pure_yin()) || (self.is_pure_yin() && other.is_pure_yang())
    }

    const fn is_pure_yang(self) -> bool {
        self.is_yang() && !self.is_yin()
    }

    const fn is_pure_yin(self) -> bool {
        self.is_yin() && !self.is_yang()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_follows_the_codex() {
        use Element::*;
        assert_eq!(Wood.generates(), Fire);
        assert_eq!(Fire.generates(), Earth);
        assert_eq!(Earth.generates(), Metal);
        assert_eq!(Metal.generates(), Water);
        assert_eq!(Water.generates(), Wood);
    }

    #[test]
    fn quenching_follows_the_codex() {
        use Element::*;
        assert_eq!(Water.quenches(), Fire);
        assert_eq!(Fire.quenches(), Metal);
        assert_eq!(Metal.quenches(), Wood);
        assert_eq!(Wood.quenches(), Earth);
        assert_eq!(Earth.quenches(), Water);
    }

    #[test]
    fn every_element_has_exactly_one_counter() {
        for e in Element::ALL {
            let counters: Vec<_> = Element::ALL.iter().filter(|c| c.quenches() == e).collect();
            assert_eq!(counters, [&e.quenched_by()]);
        }
    }

    #[test]
    fn two_generation_pairs_break_rhythm() {
        // Codex: wood→fire (yang-yang) and metal→water (yin-yin) break it.
        // Earth pairs are ambiguous by design and never count as keeping it.
        let broken: Vec<_> = Element::ALL
            .into_iter()
            .filter(|e| !e.keeps_rhythm_with(e.generates()))
            .filter(|e| *e != Element::Earth && e.generates() != Element::Earth)
            .collect();
        assert_eq!(broken, [Element::Wood, Element::Metal]);
    }

    #[test]
    fn gods_map_to_their_elements() {
        assert_eq!(God::Bhava.element(), Element::Wood);
        assert_eq!(God::Trishna.element(), Element::Fire);
        assert_eq!(God::Zaga.element(), Element::Earth);
        assert_eq!(God::Ahamar.element(), Element::Metal);
        assert_eq!(God::Maya.element(), Element::Water);
    }
}
