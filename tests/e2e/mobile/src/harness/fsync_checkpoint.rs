//! Atomic disk persistence and crash recovery test harness for Mobile Checkpoint v2.
//!
//! Validates `draft_json()` persistence via `sync_all()` prior to dispatch,
//! simulates power cut / process termination, injects corruption, and verifies
//! that unknown operations enter `recovery_required` mode.

use hakille_mobile::MobileBridge;
use std::path::PathBuf;

pub struct CheckpointHarness {
    pub temp_dir: tempfile::TempDir,
}

impl CheckpointHarness {
    /// Creates a new isolated checkpoint harness with its own temporary directory.
    pub fn new() -> std::io::Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        Ok(Self { temp_dir })
    }

    /// The durable path to the checkpoint file on disk.
    pub fn checkpoint_path(&self) -> PathBuf {
        self.temp_dir.path().join("checkpoint_v2.json")
    }

    /// Writes content to disk with atomic rename and `sync_all()` guarantees.
    pub fn write_atomic_fsync(&self, content: &str) -> std::io::Result<()> {
        let target = self.checkpoint_path();
        let tmp_path = self
            .temp_dir
            .path()
            .join(format!(".checkpoint-{}.tmp", uuid::Uuid::new_v4()));

        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_path)?;
            use std::io::Write;
            file.write_all(content.as_bytes())?;
            file.flush()?;
            file.sync_all()?;
        }

        std::fs::rename(&tmp_path, &target)?;

        #[cfg(unix)]
        {
            let dir = std::fs::File::open(self.temp_dir.path())?;
            dir.sync_all()?;
        }

        Ok(())
    }

    /// Reads the persisted checkpoint content from disk.
    pub fn read_checkpoint(&self) -> std::io::Result<String> {
        std::fs::read_to_string(self.checkpoint_path())
    }

    /// Captures `draft_json()` from the bridge and fsyncs it to disk.
    pub fn save_draft_from_bridge(&self, bridge: &MobileBridge) -> std::io::Result<String> {
        let draft_json = bridge.draft_json();
        if draft_json == "null" {
            return Err(std::io::Error::other("bridge returned null draft_json"));
        }
        self.write_atomic_fsync(&draft_json)?;
        Ok(draft_json)
    }

    /// Restores the persisted on-disk checkpoint into a bridge.
    pub fn restore_into_bridge(&self, bridge: &MobileBridge) -> Result<serde_json::Value, String> {
        let content = self.read_checkpoint().map_err(|e| e.to_string())?;
        let res_str = bridge.restore_draft_json(content);
        serde_json::from_str(&res_str).map_err(|e| e.to_string())
    }

    /// Corrupts the raw checkpoint bytes on disk.
    pub fn corrupt_checkpoint_bytes(
        &self,
        modifier: impl FnOnce(&mut Vec<u8>),
    ) -> std::io::Result<()> {
        let target = self.checkpoint_path();
        let mut bytes = std::fs::read(&target)?;
        modifier(&mut bytes);
        let tmp_path = self
            .temp_dir
            .path()
            .join(format!(".corrupt-{}.tmp", uuid::Uuid::new_v4()));
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp_path)?;
            use std::io::Write;
            file.write_all(&bytes)?;
            file.flush()?;
            file.sync_all()?;
        }
        std::fs::rename(&tmp_path, &target)?;
        Ok(())
    }

    /// Corrupts the parsed JSON checkpoint on disk and rewrites it.
    pub fn corrupt_checkpoint_json(
        &self,
        modifier: impl FnOnce(&mut serde_json::Value),
    ) -> std::io::Result<()> {
        let content = self.read_checkpoint()?;
        let mut val: serde_json::Value =
            serde_json::from_str(&content).map_err(std::io::Error::other)?;
        modifier(&mut val);
        self.write_atomic_fsync(&serde_json::to_string(&val).map_err(std::io::Error::other)?)
    }

    /// Truncates the checkpoint file to `bytes_len` bytes.
    pub fn truncate_checkpoint(&self, bytes_len: usize) -> std::io::Result<()> {
        let target = self.checkpoint_path();
        let bytes = std::fs::read(&target)?;
        let truncated = &bytes[..bytes_len.min(bytes.len())];
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&target)?;
        use std::io::Write;
        file.write_all(truncated)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }

    /// Validates Checkpoint v2 JSON schema strictly according to spec.
    pub fn validate_checkpoint_schema(json_str: &str) -> Result<serde_json::Value, String> {
        let val: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| format!("invalid JSON: {e}"))?;

        let schema_version = val
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| "missing or invalid schema_version".to_string())?;

        if schema_version != 2 {
            return Err(format!("expected schema_version 2, got {schema_version}"));
        }

        if !val.get("draft").is_some_and(|v| v.is_string()) {
            return Err("missing or non-string 'draft' field".into());
        }

        if !val.get("next_request_id").is_some_and(|v| v.is_u64()) {
            return Err("missing or non-u64 'next_request_id' field".into());
        }

        if !val
            .get("uncertain_operations")
            .is_some_and(|v| v.is_array())
        {
            return Err("missing or non-array 'uncertain_operations' field".into());
        }

        if !val
            .get("acknowledged_operations")
            .is_some_and(|v| v.is_array())
        {
            return Err("missing or non-array 'acknowledged_operations' field".into());
        }

        Ok(val)
    }

    /// Asserts that `recovery_required` in the snapshot matches the expected boolean.
    pub fn assert_recovery_required(snapshot: &serde_json::Value, expected: bool) {
        assert_eq!(
            snapshot["recovery_required"], expected,
            "expected recovery_required == {expected}, got snapshot: {snapshot}"
        );
    }

    /// Asserts that an uncertain operation is present with matching ID, kind, and phase.
    pub fn assert_uncertain_operation(
        snapshot: &serde_json::Value,
        expected_id: u64,
        expected_kind: &str,
        expected_phase: &str,
    ) {
        let uncertain = snapshot["uncertain_operations"]
            .as_array()
            .expect("uncertain_operations array in snapshot");

        let found = uncertain.iter().any(|op| {
            op["request_id"].as_u64() == Some(expected_id)
                && op["kind"].as_str() == Some(expected_kind)
                && op["phase"].as_str() == Some(expected_phase)
        });

        assert!(
            found,
            "uncertain operation not found: id={expected_id}, kind={expected_kind}, phase={expected_phase} in {uncertain:?}"
        );
    }

    /// Asserts that an acknowledged operation is present in the snapshot.
    pub fn assert_acknowledged_operation(
        snapshot: &serde_json::Value,
        expected_id: u64,
        expected_kind: &str,
    ) {
        let acknowledged = snapshot["acknowledged_operations"]
            .as_array()
            .expect("acknowledged_operations array in snapshot");

        let found = acknowledged.iter().any(|op| {
            op["request_id"].as_u64() == Some(expected_id)
                && op["kind"].as_str() == Some(expected_kind)
        });

        assert!(
            found,
            "acknowledged operation not found: id={expected_id}, kind={expected_kind} in {acknowledged:?}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_schema_validation_rejects_missing_fields() {
        // Missing next_request_id
        let bad = r#"{"schema_version": 2, "draft": "hello", "uncertain_operations": [], "acknowledged_operations": []}"#;
        assert!(CheckpointHarness::validate_checkpoint_schema(bad).is_err());

        // Incompatible schema version
        let v1 = r#"{"schema_version": 1, "draft": "hello", "next_request_id": 1, "uncertain_operations": [], "acknowledged_operations": []}"#;
        assert!(CheckpointHarness::validate_checkpoint_schema(v1).is_err());
    }

    #[test]
    fn test_checkpoint_atomic_fsync_and_truncate() {
        let harness = CheckpointHarness::new().unwrap();
        let valid = r#"{"schema_version": 2, "draft": "test", "next_request_id": 5, "uncertain_operations": [], "acknowledged_operations": []}"#;

        harness.write_atomic_fsync(valid).unwrap();
        let read = harness.read_checkpoint().unwrap();
        assert_eq!(read, valid);

        harness.truncate_checkpoint(10).unwrap();
        let truncated = harness.read_checkpoint().unwrap();
        assert_eq!(truncated.len(), 10);
        assert!(CheckpointHarness::validate_checkpoint_schema(&truncated).is_err());
    }
}
