//! `GET /stream/activity` — SSE feed of NATS `ares.activity` events.
//!
//! A background task relays `ares.activity` messages into a
//! [`tokio::sync::broadcast`] channel; each SSE client subscribes to it.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use async_nats::Client;
use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures::{Stream, StreamExt};
use tokio::sync::broadcast;
use tracing::warn;

use crate::AppState;

/// Broadcast channel capacity; slow clients drop oldest events.
const CHANNEL_CAP: usize = 512;

/// Broadcast sender every SSE handler taps into.
pub type ActivityTx = broadcast::Sender<String>;

/// Subscribes to `ares.activity` and relays each payload into a broadcast
/// channel consumed by SSE handlers.
pub async fn spawn_activity_relay(nats: Client) -> anyhow::Result<ActivityTx> {
    let (tx, _) = broadcast::channel(CHANNEL_CAP);
    let mut sub = nats
        .subscribe(agent_protocol::activity::SUBJECT_ACTIVITY.to_string())
        .await?;
    let relay_tx = tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = sub.next().await {
            let text = String::from_utf8_lossy(&msg.payload).to_string();
            // No receivers is normal before the first console connects.
            if relay_tx.send(text).is_err() && relay_tx.receiver_count() > 0 {
                warn!("activity relay send failed with live receivers");
            }
        }
    });
    Ok(tx)
}

/// `GET /stream/activity` — streams activity events as SSE `data:` frames.
pub async fn activity_sse(
    State(s): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = s.activity_tx.subscribe();
    // `unfold` over the broadcast receiver keeps this dep-free; a lagged
    // receiver skips the gap rather than dying.
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(text) => return Some((Ok(Event::default().data(text)), rx)),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("ping"),
    )
}
