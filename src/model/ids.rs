//! Strongly typed identifiers for USBGuard domain objects.
//!
//! [`DeviceId`] and [`RuleId`] wrap the same underlying integer (`u32`) but are
//! distinct types: the compiler rejects any attempt to use one where the other
//! is expected (docs/architecture.md §2.4.1).
//!
//! [`UsbId`] and [`InterfaceType`] are the two structured attribute values of
//! the rule language: `vendor:product` and `class:subclass:protocol`.

use std::fmt;

/// Unique identifier assigned by the USBGuard daemon to a connected USB device.
///
/// Device IDs are volatile: they are assigned at insertion time and recycled
/// when the device is removed. A device ID has nothing to do with the device's
/// `id` *attribute*, which is the `vendor:product` pair ([`UsbId`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeviceId(u32);

impl DeviceId {
    /// Creates a new `DeviceId` from the raw value returned by the daemon.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying `u32` value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// Positional identifier assigned by the USBGuard daemon to a policy rule.
///
/// Rule IDs are volatile: they shift whenever a rule is added or removed from
/// the policy. Code must never cache a `RuleId` across operations that modify
/// the ruleset (docs/architecture.md §6.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RuleId(u32);

impl RuleId {
    /// Conventional `parent_id` meaning "append at the end of the ruleset"
    /// (docs/architecture.md §2.4.6).
    pub const APPEND_LAST: Self = Self(u32::MAX - 2);

    /// Creates a new `RuleId` from the raw value returned by the daemon.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the underlying `u32` value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One half of a [`UsbId`]: a 16-bit number, or the `*` wildcard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdPart {
    /// `*` — matches any value.
    Any,
    /// A concrete 16-bit value.
    Value(u16),
}

/// The `id` attribute of a device: `vendor:product`.
///
/// Invariant: a wildcard vendor implies a wildcard product (`*:1234` is not
/// valid rule syntax). The constructor [`UsbId::new`] enforces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UsbId {
    vendor: IdPart,
    product: IdPart,
}

impl UsbId {
    /// Builds a `UsbId`, or `None` if `vendor` is a wildcard and `product` is not.
    #[must_use]
    pub const fn new(vendor: IdPart, product: IdPart) -> Option<Self> {
        match (vendor, product) {
            (IdPart::Any, IdPart::Value(_)) => None,
            _ => Some(Self { vendor, product }),
        }
    }

    /// The vendor half.
    #[must_use]
    pub const fn vendor(self) -> IdPart {
        self.vendor
    }

    /// The product half.
    #[must_use]
    pub const fn product(self) -> IdPart {
        self.product
    }

    /// Parses `vvvv:pppp`, where each half is exactly four hex digits or `*`.
    #[must_use]
    pub fn parse(text: &[u8]) -> Option<Self> {
        let (vendor, product) = split_once(text, b':')?;
        Self::new(parse_id_part(vendor)?, parse_id_part(product)?)
    }
}

impl fmt::Display for UsbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_id_part(f, self.vendor)?;
        f.write_str(":")?;
        write_id_part(f, self.product)
    }
}

fn parse_id_part(text: &[u8]) -> Option<IdPart> {
    if text == b"*" {
        return Some(IdPart::Any);
    }
    if text.len() != 4 {
        return None;
    }
    parse_hex(text)
        .and_then(|v| u16::try_from(v).ok())
        .map(IdPart::Value)
}

fn write_id_part(f: &mut fmt::Formatter<'_>, part: IdPart) -> fmt::Result {
    match part {
        IdPart::Any => f.write_str("*"),
        IdPart::Value(v) => write!(f, "{v:04x}"),
    }
}

/// The `with-interface` attribute: `cc:ss:pp`, three 8-bit hex numbers.
///
/// `None` in `subclass` or `protocol` is the `*` wildcard. Invariant: a
/// wildcard subclass implies a wildcard protocol, enforced by [`InterfaceType::new`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InterfaceType {
    class: u8,
    subclass: Option<u8>,
    protocol: Option<u8>,
}

impl InterfaceType {
    /// Builds an `InterfaceType`, or `None` if `subclass` is a wildcard and
    /// `protocol` is not.
    #[must_use]
    pub const fn new(class: u8, subclass: Option<u8>, protocol: Option<u8>) -> Option<Self> {
        match (subclass, protocol) {
            (None, Some(_)) => None,
            _ => Some(Self {
                class,
                subclass,
                protocol,
            }),
        }
    }

    /// The interface class.
    #[must_use]
    pub const fn class(self) -> u8 {
        self.class
    }

    /// The interface subclass, `None` for `*`.
    #[must_use]
    pub const fn subclass(self) -> Option<u8> {
        self.subclass
    }

    /// The interface protocol, `None` for `*`.
    #[must_use]
    pub const fn protocol(self) -> Option<u8> {
        self.protocol
    }

    /// Parses `cc:ss:pp`, where each part is exactly two hex digits and the
    /// last two may be `*`.
    #[must_use]
    pub fn parse(text: &[u8]) -> Option<Self> {
        let (class, rest) = split_once(text, b':')?;
        let (subclass, protocol) = split_once(rest, b':')?;
        let class = parse_byte(class)?;
        let subclass = parse_wildcard_byte(subclass)?;
        let protocol = parse_wildcard_byte(protocol)?;
        Self::new(class, subclass.into(), protocol.into())
    }
}

impl fmt::Display for InterfaceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02x}:", self.class)?;
        match self.subclass {
            Some(v) => write!(f, "{v:02x}:")?,
            None => f.write_str("*:")?,
        }
        match self.protocol {
            Some(v) => write!(f, "{v:02x}"),
            None => f.write_str("*"),
        }
    }
}

/// One part of `cc:ss:pp` that may be `*`.
#[derive(Clone, Copy)]
enum WildcardByte {
    Any,
    Value(u8),
}

impl From<WildcardByte> for Option<u8> {
    fn from(w: WildcardByte) -> Self {
        match w {
            WildcardByte::Any => None,
            WildcardByte::Value(v) => Some(v),
        }
    }
}

fn parse_wildcard_byte(text: &[u8]) -> Option<WildcardByte> {
    if text == b"*" {
        Some(WildcardByte::Any)
    } else {
        parse_byte(text).map(WildcardByte::Value)
    }
}

fn parse_byte(text: &[u8]) -> Option<u8> {
    if text.len() != 2 {
        return None;
    }
    parse_hex(text).and_then(|v| u8::try_from(v).ok())
}

/// Parses up to eight hex digits. Returns `None` on any non-hex byte or on
/// empty input; callers bound the length before calling.
fn parse_hex(text: &[u8]) -> Option<u32> {
    if text.is_empty() || text.len() > 8 {
        return None;
    }
    text.iter().try_fold(0u32, |acc, &b| {
        let digit = char::from(b).to_digit(16)?;
        Some(acc << 4 | digit)
    })
}

fn split_once(text: &[u8], sep: u8) -> Option<(&[u8], &[u8])> {
    let pos = text.iter().position(|&b| b == sep)?;
    Some((text.get(..pos)?, text.get(pos + 1..)?))
}

#[cfg(test)]
mod tests {
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
}
