//! 台灣版：吉祥物形象。預設石虎（島虎），其餘形象可日後在設定中切換。

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mascot {
    #[default]
    Tiger,
    Shiba,
    Cat,
    Husky,
    Golden,
}

impl Mascot {
    pub const ALL: [Self; 5] = [Self::Tiger, Self::Shiba, Self::Cat, Self::Husky, Self::Golden];

    pub fn key(self) -> &'static str {
        match self { Self::Tiger=>"tiger", Self::Shiba=>"shiba", Self::Cat=>"cat", Self::Husky=>"husky", Self::Golden=>"golden" }
    }

    pub fn label(self) -> &'static str {
        match self { Self::Tiger=>"島虎（石虎）", Self::Shiba=>"柴犬", Self::Cat=>"貓", Self::Husky=>"哈士奇", Self::Golden=>"黃金獵犬" }
    }

    pub fn from_key(value: &str) -> Self {
        match value { "shiba"=>Self::Shiba, "cat"=>Self::Cat, "husky"=>Self::Husky, "golden"=>Self::Golden, _=>Self::Tiger }
    }

    /// 目前已有動畫序列的形象；其餘先沿用石虎，等美術補齊再切換。
    pub fn animated(self) -> Self {
        if self == Self::Shiba { self } else { Self::Tiger }
    }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(mascot: Mascot) {
    let index = Mascot::ALL.iter().position(|x| *x == mascot).unwrap_or(0);
    CURRENT.store(index as u8, Ordering::Relaxed);
}

pub fn current() -> Mascot {
    Mascot::ALL.get(CURRENT.load(Ordering::Relaxed) as usize).copied().unwrap_or_default()
}
