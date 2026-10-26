#[test]
fn development_instance_can_be_isolated_without_changing_release_identity() {
    assert_eq!(super::application_id(false), super::APP_ID);
    if cfg!(debug_assertions) {
        assert_eq!(
            super::application_id(true),
            format!("{}.Development", super::APP_ID)
        );
    } else {
        assert_eq!(super::application_id(true), super::APP_ID);
    }
}
