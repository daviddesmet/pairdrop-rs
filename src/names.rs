//! Display and device names, ported from `_setName` in `server/peer.js`.
//!
//! Word lists are PairDrop's, via `unique-names-generator` (colors + animals). The selection is
//! our own: a peer keeps one name for as long as it keeps its id, which is all the client needs.

/// cyrb53 by bryc (github.com/bryc), public domain -- the seed upstream feeds its name generator.
pub fn cyrb53(input: &str, seed: u32) -> u64 {
    let mut h1: u32 = 0xdead_beef ^ seed;
    let mut h2: u32 = 0x41c6_ce57 ^ seed;

    for ch in input.encode_utf16() {
        let ch = ch as u32;
        h1 = (h1 ^ ch).wrapping_mul(2_654_435_761);
        h2 = (h2 ^ ch).wrapping_mul(1_597_334_677);
    }

    h1 = (h1 ^ (h1 >> 16)).wrapping_mul(2_246_822_507) ^ (h2 ^ (h2 >> 13)).wrapping_mul(3_266_489_909);
    h2 = (h2 ^ (h2 >> 16)).wrapping_mul(2_246_822_507) ^ (h1 ^ (h1 >> 13)).wrapping_mul(3_266_489_909);

    4_294_967_296u64.wrapping_mul((2_097_151 & h2) as u64) + h1 as u64
}

/// "Apricot Kiwi" -- stable for a given peer id.
pub fn display_name(peer_id: &str) -> String {
    let color = COLORS[(cyrb53(peer_id, 0) % COLORS.len() as u64) as usize];
    let animal = ANIMALS[(cyrb53(peer_id, 1) % ANIMALS.len() as u64) as usize];
    format!("{} {}", capitalize(color), capitalize(animal))
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

const COLORS: [&str; 52] = [
    "amaranth", "amber", "amethyst", "apricot", "aqua", "aquamarine", "azure", "beige", "black",
    "blue", "blush", "bronze", "brown", "chocolate", "coffee", "copper", "coral", "crimson",
    "cyan", "emerald", "fuchsia", "gold", "gray", "green", "harlequin", "indigo", "ivory",
    "jade", "lavender", "lime", "magenta", "maroon", "moccasin", "olive", "orange", "peach",
    "pink", "plum", "purple", "red", "rose", "salmon", "sapphire", "scarlet", "silver", "tan",
    "teal", "tomato", "turquoise", "violet", "white", "yellow"
];

const ANIMALS: [&str; 355] = [
    "aardvark", "aardwolf", "albatross", "alligator", "alpaca", "amphibian", "anaconda",
    "angelfish", "anglerfish", "ant", "anteater", "antelope", "antlion", "ape", "aphid",
    "armadillo", "asp", "baboon", "badger", "bandicoot", "barnacle", "barracuda", "basilisk",
    "bass", "bat", "bear", "beaver", "bedbug", "bee", "beetle", "bird", "bison", "blackbird",
    "boa", "boar", "bobcat", "bobolink", "bonobo", "booby", "bovid", "bug", "butterfly",
    "buzzard", "camel", "canid", "canidae", "capybara", "cardinal", "caribou", "carp", "cat",
    "caterpillar", "catfish", "catshark", "cattle", "centipede", "cephalopod", "chameleon",
    "cheetah", "chickadee", "chicken", "chimpanzee", "chinchilla", "chipmunk", "cicada", "clam",
    "clownfish", "cobra", "cockroach", "cod", "condor", "constrictor", "coral", "cougar", "cow",
    "coyote", "crab", "crane", "crawdad", "crayfish", "cricket", "crocodile", "crow", "cuckoo",
    "damselfly", "deer", "dingo", "dinosaur", "dog", "dolphin", "donkey", "dormouse", "dove",
    "dragon", "dragonfly", "duck", "eagle", "earthworm", "earwig", "echidna", "eel", "egret",
    "elephant", "elk", "emu", "ermine", "falcon", "felidae", "ferret", "finch", "firefly",
    "fish", "flamingo", "flea", "fly", "flyingfish", "fowl", "fox", "frog", "galliform",
    "gamefowl", "gayal", "gazelle", "gecko", "gerbil", "gibbon", "giraffe", "goat", "goldfish",
    "goose", "gopher", "gorilla", "grasshopper", "grouse", "guan", "guanaco", "guineafowl",
    "gull", "guppy", "haddock", "halibut", "hamster", "hare", "harrier", "hawk", "hedgehog",
    "heron", "herring", "hippopotamus", "hookworm", "hornet", "horse", "hoverfly",
    "hummingbird", "hyena", "iguana", "impala", "jackal", "jaguar", "jay", "jellyfish",
    "junglefowl", "kangaroo", "kingfisher", "kite", "kiwi", "koala", "koi", "krill", "ladybug",
    "lamprey", "landfowl", "lark", "leech", "lemming", "lemur", "leopard", "leopon", "limpet",
    "lion", "lizard", "llama", "lobster", "locust", "loon", "louse", "lungfish", "lynx",
    "macaw", "mackerel", "magpie", "mammal", "manatee", "mandrill", "marlin", "marmoset",
    "marmot", "marsupial", "marten", "mastodon", "meadowlark", "meerkat", "mink", "minnow",
    "mite", "mockingbird", "mole", "mollusk", "mongoose", "monkey", "moose", "mosquito", "moth",
    "mouse", "mule", "muskox", "narwhal", "newt", "nightingale", "ocelot", "octopus", "opossum",
    "orangutan", "orca", "ostrich", "otter", "owl", "ox", "panda", "panther", "parakeet",
    "parrot", "parrotfish", "partridge", "peacock", "peafowl", "pelican", "penguin", "perch",
    "pheasant", "pig", "pigeon", "pike", "pinniped", "piranha", "planarian", "platypus", "pony",
    "porcupine", "porpoise", "possum", "prawn", "primate", "ptarmigan", "puffin", "puma",
    "python", "quail", "quelea", "quokka", "rabbit", "raccoon", "rat", "rattlesnake", "raven",
    "reindeer", "reptile", "rhinoceros", "roadrunner", "rodent", "rook", "rooster", "roundworm",
    "sailfish", "salamander", "salmon", "sawfish", "scallop", "scorpion", "seahorse", "shark",
    "sheep", "shrew", "shrimp", "silkworm", "silverfish", "skink", "skunk", "sloth", "slug",
    "smelt", "snail", "snake", "snipe", "sole", "sparrow", "spider", "spoonbill", "squid",
    "squirrel", "starfish", "stingray", "stoat", "stork", "sturgeon", "swallow", "swan",
    "swift", "swordfish", "swordtail", "tahr", "takin", "tapir", "tarantula", "tarsier",
    "termite", "tern", "thrush", "tick", "tiger", "tiglon", "toad", "tortoise", "toucan",
    "trout", "tuna", "turkey", "turtle", "tyrannosaurus", "unicorn", "urial", "vicuna", "viper",
    "vole", "vulture", "wallaby", "walrus", "warbler", "wasp", "weasel", "whale", "whippet",
    "whitefish", "wildcat", "wildebeest", "wildfowl", "wolf", "wolverine", "wombat",
    "woodpecker", "worm", "wren", "xerinae", "yak", "zebra"
];

/// What the client shows under the display name, e.g. "Mac Safari" or "Android Pixel 8".
///
/// Upstream runs `ua-parser-js`; this covers the platforms that actually reach a LAN drop tool
/// and falls back to "Unknown Device" exactly as upstream does.
pub fn device_name(user_agent: Option<&str>) -> String {
    let ua = user_agent.unwrap_or("");

    let os = if ua.contains("iPhone") {
        Some("iOS")
    } else if ua.contains("iPad") {
        Some("iPadOS")
    } else if ua.contains("Android") {
        Some("Android")
    } else if ua.contains("Macintosh") || ua.contains("Mac OS") {
        Some("Mac")
    } else if ua.contains("Windows") {
        Some("Windows")
    } else if ua.contains("CrOS") {
        Some("Chrome OS")
    } else if ua.contains("Linux") {
        Some("Linux")
    } else {
        None
    };

    // Order matters: every Chromium UA also claims Safari, and Edge claims both.
    let browser = if ua.contains("Edg/") {
        Some("Edge")
    } else if ua.contains("OPR/") || ua.contains("Opera") {
        Some("Opera")
    } else if ua.contains("Firefox/") {
        Some("Firefox")
    } else if ua.contains("Chrome/") {
        Some("Chrome")
    } else if ua.contains("Safari/") {
        Some("Safari")
    } else {
        None
    };

    match (os, browser) {
        (Some(os), Some(browser)) => format!("{os} {browser}"),
        (Some(os), None) => os.to_string(),
        (None, Some(browser)) => browser.to_string(),
        (None, None) => "Unknown Device".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{device_name, display_name};

    #[test]
    fn display_name_is_stable_per_peer_id() {
        let id = "ce5ac6a0-8633-483e-88a8-bf6843f81f0a";
        assert_eq!(display_name(id), display_name(id));
        assert_ne!(display_name(id), display_name("7b7d6170-61a3-4c69-bc40-7bb0dfc6dad7"));
        assert_eq!(display_name(id).split(' ').count(), 2);
    }

    #[test]
    fn device_name_prefers_the_real_browser_over_the_safari_token() {
        let chrome_on_mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
                             (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36";
        assert_eq!(device_name(Some(chrome_on_mac)), "Mac Chrome");

        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like \
                    Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0";
        assert_eq!(device_name(Some(edge)), "Windows Edge");

        assert_eq!(device_name(None), "Unknown Device");
    }
}
