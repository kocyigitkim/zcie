//! Scope enforcement (`docs/11-security.md` section 4).

use zcie_types::{Classification, Scope};

use crate::auth::{AuthContext, Permission};
use crate::error::{Error, Result};

/// Returns true if the request may read a record.
///
/// Tenant must agree, the record must be in the request workspace, and a `Secret` record needs `Admin`.
pub fn can_read(
    auth: &AuthContext,
    request_scope: &Scope,
    record_scope: &Scope,
    classification: Classification,
) -> bool {
    auth.tenant_id == record_scope.tenant_id
        && record_scope.workspace_id == request_scope.workspace_id
        && (classification != Classification::Secret || auth.is_admin())
}

/// Returns true if the request may write in its scope.
pub fn can_write(auth: &AuthContext, request_scope: &Scope) -> bool {
    auth.can(Permission::Write) && auth.tenant_id == request_scope.tenant_id
}

/// Returns `Forbidden` if the context does not have the permission.
pub fn require(auth: &AuthContext, permission: Permission) -> Result<()> {
    if auth.can(permission) { Ok(()) } else { Err(Error::Forbidden) }
}
