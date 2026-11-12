use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use super::{Archive, Result};

/// Archives currently open, keyed by volume id. Opened on first use.
#[derive(Default)]
pub struct ArchiveRegistry {
    open: RwLock<HashMap<String, Arc<Archive>>>,
}

impl ArchiveRegistry {
    /// Forgets a volume's open archive (its file handle closes once unused).
    pub fn close(&self, volume_id: &str) {
        self.open
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(volume_id);
    }

    /// Returns the open archive for this volume, opening it if needed.
    pub fn get_or_open(&self, volume_id: &str, path: &Path) -> Result<Arc<Archive>> {
        if let Some(archive) = self
            .open
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(volume_id)
        {
            return Ok(archive.clone());
        }
        let archive = Arc::new(Archive::open(path)?);
        Ok(self
            .open
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .entry(volume_id.to_string())
            .or_insert(archive)
            .clone())
    }
}
