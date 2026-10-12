//! La identidad pendiente suspende la presentación, no la historia del documento.

use obrain_core::{EntityId, Result, Store, TimePoint};
use std::collections::HashSet;

pub(crate) fn pending(store: &Store, id: &EntityId, at: TimePoint) -> Result<bool> {
    Ok(store.content_pending(id)?
        || !store
            .query(Some(id), Some("identity_pending_path"), at)?
            .is_empty())
}

pub(crate) fn pending_set(store: &Store, at: TimePoint) -> Result<HashSet<EntityId>> {
    let mut pending = store.content_pending_set()?;
    pending.extend(
        store
            .query(None, Some("identity_pending_path"), at)?
            .into_iter()
            .map(|a| a.subject),
    );
    Ok(pending)
}
