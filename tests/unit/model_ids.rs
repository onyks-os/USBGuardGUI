use super::{DeviceId, IdPart, InterfaceType, RuleId, UsbId};

#[test]
fn device_id_round_trip() {
    let id = DeviceId::new(42);
    assert_eq!(id.get(), 42);
}

#[test]
fn rule_id_round_trip() {
    let id = RuleId::new(7);
    assert_eq!(id.get(), 7);
}

#[test]
fn append_last_is_the_upstream_convention() {
    assert_eq!(RuleId::APPEND_LAST.get(), 4_294_967_293);
}

#[test]
fn equality_same_value() {
    assert_eq!(DeviceId::new(1), DeviceId::new(1));
    assert_ne!(DeviceId::new(1), DeviceId::new(2));
}

#[test]
fn debug_format_includes_type_name() {
    let output = format!("{:?}", DeviceId::new(99));
    assert!(output.contains("DeviceId"));
    assert!(output.contains("99"));
}

#[test]
fn usb_id_parses_and_renders_lowercase() {
    let id = UsbId::parse(b"1D6B:0002").unwrap();
    assert_eq!(id.vendor(), IdPart::Value(0x1d6b));
    assert_eq!(id.to_string(), "1d6b:0002");
}

#[test]
fn usb_id_wildcards() {
    assert_eq!(UsbId::parse(b"*:*").unwrap().to_string(), "*:*");
    assert_eq!(UsbId::parse(b"1234:*").unwrap().to_string(), "1234:*");
    assert!(UsbId::parse(b"*:1234").is_none());
}

#[test]
fn usb_id_rejects_malformed() {
    for bad in [
        &b"123:4567"[..],
        b"12345:1234",
        b"12g4:0000",
        b"1234",
        b"",
        b":",
    ] {
        assert!(UsbId::parse(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn interface_type_forms() {
    assert_eq!(
        InterfaceType::parse(b"09:00:00").unwrap().to_string(),
        "09:00:00"
    );
    assert_eq!(
        InterfaceType::parse(b"03:*:*").unwrap().to_string(),
        "03:*:*"
    );
    assert_eq!(
        InterfaceType::parse(b"03:01:*").unwrap().to_string(),
        "03:01:*"
    );
    assert!(InterfaceType::parse(b"03:*:01").is_none());
    assert!(InterfaceType::parse(b"*:00:00").is_none());
    assert!(InterfaceType::parse(b"3:00:00").is_none());
    assert!(InterfaceType::parse(b"03:00").is_none());
}

#[test]
fn structured_components_and_daemon_ids_keep_their_exact_values() {
    let id = UsbId::parse(b"1234:5678").unwrap();
    assert_eq!(id.product(), IdPart::Value(0x5678));
    let interface = InterfaceType::parse(b"08:06:50").unwrap();
    assert_eq!(
        (
            interface.class(),
            interface.subclass(),
            interface.protocol()
        ),
        (8, Some(6), Some(0x50))
    );
    assert_eq!(DeviceId::new(u32::MAX).to_string(), "4294967295");
    assert_eq!(RuleId::new(0).to_string(), "0");
    assert_eq!(super::parse_hex(b""), None);
    assert_eq!(super::parse_hex(b"123456789"), None);
}
