use agent_home::{HomeCommand, SmartHomeHub};

/// Intégration home : découverte du hub + actionneur non-critique et critique.
#[test]
fn test_home_integration_hub_and_actuators() {
    let mut hub = SmartHomeHub::new();
    assert_eq!(hub.discover_devices().len(), 4, "hub seedé attendu");
    assert_eq!(hub.get_devices_by_room("Salon").len(), 2);

    // Actionneur non-critique : luminosité sans confirmation.
    let light = HomeCommand {
        device_id: "light-living-01".to_string(),
        action: "set_brightness".to_string(),
        params: serde_json::json!({ "brightness": 42 }),
        user_confirmed: false,
    };
    assert!(hub.execute_command(&light).is_ok());
    assert_eq!(
        hub.get_device("light-living-01").unwrap().state.brightness,
        Some(42)
    );

    // Actionneur critique (serrure) : refus sans confirmation, OK avec.
    let unlock_no = HomeCommand {
        device_id: "lock-front-door".to_string(),
        action: "unlock".to_string(),
        params: serde_json::Value::Null,
        user_confirmed: false,
    };
    assert!(hub.execute_command(&unlock_no).is_err());
    assert_eq!(
        hub.get_device("lock-front-door").unwrap().state.is_locked,
        Some(true)
    );

    let unlock_yes = HomeCommand {
        device_id: "lock-front-door".to_string(),
        action: "unlock".to_string(),
        params: serde_json::Value::Null,
        user_confirmed: true,
    };
    assert!(hub.execute_command(&unlock_yes).is_ok());
    assert_eq!(
        hub.get_device("lock-front-door").unwrap().state.is_locked,
        Some(false)
    );
}
