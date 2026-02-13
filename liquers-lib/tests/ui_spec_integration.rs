//! Integration tests for UISpecElement YAML parsing and creation

use liquers_lib::ui::widgets::ui_spec_element::{
    LayoutSpec, MenuAction, MenuBarSpec, MenuItem, TopLevelItem, UISpec, UISpecElement,
};
use liquers_lib::ui::UIElement;

#[test]
fn test_ui_spec_parse_basic() {
    // YAML with basic layout
    let yaml = r#"
init: []
layout: horizontal
"#;

    // Parse YAML
    let spec = UISpec::from_yaml(yaml).expect("YAML parsing should succeed");

    // Verify spec
    assert_eq!(spec.init.len(), 0);
    assert!(spec.menu.is_none());

    // Create element
    let element = UISpecElement::from_spec("Test UI".to_string(), spec);

    // Verify element basics
    assert_eq!(element.type_name(), "UISpecElement");
    assert_eq!(element.title(), "Test UI");
}

#[test]
fn test_ui_spec_parse_with_init() {
    let yaml = r#"
init:
  - "-R/test.txt/-/ns-lui/add-child"
  - "-R/test2.txt/-/ns-lui/add-child"
layout: vertical
"#;

    let spec = UISpec::from_yaml(yaml).expect("YAML parsing should succeed");

    // Verify init queries
    assert_eq!(spec.init.len(), 2);
    assert!(spec.menu.is_none());

    // Create element
    let element = UISpecElement::from_spec("Test UI".to_string(), spec);
    assert_eq!(element.type_name(), "UISpecElement");
}

#[test]
fn test_ui_spec_parse_with_menu() {
    // Create spec programmatically and serialize to see format
    let spec = UISpec {
        init: vec![],
        menu: Some(MenuBarSpec {
            items: vec![TopLevelItem::Menu {
                label: "File".to_string(),
                shortcut: None,
                items: vec![MenuItem::Button {
                    label: "Quit".to_string(),
                    icon: None,
                    shortcut: Some("Ctrl+Q".to_string()),
                    action: MenuAction::Quit,
                }],
            }],
        }),
        layout: LayoutSpec::Horizontal,
    };

    let yaml_out = serde_yaml::to_string(&spec).expect("Serialization should work");
    println!("Menu YAML:\n{}", yaml_out);

    // Deserialize it back
    let spec = UISpec::from_yaml(&yaml_out).expect("YAML parsing should succeed");

    // Verify menu
    assert!(spec.menu.is_some());
    if let Some(ref menu) = spec.menu {
        assert_eq!(menu.items.len(), 1);

        // The first item should be a menu
        if let liquers_lib::ui::widgets::ui_spec_element::TopLevelItem::Menu {
            label,
            items,
            shortcut: _,
        } = &menu.items[0]
        {
            assert_eq!(label, "File");
            assert_eq!(items.len(), 1);

            // The first menu item should be a button
            if let liquers_lib::ui::widgets::ui_spec_element::MenuItem::Button {
                label: item_label,
                ..
            } = &items[0]
            {
                assert_eq!(item_label, "Quit");
            } else {
                panic!("Expected Button menu item");
            }
        } else {
            panic!("Expected Menu variant");
        }
    }

    // Create element
    let element = UISpecElement::from_spec("Test UI".to_string(), spec);
    assert_eq!(element.type_name(), "UISpecElement");
}

#[test]
fn test_ui_spec_parse_grid_layout() {
    // Serde_yaml uses YAML tags (!) for externally tagged enums
    let yaml = r#"
layout: !grid
  rows: 2
  columns: 3
"#;

    let spec = UISpec::from_yaml(yaml).expect("YAML parsing should succeed");

    // Verify
    if let LayoutSpec::Grid { rows, columns } = spec.layout {
        assert_eq!(rows, 2);
        assert_eq!(columns, 3);
    } else {
        panic!("Expected Grid layout");
    }

    // Create element
    let element = UISpecElement::from_spec("Grid UI".to_string(), spec);
    assert_eq!(element.type_name(), "UISpecElement");
}

#[test]
fn test_ui_spec_parse_tabs_layout() {
    // Serde_yaml uses YAML tags (!) for externally tagged enums
    let yaml = r#"
layout: !tabs
  selected: 0
"#;

    let spec = UISpec::from_yaml(yaml).expect("YAML parsing should succeed");

    // Verify
    if let LayoutSpec::Tabs { selected } = spec.layout {
        assert_eq!(selected, 0);
    } else {
        panic!("Expected Tabs layout");
    }

    // Create element
    let element = UISpecElement::from_spec("Tabs UI".to_string(), spec);
    assert_eq!(element.type_name(), "UISpecElement");
}

#[test]
fn test_ui_spec_parse_invalid_yaml() {
    let yaml = "invalid: { yaml: [syntax";

    let result = UISpec::from_yaml(yaml);
    assert!(result.is_err(), "Invalid YAML should return error");
}

#[test]
fn test_ui_spec_parse_windows_layout() {
    let yaml = r#"
layout: windows
"#;

    let spec = UISpec::from_yaml(yaml).expect("YAML parsing should succeed");

    // Create element
    let element = UISpecElement::from_spec("Windows UI".to_string(), spec);
    assert_eq!(element.type_name(), "UISpecElement");
}

#[test]
fn test_ui_spec_shortcut_validation() {
    let yaml = r#"
layout: horizontal
menu:
  items:
  - !menu
    label: File
    items:
    - !button
      label: Action 1
      shortcut: Ctrl+Q
      action: null
    - !button
      label: Action 2
      shortcut: Ctrl+Q
      action: null
"#;

    let spec = UISpec::from_yaml(yaml).expect("YAML parsing should succeed");

    // Verify shortcut conflict detection
    if let Some(menu) = &spec.menu {
        let conflicts = menu.validate_shortcuts();
        assert_eq!(conflicts.len(), 1, "Should detect one conflicting shortcut");

        // Find the Ctrl+Q conflict
        let ctrl_q_conflict = conflicts.iter().find(|(key, _)| key == "Ctrl+Q");
        assert!(
            ctrl_q_conflict.is_some(),
            "Should find Ctrl+Q in conflicts"
        );
        assert_eq!(
            ctrl_q_conflict.unwrap().1,
            2,
            "Ctrl+Q should be defined twice"
        );
    } else {
        panic!("Menu should be present");
    }
}
