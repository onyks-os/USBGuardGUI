use super::super::tests::{button, widgets};
use super::*;
use crate::dbus::client::to_handles;
#[allow(clippy::unwrap_used)]
pub(in crate::ui) fn exercise() {
    let actions = Rc::new(RefCell::new(Vec::new()));
    let view = PolicyView::new(Rc::new({
        let actions = actions.clone();
        move |a| actions.borrow_mut().push(a)
    }));
    assert_eq!(
        view.list_stack.visible_child_name().as_deref(),
        Some("loading")
    );
    assert!(view.parameters.iter().all(|p| !p.row.is_sensitive()));
    view.revert_parameter(Parameter::InsertedDevicePolicy);
    view.set_parameter(Parameter::ImplicitPolicyTarget, "block");
    view.set_blocks(None, Some("IPC access denied"));
    view.set_parameter(Parameter::ImplicitPolicyTarget, "reject");
    assert!(
        !view.parameters[0].row.is_sensitive(),
        "daemon signals must not undo a structural denial"
    );
    view.set_parameter(Parameter::InsertedDevicePolicy, "future-policy");
    view.set_blocks(None, None);
    assert!(
        !view.parameters[1].row.is_sensitive(),
        "unknown daemon values cannot become commands"
    );
    view.set_parameter(Parameter::InsertedDevicePolicy, "apply-policy");
    assert!(view.parameters[1].row.is_sensitive());
    assert!(
        actions.borrow().is_empty(),
        "daemon refreshes must not emit writes"
    );
    view.parameters[0].row.set_selected(0);
    assert_eq!(
        actions.borrow().last(),
        Some(&PolicyAction::SetParameter {
            parameter: Parameter::ImplicitPolicyTarget,
            old: "reject".into(),
            new: "allow".into()
        })
    );
    view.revert_parameter(Parameter::ImplicitPolicyTarget);
    assert_eq!(view.parameters[0].row.selected(), 2);
    view.set_error("<daemon error>");
    assert_eq!(
        view.error.description().as_deref(),
        Some("&lt;daemon error&gt;")
    );
    view.set_rules(Vec::new());
    assert_eq!(
        view.list_stack.visible_child_name().as_deref(),
        Some("empty")
    );
    let rules = to_handles(vec![
        (9, "allow label \"<admin>\"".into()),
        (2, "malformed text".into()),
    ]);
    view.set_rules(rules.clone());
    assert_eq!(view.rules(), rules);
    let rows = widgets::<adw::ActionRow>(&view.list);
    assert_eq!(rows[0].title(), rules[0].text);
    assert!(!rows[0].uses_markup());
    assert_eq!(rows[0].subtitle().as_deref(), Some("label: <admin>"));
    let remove = widgets::<gtk::Button>(&rows[0])
        .into_iter()
        .find(|b| b.icon_name().as_deref() == Some("user-trash-symbolic"))
        .unwrap();
    remove.emit_clicked();
    assert_eq!(
        actions.borrow().last(),
        Some(&PolicyAction::Remove(rules[0].clone()))
    );
    button(&view.root, "Add Rule…").emit_clicked();
    assert_eq!(actions.borrow().last(), Some(&PolicyAction::Add));
    view.set_blocks(Some("read only"), Some("read only"));
    assert!(!view.add_button.is_sensitive());
    assert!(
        widgets::<gtk::Button>(&view.list)
            .iter()
            .filter(|b| b.icon_name().as_deref() == Some("user-trash-symbolic"))
            .all(|b| !b.is_sensitive())
    );
}
