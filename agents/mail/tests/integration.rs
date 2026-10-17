use agent_mail::{MailClient, MailFolder};

/// Intégration mail : dossier INBOX seedé + recherche plein-texte + marquage.
#[test]
fn test_mail_integration_folder_and_search() {
    let mut client = MailClient::new();

    // Dossier : INBOX seedé avec 2 messages de référence.
    let inbox = client.fetch_folder(&MailFolder::Inbox);
    assert_eq!(inbox.len(), 2, "INBOX seedé attendu");

    // Recherche : terme du seed + expéditeur.
    let contrat = client.search("contrat");
    assert_eq!(contrat.len(), 1);
    assert_eq!(contrat[0].id, "msg-101");

    let cluster = client.search("annora.io");
    assert_eq!(cluster.len(), 1);
    assert_eq!(cluster[0].id, "msg-102");

    // Recherche insensible à la casse + vide pour terme inconnu.
    assert_eq!(client.search("CONTRAT").len(), 1);
    assert!(client.search("zzz-introuvable").is_empty());

    // Marquage lu + réponse avec validation obligatoire.
    assert!(client.mark_read("msg-101"));
    assert!(client.get_message("msg-101").unwrap().is_read);
    let draft = client
        .create_reply("msg-101", "Vu, article 8 validé.")
        .unwrap();
    assert!(draft.requires_validation);
    assert!(client.send_message(&draft, false).is_err());
    assert!(client.send_message(&draft, true).is_ok());
    assert_eq!(client.fetch_folder(&MailFolder::Sent).len(), 1);
}
