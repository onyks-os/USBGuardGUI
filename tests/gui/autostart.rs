use super::*;
#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise() {
    assert!(!sandboxed());
    assert!(!enabled());
    set(true).unwrap();
    assert!(enabled());
    assert_eq!(std::fs::read_to_string(path()).unwrap(), ENTRY);
    set(true).unwrap();
    set(false).unwrap();
    assert!(!enabled());
    set(false).unwrap();
    let parent = path().parent().unwrap().to_owned();
    std::fs::remove_dir(&parent).unwrap();
    std::fs::write(&parent, "blocking file").unwrap();
    assert!(set(true).is_err());
    assert!(set(false).is_err());
    std::fs::remove_file(parent).unwrap();
}
