use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::docs::read_text_allow_missing;
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeSnapshot {
    pub files: BTreeMap<String, String>,
    pub hash: String,
}

pub fn snapshot_scope(repo_root: &Path, files: &[String]) -> Result<ScopeSnapshot> {
    let mut map = BTreeMap::new();
    for file in files {
        let content = read_text_allow_missing(&repo_root.join(file))?;
        map.insert(file.clone(), content);
    }

    let hash = snapshot_hash(&map);
    Ok(ScopeSnapshot { files: map, hash })
}

fn snapshot_hash(files: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (path, content) in files {
        hasher.update(path.as_bytes());
        hasher.update(content.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}
