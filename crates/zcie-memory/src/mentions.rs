//! Mention edges from a memory record (X-01, X-02, X-05).

use zcie_store::{Batch, Cf, Store, layout};
use zcie_types::{EdgeStatus, EventEnvelope, EvidenceKind, Relation};

use crate::capture::{find_file_ref, worktrees_of_workspace};
use crate::error::Result;
use crate::extraction::tier_a::Matches;
use crate::records::evidence;

/// Maximum symbol matches for one name. More matches make no edge.
const MAX_SYMBOL_MATCHES: usize = 5;
/// Maximum commit matches for one SHA prefix.
const MAX_COMMIT_MATCHES: usize = 1;

/// Adds `References` edges (TextMention) from `from_ref` to mentioned files, symbols and commits.
pub fn add_mentions(store: &Store, batch: &mut Batch, event: &EventEnvelope, from_ref: &str, matches: &Matches) -> Result<Vec<String>> {
    let mut targets = Vec::new();
    let tenant = &event.scope.tenant_id;
    let workspace = &event.scope.workspace_id;
    for path in &matches.paths {
        if let Some(file_ref) = find_file_ref(store, batch, tenant, workspace, path)? {
            targets.push(file_ref);
        }
    }
    for name in &matches.symbol_names {
        let found = zcie_symbols::find_by_name(store, tenant, workspace, name, MAX_SYMBOL_MATCHES + 1)?;
        if (1..=MAX_SYMBOL_MATCHES).contains(&found.len()) {
            targets.extend(found.into_iter().map(|s| format!("sym:{}", s.symbol_id)));
        }
    }
    targets.extend(find_commits(store, tenant, workspace, &matches.shas)?);
    targets.dedup();
    for target in &targets {
        let evidence = evidence(EvidenceKind::TextMention, &[&event.event_id], None);
        let edge = zcie_graph::new_edge(&event.scope, from_ref, Relation::References, target, EdgeStatus::Inferred, evidence);
        zcie_graph::put_edge(store, batch, edge)?;
    }
    Ok(targets)
}

fn find_commits(store: &Store, tenant: &str, workspace_id: &str, shas: &[String]) -> Result<Vec<String>> {
    if shas.is_empty() {
        return Ok(Vec::new());
    }
    let mut repositories: Vec<String> = worktrees_of_workspace(store, tenant, workspace_id)?
        .into_iter()
        .map(|w| w.repository_id)
        .collect();
    repositories.dedup();
    let mut out = Vec::new();
    for sha in shas {
        for repository in &repositories {
            let keys = store.prefix_keys(Cf::Lookup, &layout::commit_sha(tenant, repository, sha), MAX_COMMIT_MATCHES + 1)?;
            if keys.len() == MAX_COMMIT_MATCHES {
                out.push(format!("commit:commit_{}", zcie_store::keys::last_part(&keys[0])));
            }
        }
    }
    Ok(out)
}
