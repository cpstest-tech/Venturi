use super::*;

#[test]
fn a_written_matte_reads_back_and_lends_itself_to_the_next_frame() {
    let hash = 0x7E57_3A77_E000_0001;
    let _ = std::fs::remove_dir_all(matte_dir(hash));
    let matte = Matte {
        width: 3,
        height: 2,
        data: vec![0, 128, 255, 255, 128, 0],
    };
    write_matte(hash, 10, &matte).unwrap();
    assert_eq!(load_matte(hash, 10).as_deref(), Some(&matte));
    assert_eq!(load_matte(hash, 11).as_deref(), Some(&matte));
    assert!(load_matte(hash, 20).is_none());
    assert!(!mattes_complete(hash));
    mark_mattes_complete(hash).unwrap();
    assert!(mattes_complete(hash));
    let _ = std::fs::remove_dir_all(matte_dir(hash));
}
