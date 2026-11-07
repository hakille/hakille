//! Persistance de l'outbox : remplacement atomique, erreurs explicites et
//! publication en mémoire seulement lorsque le fichier a été remplacé.

use hakille_sync::Outbox;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    #[error("outbox non écrite : {0}")]
    NotCommitted(#[source] io::Error),
    #[error("outbox remplacée, mais durabilité après coupure non confirmée : {0}")]
    CommittedNotSynced(#[source] io::Error),
}

impl PersistError {
    fn committed(&self) -> bool {
        matches!(self, Self::CommittedNotSynced(_))
    }
}

pub fn outbox_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("outbox.json")
}

/// Seule l'absence du fichier signifie une file vide. Une corruption ou une
/// erreur d'accès arrête le chargement et laisse les octets d'origine intacts.
pub fn load_outbox(data_dir: &Path) -> io::Result<Outbox> {
    Outbox::load_from_file(outbox_file_path(data_dir))
}

pub fn copy_outbox(outbox: &Outbox) -> Outbox {
    let mut copy = Outbox::new();
    for entry in outbox.list() {
        copy.enqueue(entry.clone());
    }
    copy
}

fn sync_directory(data_dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(data_dir)?.sync_all()
    }
    // Windows ne permet pas File::open sur un dossier : le fichier est
    // synchronisé avant remplacement. Aucun fsync de dossier n'est promis.
    #[cfg(not(unix))]
    {
        let _ = data_dir;
        Ok(())
    }
}

fn save_with_sync(
    data_dir: &Path,
    outbox: &Outbox,
    sync: impl FnOnce(&Path) -> io::Result<()>,
) -> Result<(), PersistError> {
    let json = serde_json::to_vec_pretty(outbox.list())
        .map_err(|e| PersistError::NotCommitted(io::Error::new(io::ErrorKind::InvalidData, e)))?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(data_dir).map_err(PersistError::NotCommitted)?;
    temporary
        .write_all(&json)
        .map_err(PersistError::NotCommitted)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(PersistError::NotCommitted)?;
    // Même dossier : aucune suppression préalable du fichier précédent.
    temporary
        .persist(outbox_file_path(data_dir))
        .map_err(|e| PersistError::NotCommitted(e.error))?;
    sync(data_dir).map_err(PersistError::CommittedNotSynced)
}

pub fn save_outbox(data_dir: &Path, outbox: &Outbox) -> Result<(), PersistError> {
    save_with_sync(data_dir, outbox, sync_directory)
}

/// Remplace le snapshot RAM avec le snapshot effectivement publié sur disque.
/// Une erreur après renommage reste une erreur, mais ne masque pas son effet.
fn replace_with_save(
    outbox: &mut Outbox,
    next: Outbox,
    persist: impl FnOnce(&Outbox) -> Result<(), PersistError>,
) -> Result<(), PersistError> {
    let result = persist(&next);
    if result.is_ok() || result.as_ref().is_err_and(|err| err.committed()) {
        *outbox = next;
    }
    result
}

pub fn replace_outbox(
    data_dir: &Path,
    outbox: &mut Outbox,
    next: Outbox,
) -> Result<(), PersistError> {
    replace_with_save(outbox, next, |snapshot| save_outbox(data_dir, snapshot))
}

pub fn update_outbox<T>(
    data_dir: &Path,
    outbox: &mut Outbox,
    update: impl FnOnce(&mut Outbox) -> T,
) -> Result<T, PersistError> {
    let mut next = copy_outbox(outbox);
    let result = update(&mut next);
    replace_outbox(data_dir, outbox, next)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hakille_sync::OutboxEntry;

    #[test]
    fn corrupt_snapshot_is_not_replaced_by_an_empty_queue() {
        let dir = tempfile::tempdir().unwrap();
        let path = outbox_file_path(dir.path());
        std::fs::write(&path, b"[{corrupt").unwrap();
        assert_eq!(
            load_outbox(dir.path()).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(std::fs::read(path).unwrap(), b"[{corrupt");
    }

    #[test]
    fn missing_and_inaccessible_snapshots_are_distinct() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_outbox(dir.path()).unwrap().is_empty());
        std::fs::create_dir(outbox_file_path(dir.path())).unwrap();
        assert!(load_outbox(dir.path()).is_err());
    }

    #[test]
    fn enqueue_and_revalidate_survive_reload() {
        let dir = tempfile::tempdir().unwrap();
        let mut outbox = Outbox::new();
        assert!(
            update_outbox(dir.path(), &mut outbox, |q| q
                .enqueue(OutboxEntry::new("Envoyer", "message", "k")))
            .unwrap()
        );
        assert!(update_outbox(dir.path(), &mut outbox, |q| q.revalidate("k")).unwrap());
        assert_eq!(load_outbox(dir.path()).unwrap().list(), outbox.list());
        assert!(outbox.list()[0].revalidated);
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
            1,
            "aucun temporaire résiduel"
        );
    }

    #[test]
    fn failed_replace_keeps_the_previous_memory_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = outbox_file_path(dir.path());
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("sentinel"), b"intact").unwrap();
        let mut outbox = Outbox::new();
        let error = update_outbox(dir.path(), &mut outbox, |q| {
            q.enqueue(OutboxEntry::new("Envoyer", "message", "k"))
        })
        .unwrap_err();
        assert!(!error.committed());
        assert!(outbox.is_empty());
        assert_eq!(std::fs::read(path.join("sentinel")).unwrap(), b"intact");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn directory_sync_error_reports_that_replacement_already_happened() {
        let dir = tempfile::tempdir().unwrap();
        let mut current = Outbox::new();
        let mut next = Outbox::new();
        next.enqueue(OutboxEntry::new("Envoyer", "message", "k"));
        let err = replace_with_save(&mut current, next, |snapshot| {
            save_with_sync(dir.path(), snapshot, |_| {
                Err(io::Error::other("sync refusé"))
            })
        })
        .unwrap_err();
        assert!(err.committed());
        assert_eq!(
            current.len(),
            1,
            "la RAM ne masque pas le remplacement déjà réalisé"
        );
        assert_eq!(load_outbox(dir.path()).unwrap().list(), current.list());
    }
}
