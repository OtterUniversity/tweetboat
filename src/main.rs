use std::fs;
use std::future::IntoFuture;
use std::sync::{Arc, RwLock};

use twilight_gateway::{Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _};
use twilight_http::Client;
use twilight_model::channel::message::{AllowedMentions, MessageFlags};
use twilight_model::id::{
    marker::{ChannelMarker, MessageMarker},
    Id,
};

use crate::cache::CacheEntry;
use crate::pass::Pass;
use crate::{cache::ReplyCache, config::Config};

mod cache;
mod config;
mod pass;

struct State {
    config: Config,
    rest: Client,
    replies: RwLock<ReplyCache>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let config: Config = toml::from_str(&fs::read_to_string("config.toml")?)?;

    let rest = Client::new(config.token.clone());
    let shard = Shard::new(
        ShardId::ONE,
        config.token.clone(),
        Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT,
    );

    let state = Arc::new(State {
        replies: RwLock::new(ReplyCache::with_capacity(config.reply_cache_size)),
        config,
        rest,
    });

    shard_loop(state, shard).await
}

async fn shard_loop(state: Arc<State>, mut shard: Shard) -> Result<(), anyhow::Error> {
    while let Some(event) = shard.next_event(EventTypeFlags::all()).await {
        if let Err(e) = dispatch_event(Arc::clone(&state), event?).await {
            tracing::error!(error = ?e, "Dispatch failed");
        }
    }

    Ok(())
}

/// Launches a background Tokio task to suppress an embed. If the request fails,
/// the error is logged. The resulting [Joinhandle] is returned.
///
/// [JoinHandle]: tokio::task::JoinHandle
fn suppress_embeds_deferred(
    rest: &Client,
    channel_id: Id<ChannelMarker>,
    message_id: Id<MessageMarker>,
) -> tokio::task::JoinHandle<()> {
    // Create the future separately from spawning so that `client` isn't sent across threads
    let f = rest
        .update_message(channel_id, message_id)
        .flags(MessageFlags::SUPPRESS_EMBEDS)
        .into_future();

    tokio::spawn(async move {
        if let Err(e) = f.await {
            tracing::error!(error = ?e, "Error suppressing embeds on {channel_id}/{message_id}");
        }
    })
}

async fn dispatch_event(state: Arc<State>, event: Event) -> Result<(), anyhow::Error> {
    match event {
        // CREATE: Fix embeds when someone sends a twitter link
        Event::MessageCreate(message) => {
            if message.author.bot || state.config.ignored_users.contains(&message.author.id) {
                return Ok(());
            }

            if let Some(content) = Pass::apply_all(&state.config.passes, &message.content) {
                tracing::info!("Rewriting {:?} => {content:?}", message.content);

                // If the unfurler has an embed cached, embeds will be included
                if !message.embeds.is_empty() {
                    suppress_embeds_deferred(&state.rest, message.channel_id, message.id);
                }

                let token = state.replies.write().unwrap().file_pending(message.id);
                if let Some(token) = token {
                    let reply = state
                        .rest
                        .create_message(message.channel_id)
                        .content(&content)
                        .reply(message.id)
                        .allowed_mentions(Some(&AllowedMentions::default()))
                        .await?
                        .model()
                        .await?;

                    state.replies.write().unwrap().insert(token, reply.id);
                }
            }
        }

        // UPDATE: Edit our reply when someone edits a link in/out
        Event::MessageUpdate(message) => {
            let entry = state.replies.read().unwrap().get_entry(message.id);
            let Some(entry) = entry else {
                return Ok(());
            };

            // Suppress embeds the unfurler provided lazily
            if message.embeds.is_some_and(|embeds| !embeds.is_empty()) {
                tracing::info!("Unfurler triggered on {:?}, suppressing...", entry);
                suppress_embeds_deferred(&state.rest, message.channel_id, message.id);
            };

            if let CacheEntry::Filled(reply_id) = entry {
                if let Some(content) = message.content {
                    if let Some(content) = Pass::apply_all(&state.config.passes, &content) {
                        state
                            .rest
                            .update_message(message.channel_id, reply_id)
                            .allowed_mentions(Some(&AllowedMentions::default()))
                            .content(Some(&content))
                            .await?;
                    } else {
                        state
                            .rest
                            .delete_message(message.channel_id, reply_id)
                            .await?;
                    }
                }
            }
        }

        // DELETE: Delete our reply when someone deletes their source message
        Event::MessageDelete(message) => {
            let entry = state.replies.write().unwrap().take_entry(message.id);

            // Temporary extension with `if let` pulls the guard across the await
            // boundary as it keeps the temp. alive for the entire scope, so we need
            // to separate it
            if let Some(CacheEntry::Filled(reply_id)) = entry {
                state
                    .rest
                    .delete_message(message.channel_id, reply_id)
                    .await?;
            }
        }

        _ => {}
    }

    Ok(())
}
