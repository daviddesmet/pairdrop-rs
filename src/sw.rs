//! Content-addressed service worker.
//!
//! PairDrop's service worker is cache-first with no revalidation, and `activate` only drops caches
//! whose name differs from the current one. Upstream hardcodes that name as the release version,
//! so a cache populated from stale HTTP responses stays stale until someone clears site data by
//! hand -- the failure behind schlagmichdoch/PairDrop#384.
//!
//! Deriving the name from a digest of `public/` instead means any asset change renames the cache,
//! `activate` deletes the old one, and everything is re-fetched. Skew cannot outlive a deploy.

use std::path::Path;

use sha2::{Digest, Sha256};

const VERSION_DECLARATION: &str = "const cacheVersion = ";

pub struct ServiceWorker {
    pub body: String,
    pub version: String,
}

impl ServiceWorker {
    /// Returns `None` when `public/service-worker.js` is missing, in which case the static file
    /// handler serves whatever is there unchanged.
    pub fn build(public_dir: &Path) -> Option<Self> {
        let path = public_dir.join("service-worker.js");
        let source = std::fs::read_to_string(&path).ok()?;

        let version = format!("sha-{}", digest(public_dir, &path));

        let body = match replace_version(&source, &version) {
            Some(body) => body,
            None => {
                tracing::warn!(
                    "service-worker.js has no `{VERSION_DECLARATION}` line; serving it unmodified"
                );
                source
            }
        };

        Some(ServiceWorker { body, version })
    }
}

/// Rewrites the first `const cacheVersion = '...';` to carry the digest.
fn replace_version(source: &str, version: &str) -> Option<String> {
    let start = source.find(VERSION_DECLARATION)?;
    let value_start = start + VERSION_DECLARATION.len();
    let end = source[value_start..].find(';')? + value_start;

    Some(format!("{}'{version}'{}", &source[..value_start], &source[end..]))
}

/// SHA-256 over every file under `public/` except the service worker itself, which would
/// otherwise depend on its own output.
fn digest(public_dir: &Path, exclude: &Path) -> String {
    let mut files = Vec::new();
    collect(public_dir, &mut files);
    files.retain(|path| path != exclude);
    files.sort();

    let mut hasher = Sha256::new();
    for path in &files {
        if let Ok(relative) = path.strip_prefix(public_dir) {
            hasher.update(relative.to_string_lossy().as_bytes());
        }
        match std::fs::read(path) {
            Ok(bytes) => hasher.update(&bytes),
            Err(err) => tracing::warn!(?path, %err, "could not hash asset"),
        }
    }

    hex::encode(&hasher.finalize()[..8])
}

fn collect(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::replace_version;

    #[test]
    fn rewrites_the_version_and_leaves_the_rest_alone() {
        let source = "const cacheVersion = 'v1.11.2';\nconst cacheTitle = `pairdrop-${x}`;\n";
        let rewritten = replace_version(source, "sha-9f3c1a7e").unwrap();

        assert!(rewritten.starts_with("const cacheVersion = 'sha-9f3c1a7e';"));
        assert!(rewritten.ends_with("const cacheTitle = `pairdrop-${x}`;\n"));
    }

    #[test]
    fn leaves_a_worker_without_the_declaration_alone() {
        assert!(replace_version("self.addEventListener('fetch', () => {});", "sha-1").is_none());
    }
}
