use liquers_lib::ui::shortcuts::{find_conflicts, Key, KeyboardShortcut, Modifiers};

#[test]
fn integration_parse_and_convert_to_egui() -> Result<(), Box<dyn std::error::Error>> {
    let shortcut: KeyboardShortcut = "Ctrl+Shift+S".parse()?;
    let egui_shortcut = shortcut.to_egui();

    // Verify egui conversion
    assert_eq!(egui_shortcut.modifiers.command, true);
    assert_eq!(egui_shortcut.modifiers.shift, true);
    assert_eq!(egui_shortcut.logical_key, egui::Key::S);

    Ok(())
}

#[test]
fn integration_yaml_with_shortcuts() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = r#"
menu:
  items:
  - !button
    label: Save
    shortcut: "Ctrl+S"
    action: null
layout: !vertical
init: []
"#;

    use liquers_lib::ui::widgets::ui_spec_element::UISpec;
    let spec: UISpec = serde_yaml::from_str(yaml)?;

    // Verify YAML parsed correctly
    assert!(spec.menu.is_some());

    Ok(())
}

#[test]
fn integration_conflict_detection_with_menu() {
    // Create shortcuts that conflict (Ctrl and Cmd are semantically the same)
    let shortcuts = vec![
        "Ctrl+S".parse::<KeyboardShortcut>().unwrap(),
        "Cmd+S".parse::<KeyboardShortcut>().unwrap(), // Same as Ctrl+S (semantic)
        "Alt+F4".parse::<KeyboardShortcut>().unwrap(),
    ];

    let conflicts = find_conflicts(&shortcuts);

    // Ctrl+S and Cmd+S are the same (semantic command modifier)
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].1, 2); // Count = 2
}

#[test]
fn integration_platform_aware_display() {
    let shortcut = KeyboardShortcut::new(Modifiers::command(), Key::S);
    let display = shortcut.to_string();

    // Display varies by platform
    #[cfg(target_os = "macos")]
    assert_eq!(display, "Cmd+S");

    #[cfg(not(target_os = "macos"))]
    assert_eq!(display, "Ctrl+S");
}

#[test]
fn integration_parse_all_modifier_combinations() -> Result<(), Box<dyn std::error::Error>> {
    let test_cases = vec![
        "Ctrl+A",
        "Alt+A",
        "Shift+A",
        "Ctrl+Alt+A",
        "Ctrl+Shift+A",
        "Alt+Shift+A",
        "Ctrl+Alt+Shift+A",
    ];

    for case in test_cases {
        let shortcut: KeyboardShortcut = case.parse()?;
        // Verify it parses without error
        assert_eq!(shortcut.key, Key::A);
    }

    Ok(())
}

#[test]
fn integration_semantic_command_modifier() -> Result<(), Box<dyn std::error::Error>> {
    // All these should parse to the same semantic meaning
    let ctrl: KeyboardShortcut = "Ctrl+S".parse()?;
    let cmd: KeyboardShortcut = "Cmd+S".parse()?;
    let command: KeyboardShortcut = "Command+S".parse()?;
    let meta: KeyboardShortcut = "Meta+S".parse()?;

    assert_eq!(ctrl, cmd);
    assert_eq!(ctrl, command);
    assert_eq!(ctrl, meta);

    // All should have ctrl=true
    assert!(ctrl.modifiers.ctrl);
    assert!(cmd.modifiers.ctrl);
    assert!(command.modifiers.ctrl);
    assert!(meta.modifiers.ctrl);

    Ok(())
}

#[test]
fn integration_round_trip_through_string() -> Result<(), Box<dyn std::error::Error>> {
    let original = KeyboardShortcut::new(
        Modifiers {
            ctrl: true,
            alt: true,
            shift: false,
        },
        Key::Q,
    );

    let string_repr = original.to_string();
    let parsed: KeyboardShortcut = string_repr.parse()?;

    assert_eq!(original, parsed);

    Ok(())
}
