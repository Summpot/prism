//! Pluggable zstd dictionaries for the native optimizer.
//!
//! Dictionary bytes are opaque to this module: they may come from a file, a
//! negotiated peer transfer, or an optional process-wide trainer. Nothing here
//! is protocol-specific.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use sha2::{Digest, Sha256};

/// Maximum dictionary payload accepted on the wire or from config (128 KiB).
pub const MAX_DICTIONARY_BYTES: usize = 128 * 1024;
/// Default trained-dictionary size.
pub const DEFAULT_TRAINED_DICT_SIZE: usize = 64 * 1024;
/// Samples collected before a process-wide trainer will fire.
pub const TRAINER_MIN_SAMPLES: usize = 32;
const TRAINER_MAX_SAMPLES: usize = 256;
const TRAINER_MAX_SAMPLE_BYTES: usize = 8 * 1024 * 1024;
const TRAINER_MAX_SAMPLE_LEN: usize = 64 * 1024;

/// Stable 32-bit id used in stream-parameter negotiation.
pub fn dictionary_id(bytes: &[u8]) -> u32 {
    if bytes.is_empty() {
        return 0;
    }
    let hash = Sha256::digest(bytes);
    u32::from_be_bytes(hash[0..4].try_into().unwrap())
}

/// Loads a dictionary from `path`. Empty files are treated as "no dictionary".
pub fn load_dictionary_file(path: &Path) -> io::Result<Option<Vec<u8>>> {
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() > MAX_DICTIONARY_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "zstd dictionary {} bytes exceeds max {MAX_DICTIONARY_BYTES}",
                bytes.len()
            ),
        ));
    }
    Ok(Some(bytes))
}

/// Trains a zstd dictionary from raw samples.
pub fn train_dictionary(samples: &[Vec<u8>], max_size: usize) -> io::Result<Vec<u8>> {
    let max_size = max_size.clamp(256, MAX_DICTIONARY_BYTES);
    if samples.len() < 2 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "dictionary training needs at least 2 samples",
        ));
    }
    zstd::dict::from_samples(samples, max_size)
}

/// Collects early-connection batches so a later connection can reuse a trained dict.
#[derive(Debug, Default)]
pub struct DictSampler {
    samples: Vec<Vec<u8>>,
    bytes: usize,
}

impl DictSampler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` when the sampler is still accepting data.
    pub fn wants_more(&self) -> bool {
        self.samples.len() < TRAINER_MAX_SAMPLES && self.bytes < TRAINER_MAX_SAMPLE_BYTES
    }

    pub fn push(&mut self, sample: &[u8]) {
        if sample.is_empty() || !self.wants_more() {
            return;
        }
        let take = sample.len().min(TRAINER_MAX_SAMPLE_LEN);
        self.bytes += take;
        self.samples.push(sample[..take].to_vec());
    }

    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Trains a dictionary when enough samples were collected.
    pub fn finish(self) -> Option<Vec<u8>> {
        self.try_train()
    }

    /// Trains a dictionary without consuming the sampler, so sampling can continue.
    pub fn try_train(&self) -> Option<Vec<u8>> {
        if self.samples.len() < TRAINER_MIN_SAMPLES {
            return None;
        }
        train_dictionary(&self.samples, DEFAULT_TRAINED_DICT_SIZE).ok()
    }
}

fn trained_cache() -> &'static Mutex<HashMap<String, Arc<[u8]>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn dictionary_dir() -> &'static Mutex<Option<std::path::PathBuf>> {
    static DIR: OnceLock<Mutex<Option<std::path::PathBuf>>> = OnceLock::new();
    DIR.get_or_init(|| Mutex::new(None))
}

/// Process-wide directory for trained dictionaries (`<workdir>/optimizer-dicts`).
pub fn set_dictionary_dir(dir: std::path::PathBuf) {
    if dir.as_os_str().is_empty() {
        return;
    }
    if let Ok(mut guard) = dictionary_dir().lock() {
        *guard = Some(dir);
    }
}

fn sanitize_dict_key(key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() || key == "." || key == ".." {
        return None;
    }
    let out: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() || out == "." || out == ".." {
        None
    } else {
        Some(out)
    }
}

fn persisted_dict_path(key: &str) -> Option<std::path::PathBuf> {
    let name = sanitize_dict_key(key)?;
    let dir = dictionary_dir().lock().ok()?.clone()?;
    Some(dir.join(format!("{name}.zdict")))
}

fn load_persisted_dictionary(key: &str) -> Option<Vec<u8>> {
    let path = persisted_dict_path(key)?;
    match load_dictionary_file(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => {
            tracing::warn!(
                path = %path.display(),
                err = %err,
                "optimizer: failed to load persisted dictionary"
            );
            None
        }
    }
}

fn persist_dictionary(key: &str, dict: &[u8]) {
    let Some(path) = persisted_dict_path(key) else {
        return;
    };
    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                path = %parent.display(),
                err = %err,
                "optimizer: failed to create dictionary dir"
            );
            return;
        }
    }
    if let Err(err) = std::fs::write(&path, dict) {
        tracing::warn!(
            path = %path.display(),
            err = %err,
            "optimizer: failed to persist dictionary"
        );
    }
}

/// Dictionary previously trained from connections of `key` (typically a service name).
pub fn cached_trained_dictionary(key: &str) -> Option<Arc<[u8]>> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    if let Some(cached) = trained_cache().lock().ok()?.get(key).cloned() {
        return Some(cached);
    }
    let bytes = load_persisted_dictionary(key)?;
    let arc: Arc<[u8]> = Arc::from(bytes);
    if let Ok(mut cache) = trained_cache().lock() {
        cache.insert(key.to_string(), arc.clone());
    }
    Some(arc)
}

/// Stores a trained dictionary for later connections of `key`.
pub fn store_trained_dictionary(key: &str, dict: Vec<u8>) {
    let key = key.trim();
    if key.is_empty() || dict.is_empty() || dict.len() > MAX_DICTIONARY_BYTES {
        return;
    }
    persist_dictionary(key, &dict);
    if let Ok(mut cache) = trained_cache().lock() {
        cache.insert(key.to_string(), Arc::from(dict));
    }
}

/// Resolves dictionary bytes from an optional file path, falling back to a
/// process-wide trained cache keyed by `cache_key`.
pub fn resolve_dictionary(path: Option<&str>, cache_key: &str) -> Option<Vec<u8>> {
    if let Some(path) = path.map(str::trim).filter(|s| !s.is_empty()) {
        match load_dictionary_file(Path::new(path)) {
            Ok(Some(bytes)) => return Some(bytes),
            Ok(None) => {}
            Err(err) => {
                tracing::warn!(path, err=%err, "optimizer: failed to load zstd dictionary file");
            }
        }
    }
    cached_trained_dictionary(cache_key).map(|d| d.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_id_is_stable_and_zero_for_empty() {
        assert_eq!(dictionary_id(b""), 0);
        let a = dictionary_id(b"hello-dict");
        let b = dictionary_id(b"hello-dict");
        let c = dictionary_id(b"other-dict");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, 0);
    }

    #[test]
    fn train_dictionary_from_repetitive_samples() {
        let sample = b"entity_id:1;pos:0,64,0;block:stone;block:dirt;block:stone;".to_vec();
        let samples: Vec<Vec<u8>> = (0..40).map(|_| sample.clone()).collect();
        let dict = train_dictionary(&samples, 1024).expect("train");
        assert!(!dict.is_empty());
        assert!(dict.len() <= 1024);
        assert_ne!(dictionary_id(&dict), 0);
    }

    #[test]
    fn sampler_trains_after_enough_samples() {
        let mut sampler = DictSampler::new();
        let sample = vec![0x11u8; 64];
        for _ in 0..TRAINER_MIN_SAMPLES {
            sampler.push(&sample);
        }
        let dict = sampler.finish().expect("trained");
        store_trained_dictionary("svc-a", dict.clone());
        assert_eq!(
            cached_trained_dictionary("svc-a").as_deref(),
            Some(dict.as_slice())
        );
    }

    #[test]
    fn persisted_dictionary_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "prism-dict-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        set_dictionary_dir(dir.clone());
        let dict = b"trained-dict-bytes-for-persist".to_vec();
        store_trained_dictionary("game.svc", dict.clone());
        trained_cache().lock().unwrap().remove("game.svc");
        let loaded = cached_trained_dictionary("game.svc").expect("loaded from disk");
        assert_eq!(loaded.as_ref(), dict.as_slice());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
