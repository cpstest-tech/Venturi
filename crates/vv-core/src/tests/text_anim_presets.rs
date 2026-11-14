use super::*;

#[test]
fn out_list_has_the_mirrors_and_extras() {
    let ids = out_ids();
    assert!(ids.contains(&"dissolve") && ids.contains(&"fall") && ids.contains(&"pop"));
    assert!(out_preset("pop").unwrap().mirror);
    assert!(!out_preset("fall").unwrap().mirror);
}
