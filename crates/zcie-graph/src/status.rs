//! Edge status rules (`docs/05-data-model.md` section 7.1).

use zcie_types::{EdgeStatus, EvidenceKind};

/// Returns the lower of the requested status and the maximum status of the evidence kind.
pub fn effective_status(requested: EdgeStatus, evidence: EvidenceKind) -> EdgeStatus {
    requested.min(evidence.max_status())
}

/// Returns the status to keep when an edge for the same triple exists.
///
/// The higher status wins. Exception: a user can reject an edge.
pub fn merged_status(old: EdgeStatus, new: EdgeStatus, new_evidence: EvidenceKind) -> EdgeStatus {
    if new == EdgeStatus::Rejected && new_evidence == EvidenceKind::UserConfirmation {
        return EdgeStatus::Rejected;
    }
    if new.rank() > old.rank() { new } else { old }
}
