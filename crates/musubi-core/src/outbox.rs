//! Arbeitet die Schreibzugriffe auf AniList aus der Outbox ab.

use std::sync::Arc;
use std::time::Duration;

use musubi_anilist::Error as AniListError;
use musubi_store::{OutboxItem, OutboxOp};
use tokio::time::timeout;

use crate::inner::{Inner, now};
use crate::{CoreEvent, Service};

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_BACKOFF_SECS: i64 = 3600;

fn backoff_secs(attempts: u32) -> i64 {
    (30i64 << attempts.min(10)).min(MAX_BACKOFF_SECS)
}

enum Outcome {
    Continue,
    /// Token ungültig: Erst nach einem neuen Login weitermachen.
    StopUntilLogin,
}

pub(crate) fn spawn_worker(inner: Arc<Inner>) {
    let weak = Arc::downgrade(&inner);
    drop(inner);
    tokio::spawn(async move {
        loop {
            let Some(inner) = weak.upgrade() else { return };
            if let Err(error) = drain(&inner).await {
                tracing::error!(%error, "outbox processing failed");
            }
            // Die starke Referenz endet mit jeder Runde, der Task stirbt also mit dem letzten Core.
            let wake = async { inner.outbox_wake.notified().await };
            let _ = timeout(POLL_INTERVAL, wake).await;
        }
    });
}

async fn drain(inner: &Arc<Inner>) -> crate::Result<()> {
    let Some(session) = inner.anilist() else {
        return Ok(());
    };
    for item in inner.db.outbox_due(now()).await? {
        if let Outcome::StopUntilLogin = process(inner, &session.client, item).await? {
            break;
        }
    }
    Ok(())
}

async fn process(
    inner: &Arc<Inner>,
    client: &musubi_anilist::Client,
    item: OutboxItem,
) -> crate::Result<Outcome> {
    let result = match &item.op {
        OutboxOp::Save(patch) => client.save_entry(patch).await.map(Some),
        OutboxOp::Delete { entry_id } => match client.delete_entry(*entry_id).await {
            Err(AniListError::NotFound) => Ok(None),
            other => other.map(|_| None),
        },
    };

    match result {
        Ok(saved) => {
            let current = inner.db.outbox_done(item.media_id, item.revision).await?;
            if let Some(entry) = saved
                && current
            {
                let title = entry.media.title.preferred().to_owned();
                let progress = entry.progress;
                let kind = inner.upsert_entry(entry);
                inner.persist_list(kind).await?;
                inner.emit(CoreEvent::ListChanged(kind));
                let message = format!("AniList updated: {title} ({progress})");
                inner.db.log_sync(now(), true, &message).await?;
                inner.emit(CoreEvent::Notice { ok: true, message });
            }
            Ok(Outcome::Continue)
        }
        Err(AniListError::Unauthorized) => {
            inner.emit(CoreEvent::AuthRequired(Service::AniList));
            Ok(Outcome::StopUntilLogin)
        }
        Err(error) if error.is_transient() => {
            let retry = now() + backoff_secs(item.attempts);
            inner
                .db
                .outbox_retry_later(
                    item.media_id,
                    item.revision,
                    retry,
                    &error.to_string(),
                    false,
                )
                .await?;
            Ok(Outcome::Continue)
        }
        Err(error) => {
            let message = format!("AniList write for media {} failed: {error}", item.media_id);
            tracing::warn!("{message}");
            inner
                .db
                .outbox_retry_later(item.media_id, item.revision, 0, &error.to_string(), true)
                .await?;
            inner.db.log_sync(now(), false, &message).await?;
            inner.emit(CoreEvent::Notice { ok: false, message });
            Ok(Outcome::Continue)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::backoff_secs;

    #[test]
    fn backoff_grows_and_is_capped() {
        assert_eq!(backoff_secs(0), 30);
        assert_eq!(backoff_secs(1), 60);
        assert_eq!(backoff_secs(3), 240);
        assert_eq!(backoff_secs(50), 3600);
    }
}
