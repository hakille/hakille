use agent_calendar::CalendarManager;

fn main() {
    println!("Annora Calendar Agent initialized with local graph synchronization.");
    let mgr = CalendarManager::new();
    println!(
        "  -> {} événements synchronisés dans l'agenda souverain.",
        mgr.events().len()
    );
}
