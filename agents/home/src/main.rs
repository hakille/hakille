use agent_home::{HomeCommand, SmartHomeHub};

fn main() {
    println!("Hakille Home Automation Agent initialized with physical safety safeguards.");
    let mut hub = SmartHomeHub::new();
    let devices = hub.discover_devices();
    println!(
        "  -> {} appareils domotiques découverts sur le réseau local.",
        devices.len()
    );

    let cmd = HomeCommand {
        device_id: "light-living-01".to_string(),
        action: "turn_on".to_string(),
        params: serde_json::Value::Null,
        user_confirmed: false,
    };
    match hub.execute_command(&cmd) {
        Ok(msg) => println!("  -> Exécution: {}", msg),
        Err(e) => eprintln!("  -> Erreur: {}", e),
    }
}
