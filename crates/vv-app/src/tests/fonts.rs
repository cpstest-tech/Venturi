use super::*;

const FAMILIES: &[&str] = &[
    "Anton",
    "Bebas Neue",
    "Bangers",
    "Bungee",
    "Fredoka",
    "Montserrat",
    "Poppins",
    "Playfair Display",
    "Abril Fatface",
    "Pacifico",
    "Lobster",
    "Caveat",
    "Shrikhand",
    "Monoton",
    "Righteous",
    "Rubik Mono One",
    "Press Start 2P",
    "Space Grotesk",
    "Archivo Black",
];

#[test]
fn every_bundled_family_is_registered() {
    register_bundled();
    let families = vv_render::text::font_families();
    for family in FAMILIES {
        assert!(families.iter().any(|f| f == family), "{family} missing");
    }
}

#[test]
fn every_bundled_family_has_its_ofl_licence() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/fonts");
    for family in FAMILIES {
        let name = format!("OFL-{}.txt", family.replace(' ', "").to_lowercase());
        let text = std::fs::read_to_string(dir.join(&name)).unwrap_or_else(|_| panic!("{name}"));
        assert!(text.contains("SIL OPEN FONT LICENSE"), "{name}");
    }
}
