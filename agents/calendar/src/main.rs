use agent_calendar::CalendarManager;

fn main() {
    println!("Agent calendrier Hakille — agenda local initialisé.");
    let mgr = CalendarManager::new();
    println!(
        "  -> {} événement(s) présent(s) dans l'agenda local.",
        mgr.events().len()
    );
}
