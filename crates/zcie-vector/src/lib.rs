//! Local semantic search (ADR-011): model2vec static embeddings and an in-memory vector index.
//! The model runs on the CPU in microseconds per text; no network and no paid model is used.

pub mod error;
pub mod index;
pub mod model;
pub mod text;
pub mod tokenize;
pub mod unigram;

use std::path::Path;
use std::sync::Arc;

pub use error::{Error, Result};
pub use index::{FlatIndex, VectorHit, VectorIndex};
pub use model::StaticModel;
pub use text::{Entity, entity_text, split_identifiers};

/// The default model folder name under `<home>/models/`. It is multilingual, so that a Turkish
/// question finds English code and an English question finds a Turkish decision (ADR-013).
pub const DEFAULT_MODEL: &str = "potion-multilingual-128M";
/// The models that `zcie vector install` knows, best first.
pub const KNOWN_MODELS: [&str; 2] = [DEFAULT_MODEL, "potion-base-8M"];

/// Returns the model folder to load: the configured model, or for the default model that is not
/// installed, the first installed known model. `None` if no model is installed.
pub fn model_folder(models: &Path, configured: &str) -> Option<std::path::PathBuf> {
    let installed = |name: &str| Some(models.join(name)).filter(|f| f.join("model.safetensors").exists());
    if configured != DEFAULT_MODEL {
        return installed(configured);
    }
    KNOWN_MODELS.iter().find_map(|name| installed(name))
}

/// Returns the lowest useful cosine similarity of a model. The multilingual model gives lower
/// similarities (related texts 0.2 to 0.4, unrelated texts mostly below 0.2) than the English model.
pub fn default_min_similarity(model_folder: &Path) -> f32 {
    match model_folder.file_name().and_then(|n| n.to_str()) {
        Some(DEFAULT_MODEL) => 0.20,
        _ => 0.30,
    }
}

/// A model and its index, shared by the search writer (updates) and the query engine (search).
pub struct Vectors {
    /// The embedding model.
    pub model: StaticModel,
    /// The index.
    pub index: Box<dyn VectorIndex>,
    /// Lowest cosine similarity of a search hit.
    pub min_similarity: f32,
}

impl Vectors {
    /// Loads the model of a folder and makes an empty flat index.
    pub fn load(model_folder: &Path, min_similarity: f32) -> Result<Arc<Vectors>> {
        Ok(Arc::new(Vectors { model: StaticModel::load(model_folder)?, index: Box::new(FlatIndex::default()), min_similarity }))
    }

    /// Embeds and stores a search document. Kinds without a text (files, events) are skipped.
    /// `replace = false` keeps an existing vector (the startup fill must not replace newer ones).
    pub fn upsert(&self, workspace: &str, entity_ref: &str, entity: &Entity, replace: bool) {
        let Some(text) = entity_text(entity) else { return };
        let Some(vector) = self.model.embed(&text) else { return };
        if replace {
            self.index.upsert(workspace, entity_ref, &vector);
        } else {
            self.index.insert_if_absent(workspace, entity_ref, &vector);
        }
    }

    /// Embeds a search document with no lock; `None` for kinds without a text (files, events).
    pub fn embed_entity(&self, entity: &Entity) -> Option<Vec<f32>> {
        self.model.embed(&entity_text(entity)?)
    }

    /// Returns the refs of a workspace that are most similar to a question (at least `min_similarity`).
    pub fn search(&self, workspace: &str, question: &str, limit: usize) -> Vec<VectorHit> {
        match self.model.embed(&split_identifiers(question)) {
            Some(vector) => self.index.search(workspace, &vector, limit, self.min_similarity),
            None => Vec::new(),
        }
    }
}
