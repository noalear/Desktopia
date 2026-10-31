//! Names and words. Everything here is deterministic in its inputs, so replaying
//! the same facts always writes the same history.

use crate::facts::{Kingdom, Locomotion, Reproduction, StrainTraits, Traits};

pub fn rooted(t: &Traits) -> bool {
    t.kingdom == Kingdom::Rooted
}

/// Size words; sizes are in world units, where 1 is a monitor's height.
pub fn size_class(size: f32) -> usize {
    match size {
        s if s < 0.006 => 0,
        s if s < 0.02 => 1,
        s if s < 0.06 => 2,
        s if s < 0.15 => 3,
        _ => 4,
    }
}

/// A small, stable hash for seeding choices.
pub fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Picks deterministically from a list.
pub fn pick<'a>(seed: u64, from: &[&'a str]) -> &'a str {
    from[(mix(seed) % from.len() as u64) as usize]
}

const ONSETS: &[&str] =
    &["v", "k", "th", "s", "r", "m", "n", "z", "dr", "sk", "h", "l", "g", "vr", "ch", "t", "q", "x", "y", "ul"];
const VOWELS: &[&str] = &["a", "e", "i", "o", "u", "ae", "ei", "au", "y", "a", "o", "e"];
const CODAS: &[&str] = &["", "", "", "r", "n", "th", "s", "k", "sh", "l", "x", "m", "rr", "nd"];

/// An invented word of `syllables` syllables, capitalised.
pub fn coin(seed: u64, syllables: u32) -> String {
    let mut s = String::new();
    let mut h = mix(seed);
    for i in 0..syllables {
        let onset = ONSETS[(h % ONSETS.len() as u64) as usize];
        h = mix(h);
        let vowel = VOWELS[(h % VOWELS.len() as u64) as usize];
        h = mix(h);
        // Codas only close the last syllable or, rarely, a middle one.
        let coda =
            if i + 1 == syllables || h.is_multiple_of(4) { CODAS[(h % CODAS.len() as u64) as usize] } else { "" };
        h = mix(h);
        s.push_str(onset);
        s.push_str(vowel);
        s.push_str(coda);
    }
    let mut c = s.chars();
    match c.next() {
        Some(first) => first.to_uppercase().chain(c).collect(),
        None => s,
    }
}

/// Drops a final vowel so a Latin ending joins cleanly, unless the word is short.
fn stem(mut word: String) -> String {
    if word.len() > 3 && word.ends_with(['a', 'e', 'i', 'o', 'u', 'y']) {
        word.pop();
    }
    word
}

/// A genus name, Latin-ish.
pub fn genus(seed: u64) -> String {
    let end = pick(seed ^ 0x51, &["is", "a", "on", "ax", "eth", "ura", "ith", "os"]);
    format!("{}{end}", stem(coin(seed, 2)))
}

/// Latin endings a genus may carry, longest first so the stem comes clean.
const GENUS_ENDS: &[&str] = &["ura", "eth", "ith", "is", "on", "ax", "os", "a"];

/// A genus without its ending, for building family and order names on.
fn genus_stem(genus: &str) -> &str {
    GENUS_ENDS.iter().find_map(|e| genus.strip_suffix(e).filter(|s| s.len() >= 3)).unwrap_or(genus)
}

/// The two kingdoms of Vesper's life.
pub fn kingdom(k: Kingdom) -> &'static str {
    match k {
        Kingdom::Rooted => "Vesperophyta",
        Kingdom::Motile => "Vesperozoa",
    }
}

/// A family named for its first genus, the way Earth's are: plants end in
/// -aceae, creatures in -idae.
pub fn family(genus: &str, k: Kingdom) -> String {
    let stem = genus_stem(genus);
    match k {
        Kingdom::Rooted => format!("{stem}aceae"),
        Kingdom::Motile => format!("{stem}idae"),
    }
}

/// An order named for its founding genus: plants end in -ales, creatures in -ida.
pub fn order(genus: &str, k: Kingdom) -> String {
    let stem = genus_stem(genus);
    match k {
        Kingdom::Rooted => format!("{stem}ales"),
        Kingdom::Motile => format!("{stem}ida"),
    }
}

/// A species epithet, lower case.
pub fn epithet(seed: u64) -> String {
    let stem = stem(coin(seed ^ 0xe9, 2).to_lowercase());
    format!("{stem}{}", pick(seed ^ 0x77, &["i", "ae", "us", "is", "ens", "or"]))
}

/// Colour word for a hue.
pub fn hue_word(hue: f32, seed: u64) -> &'static str {
    let h = hue.rem_euclid(1.0);
    let options: &[&str] = match (h * 12.0) as u32 {
        0 => &["crimson", "blood"],
        1 => &["rust", "ember"],
        2 => &["amber", "ochre"],
        3 => &["gold", "sulphur"],
        4 => &["moss", "lichen"],
        5 => &["jade", "verdigris"],
        6 => &["teal", "slate"],
        7 => &["cyan", "ice"],
        8 => &["cobalt", "deep"],
        9 => &["violet", "dusk"],
        10 => &["magenta", "orchid"],
        _ => &["rose", "wine"],
    };
    pick(seed, options)
}

/// A field-guide common name like "plated rust trudger". `variant` picks other
/// wordings when a name is already taken.
pub fn common_name(t: &Traits, seed: u64, variant: u32) -> String {
    let s = seed.wrapping_add(variant as u64 * 7919);
    let noun = noun(t, s);
    let mut mods: Vec<&str> = Vec::new();
    if t.armor > 0.6 {
        mods.push(pick(s ^ 1, &["plated", "shelled", "armored", "lacquered"]));
    }
    if t.spines > 0.6 {
        mods.push(pick(s ^ 7, &["thorned", "barbed", "quilled"]));
    }
    if t.toxin > 0.6 {
        mods.push(pick(s ^ 8, &["bitter", "venom", "blister"]));
    }
    if t.glow > 0.6 {
        mods.push(pick(s ^ 2, &["lantern", "glowing", "signal", "flaring"]));
    }
    match size_class(t.size) {
        4 => mods.push(pick(s ^ 3, &["great", "giant", "towering"])),
        0 => mods.push(pick(s ^ 3, &["mote", "dust", "lesser"])),
        _ => {}
    }
    if t.sociality > 0.7 && !rooted(t) {
        mods.push(pick(s ^ 4, &["clan", "swarm", "throng"]));
    }
    if t.zone < 0.2 {
        mods.push(pick(s ^ 5, &["scald", "sunward", "glare"]));
    } else if t.zone > 0.8 {
        mods.push(pick(s ^ 5, &["night", "ice", "dark"]));
    }
    mods.push(hue_word(t.look[0], s ^ 6));
    // Two modifiers at most; the variant decides which.
    let n = mods.len();
    let a = mods[(variant as usize) % n];
    let b = mods[(variant as usize + 1 + (mix(s) as usize % n.max(2))) % n];
    if variant > 0 && a != b { format!("{a} {b} {noun}") } else { format!("{a} {noun}") }
}

/// `name` with one more mark on it, to tell it apart from a namesake.
pub fn marked(seed: u64, name: &str) -> String {
    let mark = pick(
        seed,
        &[
            "banded", "ringed", "spotted", "striped", "pale", "dusky", "bright", "slender", "broad", "hooked",
            "forked", "long", "stub", "fringed", "mottled", "veiled", "sharp", "hollow", "twin", "crested",
        ],
    );
    format!("{mark} {name}")
}

fn noun(t: &Traits, s: u64) -> &'static str {
    if rooted(t) {
        return match () {
            _ if t.diet > 0.4 => pick(s, &["snare", "trapmouth", "maw-bloom"]),
            _ if t.locomotion == Locomotion::FloatingMat => pick(s, &["raft", "float-mat", "skein"]),
            _ if t.photosynthesis < 0.3 => pick(s, &["vent crust", "chimney-moss", "glowcap"]),
            _ if size_class(t.size) >= 3 => pick(s, &["spire", "canopy", "tower-fan"]),
            _ => pick(s, &["fan", "frond", "sail", "film", "tuft"]),
        };
    }
    let hunter = t.diet > 0.6;
    let big = size_class(t.size) >= 3;
    match t.locomotion {
        Locomotion::Drift if hunter => pick(s, &["trailer", "veil", "net"]),
        Locomotion::Drift => pick(s, &["drifter", "float", "bladder", "kite"]),
        Locomotion::Swim if hunter => pick(s, &["lancer", "darter", "shark"]),
        Locomotion::Swim => pick(s, &["finner", "skimmer", "ribbon"]),
        Locomotion::Walk if hunter => pick(s, &["stalker", "runner", "render"]),
        Locomotion::Walk if big => pick(s, &["trudger", "hulk", "browser"]),
        Locomotion::Walk => pick(s, &["strider", "picker", "forager"]),
        Locomotion::Crawl if hunter => pick(s, &["spinner", "ambusher", "clasp"]),
        Locomotion::Crawl => pick(s, &["creeper", "grazer", "scuttler"]),
        Locomotion::Burrow => pick(s, &["delver", "borer", "tunneler"]),
        Locomotion::Glide if hunter => pick(s, &["stooper", "hawk-wing", "shrike"]),
        Locomotion::Glide => pick(s, &["glider", "sailwing", "soarer"]),
        Locomotion::Rooted | Locomotion::FloatingMat => pick(s, &["clinger", "anchor"]),
    }
}

/// Where on the view an x position lies.
/// The world's width. Facts give positions in world units, from the day edge
/// at 0 to the night edge at this.
pub use vesper_terrain::WORLD_W;

/// The region at world position `x`.
pub fn place(x: f32) -> &'static str {
    region(x / WORLD_W)
}

/// The region at `zone`, 0 the day edge to 1 the night edge.
pub fn region(x: f32) -> &'static str {
    match x {
        x if x < 0.12 => "the Scald",
        x if x < 0.30 => "the Amber Flats",
        x if x < 0.45 => "the Violet Reach",
        x if x < 0.58 => "the Midband",
        x if x < 0.72 => "the Teal Hollows",
        x if x < 0.88 => "the Ice Margin",
        _ => "the Dark",
    }
}

/// "the day edge", "the twilight", "the night edge".
pub fn zone_word(x: f32) -> &'static str {
    match x {
        x if x < 0.3 => "the day edge",
        x if x > 0.7 => "the night edge",
        _ => "the twilight",
    }
}

/// A group's name: "the Uskar clan".
pub fn group_name(seed: u64, t: Option<&Traits>) -> String {
    let (social, diet, big) = t.map_or((0.5, 0.0, false), |t| (t.sociality, t.diet, size_class(t.size) >= 3));
    let kind = if social > 0.85 {
        pick(seed, &["colony", "hive", "brood"])
    } else if diet > 0.6 {
        pick(seed, &["pack", "band", "hunt"])
    } else if big {
        pick(seed, &["herd", "clan", "drove"])
    } else {
        pick(seed, &["clan", "shoal", "kin", "troop"])
    };
    format!("the {} {kind}", coin(seed ^ 0x6a, 2))
}

/// A creature's given name.
pub fn creature_name(seed: u64) -> String {
    // Coined names that read as English words get coined again.
    const WORDS: &[&str] = &[
        "her", "his", "him", "she", "the", "and", "they", "them", "this", "that", "yes", "no", "hex", "sex", "nun",
        "ham", "hen", "hum", "rot", "ram", "rat", "tax", "tar", "tin", "ton", "sir", "sin", "sun", "kin", "zen",
    ];
    let mut seed = seed;
    loop {
        let a = coin(seed, 1 + (mix(seed) % 2) as u32);
        let name = if mix(seed ^ 3).is_multiple_of(3) { format!("{a}-{}", coin(seed ^ 0x99, 1)) } else { a };
        if !WORDS.iter().any(|w| name.eq_ignore_ascii_case(w)) {
            return name;
        }
        seed = mix(seed ^ 0x5eed);
    }
}

/// A pathogen's name: "the Grey Wasting", "the Hollow Fluke", or for a
/// partner that helps its host, "the Gold Lodger".
pub fn plague_name(seed: u64, t: &StrainTraits) -> String {
    let (adjs, nouns): (&[&str], &[&str]) = if t.virulence < 0.0 || t.photosynthetic > 0.5 {
        (&["gold", "bright", "kind", "green", "quiet"], &["lodger", "partner", "film", "bloom"])
    } else if t.visible > 0.5 {
        (&["hollow", "coiled", "black", "pale", "hooked", "glass"], &["fluke", "worm", "brood", "threader", "burrower"])
    } else {
        (
            &["grey", "weeping", "burning", "black", "glass", "silent", "shaking", "pale"],
            &["wasting", "rot", "fever", "blight", "pox", "dimming", "creep"],
        )
    };
    format!("the {} {}", capital(pick(seed, adjs)), capital(pick(seed ^ 0x3c, nouns)))
}

/// "Orchid Soarers", "Vent-Crust Mats".
pub fn title_case(s: &str) -> String {
    s.split(' ').map(|w| w.split('-').map(capital).collect::<Vec<_>>().join("-")).collect::<Vec<_>>().join(" ")
}

pub fn capital(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Seconds as "40 seconds", "12 minutes", "3 hours", "2 days".
pub fn span(secs: f64) -> String {
    let (n, unit) = match secs {
        s if s < 90.0 => (s, "second"),
        s if s < 5400.0 => (s / 60.0, "minute"),
        s if s < 129_600.0 => (s / 3600.0, "hour"),
        s => (s / 86_400.0, "day"),
    };
    let n = n.round().max(1.0) as u64;
    if n == 1 { format!("1 {unit}") } else { format!("{n} {unit}s") }
}

/// A field-guide paragraph describing a species.
pub fn describe(t: &Traits) -> String {
    let size = ["A speck-sized", "A small", "A middling", "A large", "A huge"][size_class(t.size)];
    let kind = match t.locomotion {
        Locomotion::Rooted => "rooted plant",
        Locomotion::FloatingMat => "floating plant mat",
        Locomotion::Drift => "drifter",
        Locomotion::Swim => "air-swimmer",
        Locomotion::Walk => "walker",
        Locomotion::Crawl => "crawler",
        Locomotion::Burrow => "burrower",
        Locomotion::Glide => "glider",
    };
    let mut out = format!("{size} {kind} of {}.", zone_word(t.zone));
    let plant = rooted(t);
    let feeds = match () {
        _ if plant && t.diet > 0.4 => " It traps and digests whatever blunders into it.",
        _ if plant && t.photosynthesis < 0.3 => " It feeds on the chemistry of hot vents.",
        _ if plant => " It feeds on light.",
        _ if t.photosynthesis > 0.5 => " It carries light-film and feeds on the sun as well as food.",
        _ if t.diet > 0.85 => " It eats only flesh.",
        _ if t.diet > 0.6 => " It is mostly a hunter.",
        _ if t.diet > 0.35 => " It eats whatever it can, plant or flesh.",
        _ => " It grazes on plants and films.",
    };
    out.push_str(feeds);
    let mut defences = Vec::new();
    if t.armor > 0.6 {
        defences.push("heavy armor");
    }
    if t.spines > 0.5 {
        defences.push("spines");
    }
    if t.toxin > 0.5 {
        defences.push("poisoned flesh");
    }
    match defences.as_slice() {
        [] => {}
        [a] => out.push_str(&format!(" Defended by {a}.")),
        [a, b] => out.push_str(&format!(" Defended by {a} and {b}.")),
        [a, b, c, ..] => out.push_str(&format!(" Defended by {a}, {b} and {c}.")),
    }
    if t.glow > 0.5 {
        out.push_str(if plant { " It glows." } else { " It talks and hunts in light." });
    }
    if !t.features.is_empty() {
        let words: Vec<&str> = t.features.iter().take(6).map(|s| s.as_str()).collect();
        out.push_str(&format!(" It has {}.", list(&words)));
    }
    out.push_str(match t.reproduction {
        Reproduction::Budding => " It buds copies of itself.",
        Reproduction::Spores => " It scatters spores by the thousand.",
        Reproduction::Eggs => " It lays eggs.",
        Reproduction::Mating => " Pairs court and mate.",
        Reproduction::Seeds => " Its seeds travel on wind, water and passing bodies.",
    });
    if !plant {
        out.push_str(match t.sociality {
            s if s > 0.85 => " It lives in colonies with a single breeder.",
            s if s > 0.55 => " It lives in clans.",
            s if s > 0.25 => " It gathers loosely with its own kind.",
            _ => " It lives alone.",
        });
        if t.neurons > 150 {
            out.push_str(" Clever, with a long memory for faces.");
        } else if t.neurons > 50 {
            out.push_str(" It learns quickly.");
        }
    }
    out.push_str(&format!(" Lives about {}.", span(t.lifespan as f64)));
    out
}

/// The clearest difference between a species and its parent, as a phrase:
/// "larger", "heavily armored", "night-dwelling".
pub fn difference(child: &Traits, parent: &Traits) -> String {
    let mut diffs: Vec<(f32, &str)> = vec![
        (
            (child.size / parent.size.max(1e-4)).ln().abs() * 0.6,
            if child.size > parent.size { "larger" } else { "smaller" },
        ),
        (
            (child.armor - parent.armor).abs(),
            if child.armor > parent.armor { "more heavily armored" } else { "lighter-bodied" },
        ),
        ((child.glow - parent.glow).abs(), if child.glow > parent.glow { "brighter" } else { "darker" }),
        (
            (child.diet - parent.diet).abs() * 1.2,
            if child.diet > parent.diet { "more predatory" } else { "more of a grazer" },
        ),
        (
            (child.sociality - parent.sociality).abs(),
            if child.sociality > parent.sociality { "more social" } else { "more solitary" },
        ),
        (
            (child.zone - parent.zone).abs() * 1.5,
            if child.zone > parent.zone { "living closer to the night" } else { "living closer to the sun" },
        ),
        (
            ((child.neurons as f32 + 1.0) / (parent.neurons as f32 + 1.0)).ln().abs() * 0.5,
            if child.neurons > parent.neurons { "bigger-brained" } else { "simpler-minded" },
        ),
        (
            ((child.lifespan + 1.0) / (parent.lifespan + 1.0)).ln().abs() * 0.4,
            if child.lifespan > parent.lifespan { "longer-lived" } else { "faster-living" },
        ),
    ];
    if child.locomotion != parent.locomotion {
        diffs.push((0.9, "moving in a new way"));
    }
    diffs.sort_by(|a, b| b.0.total_cmp(&a.0));
    // Organs gained and lost say more than any shift in degree.
    let organs = organ_change(child, parent);
    let take = if organs.is_some() { 1 } else { 2 };
    let top: Vec<&str> = diffs.iter().take_while(|d| d.0 > 0.08).take(take).map(|d| d.1).collect();
    match (top.as_slice(), organs) {
        ([], None) => "almost identical to look at".into(),
        ([], Some(o)) => o,
        ([a], Some(o)) => format!("{o}, and {a}"),
        ([a], None) => (*a).into(),
        ([a, b, ..], _) => format!("{a} and {b}"),
    }
}

/// The organs a species has that its parent lacked and the other way round,
/// as a phrase: "with fangs and fur, and without eyes".
pub fn organ_change(child: &Traits, parent: &Traits) -> Option<String> {
    if child.features.is_empty() || parent.features.is_empty() {
        return None;
    }
    let gained: Vec<&str> =
        child.features.iter().filter(|f| !parent.features.contains(f)).take(3).map(|s| s.as_str()).collect();
    let lost: Vec<&str> =
        parent.features.iter().filter(|f| !child.features.contains(f)).take(2).map(|s| s.as_str()).collect();
    match (gained.is_empty(), lost.is_empty()) {
        (true, true) => None,
        (false, true) => Some(format!("with {}", list(&gained))),
        (true, false) => Some(format!("without the {} its ancestors had", list(&lost))),
        (false, false) => Some(format!("with {}, and without {}", list(&gained), list(&lost))),
    }
}

/// "a", "a and b", "a, b and c".
pub fn list(words: &[&str]) -> String {
    match words {
        [] => String::new(),
        [a] => (*a).to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_stable_and_readable() {
        assert_eq!(coin(7, 2), coin(7, 2));
        assert_ne!(genus(1), genus(2));
        for i in 0..200 {
            let g = genus(i);
            assert!(g.len() >= 3 && g.len() < 16, "{g}");
            assert!(g.chars().next().unwrap().is_uppercase());
        }
    }

    #[test]
    fn spans_read_naturally() {
        assert_eq!(span(30.0), "30 seconds");
        assert_eq!(span(3600.0), "60 minutes");
        assert_eq!(span(7200.0), "2 hours");
        assert_eq!(span(86_400.0 * 3.0), "3 days");
    }
}
