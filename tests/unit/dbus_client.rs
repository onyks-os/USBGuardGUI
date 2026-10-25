use super::{Resolution, resolve, to_handles};
use crate::model::RuleId;

#[test]
fn resolution_by_text() {
    let rules = to_handles(vec![
        (5, "allow id 1234:5678".into()),
        (9, "block".into()),
        (12, "allow id 1234:5678".into()),
    ]);
    assert_eq!(rules[2].position, 2);
    assert_eq!(resolve(&rules, "block"), Resolution::Unique(RuleId::new(9)));
    assert_eq!(resolve(&rules, "reject"), Resolution::Gone);
    let Resolution::Ambiguous(c) = resolve(&rules, "allow id 1234:5678") else {
        panic!("expected ambiguity");
    };
    assert_eq!(c.iter().map(|h| h.position).collect::<Vec<_>>(), vec![0, 2]);
}
