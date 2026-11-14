//! Fonts shipped with the app; imported reference faces live in `fonts/capcut`.

macro_rules! font {
    ($file:literal) => {
        include_bytes!(concat!("../assets/fonts/", $file)) as &[u8]
    };
}

const BUNDLED: &[&[u8]] = &[
    font!("Anton-Regular.ttf"),
    font!("BebasNeue-Regular.ttf"),
    font!("capcut/Bangers-Regular.ttf"),
    font!("Bungee-Regular.ttf"),
    font!("Fredoka.ttf"),
    font!("Montserrat.ttf"),
    font!("Poppins-Regular.ttf"),
    font!("Poppins-Bold.ttf"),
    font!("Poppins-Black.ttf"),
    font!("PlayfairDisplay.ttf"),
    font!("AbrilFatface-Regular.ttf"),
    font!("Pacifico-Regular.ttf"),
    font!("Lobster-Regular.ttf"),
    font!("Caveat.ttf"),
    font!("Shrikhand-Regular.ttf"),
    font!("Monoton-Regular.ttf"),
    font!("Righteous-Regular.ttf"),
    font!("RubikMonoOne-Regular.ttf"),
    font!("PressStart2P-Regular.ttf"),
    font!("SpaceGrotesk.ttf"),
    font!("ArchivoBlack-Regular.ttf"),
    font!("capcut/CoreSansD77CnBlack-Italic.otf"),
    font!("capcut/ZY Modern.ttf"),
    font!("capcut/tmpsequel100wide-75.ttf"),
    font!("capcut/en.ttf"),
    font!("capcut/ZY Resolve.ttf"),
];

pub fn register_bundled() {
    for data in BUNDLED {
        vv_render::text::register_font(data.to_vec());
    }
}

#[cfg(test)]
#[path = "tests/fonts.rs"]
mod tests;
