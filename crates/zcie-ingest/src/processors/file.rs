//! `IndexFile` and `DeleteFile` handlers (`docs/07-ingest-and-indexing.md` section 6).

use std::path::Path;

use zcie_parse::ParsedFile;
use zcie_search::SearchOp;
use zcie_store::{Cf, layout};
use zcie_types::job::split_file_target;
use zcie_types::{
    EdgeStatus, Evidence, EvidenceKind, FileId, FileRecord, JobRecord, ParserQuality, Relation, SymbolRecord,
    WorktreeRecord,
};

use crate::error::{Error, Result};
use crate::ingest::Ingest;
use crate::processors::Output;
use crate::processors::search::{file_op, symbol_op};

/// A file that was read, hashed and parsed outside the write lock.
pub struct PreparedFile {
    worktree: WorktreeRecord,
    rel_path: String,
    hash: String,
    size: u64,
    is_binary: bool,
    language: Option<String>,
    text: Option<String>,
    parsed: Option<ParsedFile>,
}

impl Ingest {
    fn job_worktree(&self, job: &JobRecord) -> Result<(WorktreeRecord, String)> {
        let (worktree_id, path) = split_file_target(&job.target).ok_or_else(|| Error::Invalid(job.target.clone()))?;
        let key = layout::registry(self.tenant(), "worktree", worktree_id);
        let worktree: WorktreeRecord = self.store.get(Cf::Registry, &key)?.ok_or_else(|| Error::NotFound(worktree_id.into()))?;
        Ok((worktree, path.to_string()))
    }

    /// Reads, hashes and parses the file of a job. Returns `None` if the file is gone or excluded.
    pub(crate) fn prepare_file(&self, job: &JobRecord) -> Result<Option<PreparedFile>> {
        let (worktree, rel_path) = self.job_worktree(job)?;
        let abs = Path::new(&worktree.root_path).join(&rel_path);
        if self.config.exclude.is_excluded(&rel_path) || !abs.is_file() {
            return Ok(None);
        }
        let bytes = std::fs::read(&abs)?;
        let size = bytes.len() as u64;
        let hash = zcie_fs::hash_bytes(&bytes);
        let is_binary = zcie_fs::is_binary(&bytes);
        let too_large = size > self.config.max_file_bytes;
        let language = zcie_fs::language_for_path(&rel_path)
            .filter(|l| self.config.languages.iter().any(|c| c == l) && !is_binary && !too_large)
            .map(str::to_string);
        let text = (!is_binary && !too_large).then(|| String::from_utf8_lossy(&bytes).into_owned());
        if text.is_some() {
            self.objects.put(&bytes)?;
        }
        let parsed = match (&language, &text) {
            (Some(language), Some(text)) => self.parser.parse_file(language, &rel_path, text).ok(),
            _ => None,
        };
        Ok(Some(PreparedFile { worktree, rel_path, hash, size, is_binary, language, text, parsed }))
    }

    /// Stores the file record, its symbols and its edges. Skips an unchanged file.
    pub(crate) fn index_file(&self, job: &JobRecord, prepared: PreparedFile) -> Result<Output> {
        let mut output = Output::default();
        let wt = prepared.worktree.worktree_id.clone();
        output.done_paths.push((wt.clone(), prepared.rel_path.clone()));
        let file_id = self.file_id_for(&mut output.batch, &wt, &prepared.rel_path)?;
        let t = self.tenant().to_string();
        let old: Option<FileRecord> = self.store.get(Cf::Entities, &layout::entity(&t, "file", file_id.as_str()))?;
        if old.as_ref().is_some_and(|o| o.content_hash == prepared.hash && !o.deleted) {
            return Ok(output);
        }
        let record = self.file_record(job, &prepared, file_id, old.as_ref());
        output.batch.put(Cf::Entities, layout::entity(&t, "file", record.file_id.as_str()), &record)?;
        let state = zcie_store::values::FileStateValue { v: 1, content_hash: record.content_hash.clone(), seq: job.source_seq };
        output.batch.put(Cf::Revisions, layout::file_state(&t, &wt, &record.path), &state)?;
        let text = prepared.text.as_deref().map(|text| self.search_text(text));
        output.search_ops.extend(file_op(&record, text.as_deref()));
        output.changed_refs.push(format!("file:{}", record.file_id));
        match &prepared.parsed {
            Some(parsed) => self.store_symbols(&mut output, &record, parsed, text.as_deref().unwrap_or_default())?,
            None => self.remove_symbols(&mut output, &record)?,
        }
        Ok(output)
    }

    fn search_text(&self, text: &str) -> String {
        if self.config.redact_secrets { zcie_security::redact(text).0 } else { text.to_string() }
    }

    fn file_record(&self, job: &JobRecord, prepared: &PreparedFile, file_id: FileId, old: Option<&FileRecord>) -> FileRecord {
        let mut scope = job.scope.clone();
        scope.repository_id = Some(prepared.worktree.repository_id.clone());
        scope.worktree_id = Some(prepared.worktree.worktree_id.clone());
        FileRecord {
            v: 1,
            file_id,
            scope,
            path: prepared.rel_path.clone(),
            language: prepared.language.clone(),
            content_hash: prepared.hash.clone(),
            size: prepared.size,
            is_binary: prepared.is_binary,
            git_revision: old.and_then(|o| o.git_revision.clone()),
            indexed_seq: job.source_seq,
            deleted: false,
            content_stored: prepared.text.is_some(),
        }
    }

    fn store_symbols(&self, output: &mut Output, file: &FileRecord, parsed: &ParsedFile, text: &str) -> Result<()> {
        let t = file.scope.tenant_id.clone();
        let file_ref = format!("file:{}", file.file_id);
        let update = zcie_symbols::upsert_file_symbols(&self.store, &mut output.batch, file, parsed)?;
        self.drop_symbols(output, &update.removed)?;
        zcie_graph::delete_edges_from(&self.store, &mut output.batch, &t, &file_ref, Relation::DefinesSymbol)?;
        let lines: Vec<&str> = text.lines().collect();
        for symbol in &update.symbols {
            let sym_ref = format!("sym:{}", symbol.symbol_id);
            let evidence = revision_evidence(EvidenceKind::AstResolved, &file.content_hash);
            let edge = zcie_graph::new_edge(&file.scope, &file_ref, Relation::DefinesSymbol, &sym_ref, EdgeStatus::Verified, evidence);
            zcie_graph::put_edge(&self.store, &mut output.batch, edge)?;
            output.search_ops.push(symbol_op(symbol, &lines));
            output.changed_refs.push(sym_ref);
        }
        self.store_imports(output, file, parsed)?;
        if parsed.quality == ParserQuality::PartialSyntaxError {
            tracing::debug!(path = %file.path, "file has syntax errors");
        }
        Ok(())
    }

    fn store_imports(&self, output: &mut Output, file: &FileRecord, parsed: &ParsedFile) -> Result<()> {
        let t = file.scope.tenant_id.clone();
        let worktree_id = file.scope.worktree_id.clone().unwrap_or_default();
        let file_ref = format!("file:{}", file.file_id);
        zcie_graph::delete_edges_from(&self.store, &mut output.batch, &t, &file_ref, Relation::Imports)?;
        for import in &parsed.imports {
            for candidate in zcie_parse::resolve_relative_import(&file.path, import) {
                let key = layout::path(&t, &worktree_id, &candidate);
                let Some(found) = self.store.get::<zcie_store::values::FileIdValue>(Cf::Lookup, &key)? else { continue };
                let evidence = revision_evidence(EvidenceKind::AstApproximate, &file.content_hash);
                let target = format!("file:{}", found.file_id);
                let edge = zcie_graph::new_edge(&file.scope, &file_ref, Relation::Imports, &target, EdgeStatus::Inferred, evidence);
                zcie_graph::put_edge(&self.store, &mut output.batch, edge)?;
                break;
            }
        }
        Ok(())
    }

    fn remove_symbols(&self, output: &mut Output, file: &FileRecord) -> Result<()> {
        let t = file.scope.tenant_id.clone();
        let removed = zcie_symbols::remove_file_symbols(&self.store, &mut output.batch, &t, file.file_id.as_str())?;
        self.drop_symbols(output, &removed)?;
        let file_ref = format!("file:{}", file.file_id);
        zcie_graph::delete_edges_from(&self.store, &mut output.batch, &t, &file_ref, Relation::DefinesSymbol)?;
        zcie_graph::delete_edges_from(&self.store, &mut output.batch, &t, &file_ref, Relation::Imports)?;
        Ok(())
    }

    fn drop_symbols(&self, output: &mut Output, symbols: &[SymbolRecord]) -> Result<()> {
        for symbol in symbols {
            let sym_ref = format!("sym:{}", symbol.symbol_id);
            zcie_graph::delete_all_edges_of(&self.store, &mut output.batch, &symbol.scope.tenant_id, &sym_ref)?;
            output.search_ops.push(SearchOp::Delete(sym_ref.clone()));
            output.changed_refs.push(sym_ref);
        }
        Ok(())
    }

    /// Marks a file deleted and removes its symbols and code edges. History edges stay.
    pub(crate) fn delete_file(&self, job: &JobRecord) -> Result<Output> {
        let mut output = Output::default();
        let (worktree, rel_path) = self.job_worktree(job)?;
        output.done_paths.push((worktree.worktree_id.clone(), rel_path.clone()));
        let t = self.tenant().to_string();
        let key = layout::path(&t, &worktree.worktree_id, &rel_path);
        let Some(found) = self.store.get::<zcie_store::values::FileIdValue>(Cf::Lookup, &key)? else { return Ok(output) };
        let entity_key = layout::entity(&t, "file", &found.file_id);
        let Some(mut record) = self.store.get::<FileRecord>(Cf::Entities, &entity_key)? else { return Ok(output) };
        if record.deleted {
            return Ok(output);
        }
        record.deleted = true;
        record.indexed_seq = job.source_seq;
        output.batch.put(Cf::Entities, entity_key, &record)?;
        output.batch.delete(Cf::Revisions, layout::file_state(&t, &worktree.worktree_id, &rel_path));
        self.remove_symbols(&mut output, &record)?;
        let file_ref = format!("file:{}", record.file_id);
        output.search_ops.push(SearchOp::Delete(file_ref.clone()));
        output.changed_refs.push(file_ref);
        Ok(output)
    }
}

fn revision_evidence(kind: EvidenceKind, revision: &str) -> Evidence {
    Evidence { kind, source_event_ids: Vec::new(), revision: Some(revision.to_string()), note: None }
}
