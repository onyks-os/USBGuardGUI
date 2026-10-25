use super::Parameter;

#[test]
fn names_round_trip() {
    for p in Parameter::ALL {
        assert_eq!(Parameter::from_name(p.name()), Some(p));
    }
    assert_eq!(Parameter::from_name("RuleFile"), None);
}

#[test]
fn display_uses_the_daemons_exact_parameter_name() {
    for p in Parameter::ALL {
        assert_eq!(p.to_string(), p.name());
    }
}
