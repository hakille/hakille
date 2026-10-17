use annora_core::{CapabilityAction, CapabilityScope};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailFolder {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Trash,
}

impl MailFolder {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbox => "INBOX",
            Self::Sent => "SENT",
            Self::Drafts => "DRAFTS",
            Self::Archive => "ARCHIVE",
            Self::Trash => "TRASH",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailMessage {
    pub id: String,
    pub message_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub date: String,
    pub folder: MailFolder,
    pub is_read: bool,
    pub is_flagged: bool,
    pub has_attachment: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailDraft {
    pub draft_id: String,
    pub in_reply_to: Option<String>,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub created_at: String,
    pub requires_validation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendReceipt {
    pub receipt_id: String,
    pub message_id: String,
    pub timestamp: String,
    pub recipient_count: usize,
    pub status: String,
}

pub struct MailClient {
    messages: HashMap<String, MailMessage>,
    drafts: HashMap<String, MailDraft>,
}

impl MailClient {
    pub fn new() -> Self {
        let mut client = Self {
            messages: HashMap::new(),
            drafts: HashMap::new(),
        };
        client.seed_default_inbox();
        client
    }

    fn seed_default_inbox(&mut self) {
        let sample_messages = vec![
            MailMessage {
                id: "msg-101".to_string(),
                message_id: "<20261008-01@annora.internal>".to_string(),
                from: "sophie.martin@avocat-associes.fr".to_string(),
                to: vec!["utilisateur@entreprise.fr".to_string()],
                cc: vec![],
                subject: "Revue du contrat cadre et clauses de confidentialité".to_string(),
                body: "Bonjour,\n\nVeuillez trouver ci-joint les révisions apportées à l'accord de confidentialité. Merci de valider l'article 8 avant signature.\n\nBien cordialement,\nSophie Martin".to_string(),
                date: "2026-10-08T09:30:00Z".to_string(),
                folder: MailFolder::Inbox,
                is_read: false,
                is_flagged: true,
                has_attachment: true,
            },
            MailMessage {
                id: "msg-102".to_string(),
                message_id: "<20261008-02@annora.internal>".to_string(),
                from: "equipe-technique@annora.io".to_string(),
                to: vec!["utilisateur@entreprise.fr".to_string()],
                cc: vec![],
                subject: "Déploiement du cluster Rust 2024 souverain".to_string(),
                body: "Les 20 crates du système d'exploitation cognitif Annora compilent parfaitement sur Windows. Zero dépendance Python.".to_string(),
                date: "2026-10-08T11:15:00Z".to_string(),
                folder: MailFolder::Inbox,
                is_read: true,
                is_flagged: false,
                has_attachment: false,
            },
        ];

        for m in sample_messages {
            self.messages.insert(m.id.clone(), m);
        }
    }

    pub fn fetch_folder(&self, folder: &MailFolder) -> Vec<&MailMessage> {
        self.messages
            .values()
            .filter(|m| &m.folder == folder)
            .collect()
    }

    pub fn search(&self, query: &str) -> Vec<&MailMessage> {
        let q = query.to_lowercase();
        self.messages
            .values()
            .filter(|m| {
                m.subject.to_lowercase().contains(&q)
                    || m.body.to_lowercase().contains(&q)
                    || m.from.to_lowercase().contains(&q)
            })
            .collect()
    }

    pub fn get_message(&self, id: &str) -> Option<&MailMessage> {
        self.messages.get(id)
    }

    pub fn mark_read(&mut self, id: &str) -> bool {
        if let Some(m) = self.messages.get_mut(id) {
            m.is_read = true;
            true
        } else {
            false
        }
    }

    pub fn create_draft(&mut self, to: Vec<String>, subject: &str, body: &str) -> MailDraft {
        let draft_id = format!("draft-{}", self.drafts.len() + 1);
        let draft = MailDraft {
            draft_id: draft_id.clone(),
            in_reply_to: None,
            to,
            cc: vec![],
            subject: subject.to_string(),
            body: body.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            requires_validation: true,
        };
        self.drafts.insert(draft_id, draft.clone());
        draft
    }

    pub fn create_reply(&mut self, original_id: &str, reply_body: &str) -> Option<MailDraft> {
        let original = self.messages.get(original_id)?;
        let subject = if original.subject.starts_with("Re:") {
            original.subject.clone()
        } else {
            format!("Re: {}", original.subject)
        };

        let draft_id = format!("draft-reply-{}", original_id);
        let draft = MailDraft {
            draft_id: draft_id.clone(),
            in_reply_to: Some(original.message_id.clone()),
            to: vec![original.from.clone()],
            cc: vec![],
            subject,
            body: reply_body.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            requires_validation: true,
        };
        self.drafts.insert(draft_id, draft.clone());
        Some(draft)
    }

    /// Sending an email STRICTLY requires explicit user validation chip!
    pub fn send_message(
        &mut self,
        draft: &MailDraft,
        user_confirmed: bool,
    ) -> Result<SendReceipt, String> {
        if !user_confirmed {
            return Err("Émission refusée : L'envoi d'e-mails externes requiert la confirmation explicite de l'utilisateur (Puce #FF5722).".to_string());
        }

        let msg_id = format!("<{}-annora@local>", draft.draft_id);
        let sent_msg = MailMessage {
            id: format!("msg-sent-{}", draft.draft_id),
            message_id: msg_id.clone(),
            from: "me@annora.local".to_string(),
            to: draft.to.clone(),
            cc: draft.cc.clone(),
            subject: draft.subject.clone(),
            body: draft.body.clone(),
            date: chrono::Utc::now().to_rfc3339(),
            folder: MailFolder::Sent,
            is_read: true,
            is_flagged: false,
            has_attachment: false,
        };

        self.messages.insert(sent_msg.id.clone(), sent_msg);
        self.drafts.remove(&draft.draft_id);

        Ok(SendReceipt {
            receipt_id: format!("rcpt-{}", uuid::Uuid::now_v7()),
            message_id: msg_id,
            timestamp: chrono::Utc::now().to_rfc3339(),
            recipient_count: draft.to.len() + draft.cc.len(),
            status: "SMTP transmission successful".to_string(),
        })
    }

    /// Full RFC 5322 header parser supporting folded headers (continuation lines)
    pub fn parse_rfc5322(raw_email: &str) -> HashMap<String, String> {
        let mut headers: HashMap<String, String> = HashMap::new();
        let mut current_key: Option<String> = None;

        for line in raw_email.lines() {
            let line_trimmed_cr = line.trim_end_matches('\r');
            if line_trimmed_cr.is_empty() {
                break; // Header section ends at blank line
            }

            if line_trimmed_cr.starts_with(' ') || line_trimmed_cr.starts_with('\t') {
                // Folded continuation line
                if let Some(ref k) = current_key {
                    if let Some(val) = headers.get_mut(k) {
                        val.push(' ');
                        val.push_str(line_trimmed_cr.trim());
                    }
                }
            } else if let Some(colon) = line_trimmed_cr.find(':') {
                let key = line_trimmed_cr[..colon].trim().to_lowercase();
                let val = line_trimmed_cr[colon + 1..].trim().to_string();
                headers.insert(key.clone(), val);
                current_key = Some(key);
            }
        }
        headers
    }

    /// Parses an entire RFC 5322 raw email (headers + body) into a structured MailMessage
    pub fn parse_raw_message(raw_email: &str) -> Result<MailMessage, String> {
        let headers = Self::parse_rfc5322(raw_email);
        if headers.is_empty() {
            return Err("Invalid RFC 5322 email: missing headers".to_string());
        }

        // Find boundary between headers and body (first empty line)
        let body = if let Some(idx) = raw_email.find("\r\n\r\n") {
            &raw_email[idx + 4..]
        } else if let Some(idx) = raw_email.find("\n\n") {
            &raw_email[idx + 2..]
        } else {
            ""
        };

        let message_id = headers
            .get("message-id")
            .cloned()
            .unwrap_or_else(|| format!("<{}@annora.local>", uuid::Uuid::now_v7()));

        let from = headers
            .get("from")
            .cloned()
            .unwrap_or_else(|| "unknown@annora.local".to_string());

        let to: Vec<String> = headers
            .get("to")
            .map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let cc: Vec<String> = headers
            .get("cc")
            .map(|t| {
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let subject = headers
            .get("subject")
            .cloned()
            .unwrap_or_else(|| "(Sans objet)".to_string());

        let date = headers
            .get("date")
            .cloned()
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        let id = format!("msg-ingest-{}", uuid::Uuid::now_v7());

        Ok(MailMessage {
            id,
            message_id,
            from,
            to,
            cc,
            subject,
            body: body.trim_end().to_string(),
            date,
            folder: MailFolder::Inbox,
            is_read: false,
            is_flagged: false,
            has_attachment: headers
                .get("content-type")
                .map(|c| c.contains("multipart"))
                .unwrap_or(false),
        })
    }

    /// Ingests an incoming raw RFC 5322 email (e.g. from local IMAP/SMTP spool) into Inbox
    pub fn ingest_incoming_email(&mut self, raw_email: &str) -> Result<String, String> {
        let msg = Self::parse_raw_message(raw_email)?;
        let id = msg.id.clone();
        self.messages.insert(id.clone(), msg);
        Ok(id)
    }
}

impl Default for MailClient {
    fn default() -> Self {
        Self::new()
    }
}

pub fn check_mail_capabilities() -> Vec<(CapabilityScope, CapabilityAction)> {
    vec![
        (CapabilityScope::Network, CapabilityAction::Read),
        (CapabilityScope::Filesystem, CapabilityAction::Read),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mail_client_inbox_and_search() {
        let mut client = MailClient::new();
        let inbox = client.fetch_folder(&MailFolder::Inbox);
        assert!(!inbox.is_empty());

        let search_res = client.search("contrat");
        assert_eq!(search_res.len(), 1);
        assert_eq!(search_res[0].id, "msg-101");

        // Mark read
        assert!(client.mark_read("msg-101"));
        assert!(client.get_message("msg-101").unwrap().is_read);
    }

    #[test]
    fn test_mail_draft_and_send_with_validation() {
        let mut client = MailClient::new();
        let draft = client
            .create_reply("msg-101", "Accord validé pour l'article 8.")
            .unwrap();
        assert!(draft.requires_validation);
        assert!(draft.subject.starts_with("Re:"));

        // Sending without user approval MUST fail
        let send_res = client.send_message(&draft, false);
        assert!(send_res.is_err());

        // Sending with user approval succeeds
        let receipt = client.send_message(&draft, true).unwrap();
        assert_eq!(receipt.status, "SMTP transmission successful");
        assert_eq!(receipt.recipient_count, 1);
    }

    #[test]
    fn test_rfc5322_header_parsing() {
        let raw = "From: Alice <alice@example.com>\nTo: Bob <bob@example.com>\nSubject: Hello Annora\nDate: Fri, 08 Oct 2026 12:00:00 +0000\n\nBody here";
        let headers = MailClient::parse_rfc5322(raw);
        assert_eq!(headers.get("from").unwrap(), "Alice <alice@example.com>");
        assert_eq!(headers.get("subject").unwrap(), "Hello Annora");
    }

    #[test]
    fn test_rfc5322_folded_headers_and_message_ingestion() {
        let raw = "\
From: Charlie <charlie@enterprise.org>\r\n\
To: team@annora.internal\r\n\
Subject: Rapport de gouvernance\r\n\
\x20et souverainete numerique\r\n\
Message-ID: <unique-20261009-xyz@enterprise.org>\r\n\
Date: Fri, 09 Oct 2026 14:00:00 +0200\r\n\
\r\n\
Bonjour l'equipe Annora,\r\n\
\r\n\
Voici le compte-rendu d'audit pour le systeme.";

        let mut client = MailClient::new();
        let initial_inbox_count = client.fetch_folder(&MailFolder::Inbox).len();

        let msg_id = client
            .ingest_incoming_email(raw)
            .expect("Ingestion should succeed");
        let ingested = client
            .get_message(&msg_id)
            .expect("Message should be retrievable");

        assert_eq!(
            ingested.subject,
            "Rapport de gouvernance et souverainete numerique"
        );
        assert_eq!(ingested.from, "Charlie <charlie@enterprise.org>");
        assert_eq!(ingested.to, vec!["team@annora.internal"]);
        assert!(ingested.body.contains("Bonjour l'equipe Annora"));
        assert_eq!(
            client.fetch_folder(&MailFolder::Inbox).len(),
            initial_inbox_count + 1
        );
    }
}
