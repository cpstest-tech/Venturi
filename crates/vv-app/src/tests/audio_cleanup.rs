use super::*;

fn effect(kind: AudioEffectKind) -> AudioEffect {
    AudioEffect::new(kind)
}

#[test]
fn a_new_noise_reduction_goes_first_and_the_level_right_after_it() {
    let chain = [
        effect(Cleanup::Noise.default_kind()),
        effect(AudioEffectKind::Mono),
    ];
    assert_eq!(Cleanup::Noise.insert_at(&chain), 0);
    assert_eq!(Cleanup::Level.insert_at(&chain), 1);
    assert_eq!(Cleanup::Level.insert_at(&chain[1..]), 0);
    assert_eq!(Cleanup::Level.insert_at(&chain[..1]), 1);
}

#[test]
fn each_cleanup_finds_only_its_own_effect() {
    let noise = Cleanup::Noise.default_kind();
    let level = Cleanup::Level.default_kind();
    assert!(Cleanup::Noise.matches(&noise) && !Cleanup::Noise.matches(&level));
    assert!(Cleanup::Level.matches(&level) && !Cleanup::Level.matches(&noise));
}
