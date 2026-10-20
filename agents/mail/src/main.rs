use agent_mail::MailClient;

fn main() {
    println!("Hakille Mail Agent initialized with scoped capabilities.");
    let client = MailClient::new();
    let inbox = client.fetch_folder(&agent_mail::MailFolder::Inbox);
    println!(
        "  -> {} e-mails disponibles dans la boîte de réception.",
        inbox.len()
    );
}
