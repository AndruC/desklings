// Pixel art. One char per pixel:
//   .  transparent     k  outline       w  white
//   b  body            d  belly         a  accent      (b/d/a come from the species palette)
//   h  eye shine / e  eye top  (both become body colour when asleep -> closed eyes)
//   p  cheek           r  red           o  orange-red  n  brown
//   y  yellow          g  green         c  cyan        s  silver
// Monsters face right; the renderer mirrors them when walking left.

pub type Sprite = &'static [&'static str];

// ---------------------------------------------------------------- egg & babies

pub const EGG: Sprite = &[
    "................",
    "................",
    "......kkkk......",
    ".....kwwwwk.....",
    "....kwwwwwwk....",
    "....kwwbwwwk....",
    "...kwwbbbwwwk...",
    "...kwwwbwwwwk...",
    "...kwwwwwwbwk...",
    "...kwbwwwbbbk...",
    "...kwbbwwwbwk...",
    "...kwwwwwwwwk...",
    "....kwwwwwwk....",
    ".....kkkkkk.....",
    "................",
    "................",
];

pub const BLIP_A: Sprite = &[
    "................",
    "................",
    "................",
    "................",
    "................",
    "......kkkk......",
    "....kkbbbbkk....",
    "...kbbbbbbbbk...",
    "..kbbhebbhebbk..",
    "..kbbkkbbkkbbk..",
    "..kbpbbbbbbpbk..",
    "..kbbbbkkbbbbk..",
    "..kbbbbbbbbbbk..",
    "...kbbbbbbbbk...",
    "....kkkkkkkk....",
    "................",
];

pub const BLIP_B: Sprite = &[
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    ".....kkkkkk.....",
    "...kkbbbbbbkk...",
    "..kbbhebbhebbk..",
    "..kbbkkbbkkbbk..",
    ".kbbpbbbbbbpbbk.",
    ".kbbbbbkkbbbbbk.",
    ".kbbbbbbbbbbbbk.",
    "..kbbbbbbbbbbk..",
    "...kkkkkkkkkk...",
    "................",
];

pub const BLOP: Sprite = &[
    "................",
    "...kk......kk...",
    "..kbbk....kbbk..",
    "..kbbbkkkkbbbk..",
    "..kbbbbbbbbbbk..",
    ".kbbhebbbbhebbk.",
    ".kbbkkbbbbkkbbk.",
    ".kbpbbbbbbbbpbk.",
    ".kbbbbbkkbbbbbk.",
    "..kbbbbbbbbbbk..",
    "..kbddbbbbddbk..",
    "...kbbbbbbbbk...",
    "...kbbkkkkbbk...",
    "...kkk....kkk...",
    "................",
    "................",
];

// ---------------------------------------------------------------- rookies (16x16)

pub const RAPTIN: Sprite = &[
    "................",
    "........kkkkk...",
    ".......kbbbbbk..",
    "......kbbbbhebk.",
    "......kbbbbkkbbk",
    "......kbbbbbbbbk",
    "......kbbbbkkkkk",
    "......kbbbbwwwk.",
    "..k..kbbbbkkkk..",
    ".kbk.kbbbbbbk...",
    ".kbbkbbddbbbbk..",
    "..kbbbbdddbkbk..",
    "...kbbbddbbkk...",
    "....kbbbbbbk....",
    "....kbk..kbk....",
    "....kkk..kkk....",
];

pub const FLUFFIN: Sprite = &[
    "........kk..kk..",
    ".......kak.kak..",
    ".......kak.kak..",
    ".......kakkkak..",
    "......kbbbbbbbk.",
    ".....kbbbbbhebk.",
    ".....kbbbbbkkbbk",
    ".....kbpbbbbbbkk",
    "..kk..kbbbbbkkk.",
    ".kwwk.kbddddbk..",
    "kwwwwkkbddddbk..",
    "kwwwwbbbddddbk..",
    ".kwwkbbbbddbbk..",
    "..kkkbbbbbbbbk..",
    "....kbbk..kbbk..",
    "....kkkk..kkkk..",
];

pub const SHELLBY: Sprite = &[
    "................",
    "................",
    "................",
    ".....kkkk.......",
    "....kaaaak......",
    "...kaakaaak.kkk.",
    "..kaaakaaakkbbbk",
    ".kaakaaakaakbhek",
    ".kkkkkkkkkkkbkkk",
    ".kddddddddddkkk.",
    "..kbbk....kbbk..",
    "..kkkk....kkkk..",
];

// ---------------------------------------------------------------- champions (20 wide)

pub const PYROREX: Sprite = &[
    "...........k...k....",
    "..........kak.kak...",
    "..........kkkkkkkk..",
    ".........kbbbbbbbbk.",
    ".........kbbbbbhebbk",
    ".........kbbbbbkkbbk",
    ".........kbbbbbbbbbk",
    ".........kbbbbbkkkkk",
    ".........kbbbbkwkwk.",
    "........kbbbbbbkkkk.",
    "........kbbbbbbk....",
    ".......kbbddbbbbk...",
    "......kbbdddbbkbbk..",
    ".oy...kbbdddbbbkk...",
    "kyyk..kbbddddbbk....",
    "kyyakkbbbddddbbk....",
    ".kaabbbbbbddbbbk....",
    "..kkkkkbbbbbbbk.....",
    ".......kbbk.kbbk....",
    ".......kkkk.kkkk....",
];

pub const CRAGDON: Sprite = &[
    "..........kk.kk.....",
    ".........kakkakk....",
    "........kakbbbbbk...",
    ".......kakbbbbbbbk..",
    "........kbbbbbbhebk.",
    ".......kakbbbbbkkbbk",
    "........kbbbbbbbbbbk",
    ".......kakbbbbbkkkkk",
    "........kbbbbbkwkwk.",
    ".......kakbbbbbkkkk.",
    "......kakbbbbbbk....",
    ".....kakbbddbbbbk...",
    "....kakbbdddbbkbbk..",
    "...kakbbbdddbbbkk...",
    "..kakbbbbddddbbk....",
    ".kakbbbbbddddbbk....",
    "kbbbbbbbbbddbbbk....",
    ".kkkkkkbbbbbbbk.....",
    "......kbbbk.kbbbk...",
    "......kkkkk.kkkkk...",
];

pub const GALEWING: Sprite = &[
    "kk..................",
    "kbk.........kkkk....",
    "kbbk.......kbbbbk...",
    ".kbbk.....kbbbbhek..",
    ".kbdbk....kbbbbkkaa.",
    "..kbdbk...kbbbbbkaak",
    "..kbbdbk..kbbbbbkkk.",
    "...kbbdbkkbbbbbk....",
    "...kbbbbbbbbddbk....",
    "....kbbbbbbbdddbk...",
    "..kkkbbbbbbbddddk...",
    ".kbbbbbbbbbbddddk...",
    "kbbkkbbbbbbbbddbk...",
    "kkk..kkbbbbbbbbk....",
    ".......kkbbbbkk.....",
    "........kak.kak.....",
    ".......kaak.kaak....",
    ".......kkkk.kkkk....",
];

pub const MYSTIFUR: Sprite = &[
    "..........k.........",
    ".........kak........",
    "........kayak.......",
    ".......kaaaaak......",
    ".....kaaaaaaaaak....",
    ".....kkkkkkkkkkk....",
    ".......kbbbbbbbk....",
    ".......kbbbbbhebk...",
    ".......kbbbbbkkbbk..",
    ".......kbpbbbbbbbkk.",
    "........kbbbbbbkkk..",
    "...kk...kbddddbk....",
    "..kwwk.kbbddddbbk...",
    ".kwbbwkkbbddddbbk...",
    "kwbbbbbkbbddddbbk...",
    "kbbbbbbbbbddddbbk...",
    ".kbbbbbbbbbddbbbk...",
    "..kkkkbbbbbbbbbk....",
    "......kbbk..kbbk....",
    "......kkkk..kkkk....",
];

pub const BULWARK: Sprite = &[
    "...s....s....s......",
    "..ksk..ksk..ksk.....",
    "..kaakkkaakkkaak....",
    ".kaaaaaaaaaaaaaak...",
    ".kaaakaaaakaaaaak...",
    "kaaaakaaaakaaaakkkk.",
    "kaaaakaaaakaaaakbbbk",
    "kaaaakaaaakaaaakbhek",
    "kkkkkkkkkkkkkkkkbkkk",
    ".kdddddddddddddkbbbk",
    ".kbbbkkkkkkkkbbbkkk.",
    ".kbbbk.......kbbbk..",
    ".kbbbk.......kbbbk..",
    ".kkkkk.......kkkkk..",
];

pub const TIDECREST: Sprite = &[
    "...........a.a......",
    "..........kakak.....",
    "..........kaaaakk...",
    ".........kbbbbbbbk..",
    ".........kbbbhebbbk.",
    ".........kbbbkkbbbbk",
    ".........kbbbbbbkkk.",
    "..........kbdbbk....",
    "...........kbdbk....",
    "...........kbdbbk...",
    "..a.......kbbdbbk...",
    ".kak.....kbbddbk....",
    ".kaak...kbbddbk.....",
    "..kbbk.kbbddbk......",
    "...kbbkbbddbbk......",
    "...kbbbbddbbk.......",
    "....kbbddbbbk.......",
    ".ww..kkkkkkk..ww....",
    "wcw..........wcw....",
];

pub const GRUMBLOO: Sprite = &[
    "..kkkk.....kkkk.....",
    ".kwhewk...kwhewk....",
    ".kwkkwk...kwkkwk....",
    "..kkkk.....kkkk.....",
    "...kbk......kbk.....",
    "...kbk......kbk.....",
    "..kkbbkkkkkkbbkk....",
    ".kbbbbbbbbbbbbbbk...",
    ".kbbbbbbbbbbbbbbbk..",
    "kbbbbbkkbkkbbbbbbk..",
    "kbbbbkbbbbbkbbbbbbk.",
    "kbabbbbbbbbbbbbabbbk",
    "kbakbbbbbbbbbbkakbbk",
    ".kak.kkkkkkkkkkak.kk",
    "..k............k....",
];

// ---------------------------------------------------------------- ultimates (24 wide)

pub const INFERNAX: Sprite = &[
    "kk..............k...k...",
    "kak............kak.kak..",
    "kaak...........kkkkkkk..",
    "kaaak.........kbbbbbbbk.",
    "kakaak........kbbbbhebbk",
    "kaakaak.......kbbbbkkbbk",
    "kaaakaak......kbbbbbbbbk",
    "kakaakaak.....kbbbbkkkkk",
    "kaakaakaak....kbbbkwkwk.",
    "kaaakaakaak..kbbbbkkkk..",
    ".kaaakaakaakkbbbbbk.....",
    "..kkaaakaakbbbbbbbk.....",
    "....kkaaakbbddddbbbk....",
    "......kkkbbdddddbkbk....",
    "........kbbdddddbbk.....",
    ".......kbbbdddddbbk.....",
    "..oy..kbbbbddddbbbk.....",
    ".kyyk.kbbbbbdddbbbk.....",
    ".koyykbbbbbbbbbbbk......",
    "..kkkbbbkkbbbbbbk.......",
    ".......kbbbk.kbbbk......",
    ".......kkkkk.kkkkk......",
];

pub const SERAPHOX: Sprite = &[
    "...............kkkk.....",
    "..............kyyyyk....",
    "...............kkkk.....",
    "..............k....k....",
    "kk...........kbk..kbk...",
    "kaak.........kbbkkbbk...",
    "kaaak........kbbcbbbbk..",
    ".kakak......kbbbbbhebk..",
    ".kaaaak.....kbbbbbkkbbk.",
    "..kakaak....kbpbbbbbbbkk",
    "..kaaaaak....kbbbbbkkk..",
    "...kaakaakk..kbddddbk...",
    "...kakaakaakkbbddddbbk..",
    "....kaakaaakbbbddddbbk..",
    "....kkaaaakbbbbddddbbk..",
    "......kkkkbbbbbddddbbk..",
    "..kkk....kbbbbbddddbbk..",
    ".kbbbk...kbbbbbbddbbbk..",
    "kbbbbbk..kbbbbbbbbbbk...",
    "kbbbbbbkkbbbbbbbbbbbk...",
    ".kkbbbbbbbbbkbbbkbbbk...",
    "...kkkkkkkkkkkkkkkkk....",
];

pub const TITANSHELL: Sprite = &[
    "......kkk.kkk.kkk.......",
    "......kak.kak.kak.......",
    "......kakkkakkkak.......",
    "......kaaaaaaaaak.......",
    "......kaaakkkaaak.......",
    "......kaaakkkaaak.......",
    "......kaaaaaaaaak.......",
    "...kkkkkkkkkkkkkkkkk....",
    "..kbbbbkbbbbbkbbbbbkkkk.",
    ".kbbsbbkbbsbbkbbsbbkbbbk",
    ".kbbbbbkbbbbbkbbbbbkbhek",
    "kbbbbbkbbbbbbbkbbbbkbkkk",
    "kbbsbbkbbbsbbbkbbsbkbbbk",
    "kkkkkkkkkkkkkkkkkkkkbkk.",
    ".kddddddddddddddddkbbk..",
    ".kbbbbkkkkkkkkkkbbbbk...",
    ".kbbbbk........kbbbbk...",
    ".kbbbbk........kbbbbk...",
    ".kkkkkk........kkkkkk...",
];

// ---------------------------------------------------------------- bubble icons (max 7x7)

pub const HEART: Sprite = &[
    ".kk.kk.",
    "krrkrrk",
    "krrrrrk",
    ".krrrk.",
    "..krk..",
    "...k...",
];

pub const MEAT: Sprite = &[
    "...kkk.",
    "..koook",
    ".kooook",
    ".koook.",
    "kwkkk..",
    "kwk....",
    ".k.....",
];

pub const ZZZ: Sprite = &[
    ".......",
    ".kkkkk.",
    "....k..",
    "...k...",
    "..k....",
    ".kkkkk.",
];

pub const BANG: Sprite = &[
    "...rr..",
    "...rr..",
    "...rr..",
    "...rr..",
    ".......",
    "...rr..",
];

pub const STAR: Sprite = &[
    "...k...",
    "..kyk..",
    "kkyyykk",
    "kyyyyyk",
    ".kyyyk.",
    ".kykyk.",
    ".kk.kk.",
];

pub const UP: Sprite = &[
    "...k...",
    "..kgk..",
    ".kgggk.",
    "kgggggk",
    "kkkgkkk",
    "..kgk..",
    "..kkk..",
];

pub const SWEAT: Sprite = &[
    "...k...",
    "..kck..",
    "..kck..",
    ".kccck.",
    ".kcwck.",
    ".kccck.",
    "..kkk..",
];

pub const SHOE: Sprite = &[
    ".......",
    ".kkk...",
    ".krrk..",
    ".krrkk.",
    ".krrrrk",
    "kwwwwwk",
    ".kkkkk.",
];

pub const DUMBBELL: Sprite = &[
    "kk...kk",
    "kskkksk",
    "ksssssk",
    "kskkksk",
    "kk...kk",
];

pub const SHIELD: Sprite = &[
    "kkkkkkk",
    "kssrssk",
    "krrrrrk",
    "kssrssk",
    ".ksrsk.",
    "..ksk..",
    "...k...",
];

pub const BOOK: Sprite = &[
    ".kk.kk.",
    "kyykyyk",
    "kyykyyk",
    "kyykyyk",
    "kkkkkkk",
];

pub const WAVE: Sprite = &[
    ".kk....",
    "kcck.kk",
    "k..kcck",
    ".......",
    ".kk....",
    "kcck.kk",
    "k..kcck",
];

pub const SWORD: Sprite = &[
    ".....kk",
    "....ksk",
    "...ksk.",
    "kkksk..",
    ".knk...",
    "knkk...",
    "kk.....",
];

// ---------------------------------------------------------------- mess

pub const POOP: Sprite = &[
    "...k....",
    "..knk...",
    "..knnk..",
    ".knnnnk.",
    "knnnnnnk",
    "kkkkkkkk",
];
