pub mod cluster;
pub mod edge;

use crate::triplet::{TriplePattern, Triplet};
use annora_core::TripletId;
use async_trait::async_trait;

#[async_trait]
pub trait GraphStorage: Send + Sync {
    async fn store(&self, triplet: &Triplet) -> annora_core::Result<()>;
    async fn load(&self, id: &TripletId) -> annora_core::Result<Option<Triplet>>;
    async fn query(&self, pattern: &TriplePattern) -> annora_core::Result<Vec<Triplet>>;
    async fn delete(&self, id: &TripletId) -> annora_core::Result<()>;
    async fn all_triplets(&self) -> annora_core::Result<Vec<Triplet>>;

    /// Stocke un lot de triplets.
    ///
    /// Implémentation par défaut : boucle sur `store()` (simple, NON
    /// atomique — un échec laisse les écritures précédentes en place).
    /// `EdgeStore` surcharge avec UNE seule transaction redb (atomique :
    /// tout ou rien) ; `ClusterStore` surcharge avec un seul verrou
    /// d'écriture. Le passage par `Vec` (plutôt que `&[Triplet]`) fige
    /// le lot côté appelant avant l'envoi au backend.
    async fn batch_store(&self, triplets: Vec<Triplet>) -> annora_core::Result<()> {
        for t in &triplets {
            self.store(t).await?;
        }
        Ok(())
    }

    /// Nombre de triplets persistés.
    ///
    /// Implémentation par défaut : `all_triplets().len()` (O(N) honnête).
    /// `EdgeStore` surcharge via `ReadableTable::len()` (O(1)).
    async fn count(&self) -> annora_core::Result<usize> {
        Ok(self.all_triplets().await?.len())
    }

    /// Force la durabilité (fsync / compaction selon backend).
    ///
    /// Implémentation par défaut : no-op. Les backends dont les `commit`
    /// sont déjà durables (redb) ou purement volatils (mock `ClusterStore`)
    /// n'ont rien à faire ; c'est documenté dans chaque implémentation.
    async fn flush(&self) -> annora_core::Result<()> {
        Ok(())
    }
}
