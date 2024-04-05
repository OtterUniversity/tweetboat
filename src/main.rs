mod cache;
mod config;
mod pass;

use std::fs;
use std::future::IntoFuture;
use std::sync::{Arc, RwLock};

use twilight_gateway::{Event, Intents, Shard, ShardId};
use twilight_http::Client;
use twilight_model::channel::message::{AllowedMentions, MessageFlags};

use crate::pass::Pass;
use crate::{cache::ReplyCache, config::Config};

struct State {
    config: Config,
    rest: Client,
    replies: RwLock<ReplyCache>,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    tracing_subscriber::fmt::init();

    let config: Config = toml::from_str(&fs::read_to_string("config.toml")?)?;

    let mut shard = Shard::new(
        ShardId::ONE,
        config.token.clone(),
        Intents::GUILD_MESSAGES | Intents::MESSAGE_CONTENT,
    );
    let rest = Client::new(config.token.clone());

    let state = Arc::new(State {
        replies: RwLock::new(ReplyCache::with_capacity(config.reply_cache_size)),
        config,
        rest,
    });

    loop {
        let event = match shard.next_event().await {
            Ok(event) => event,
            Err(e) => {
                tracing::error!(source = ?e, "Error receiving event");
                if e.is_fatal() {
                    break;
                } else {
                    continue;
                }
            }
        };

        let state = Arc::clone(&state);
        tokio::spawn(async {
            if let Err(e) = process_event(state, event).await {
                tracing::error!(source = ?e, "Error dispatching event");
            }
        });
    }

    Ok(())
}

async fn process_event(state: Arc<State>, event: Event) -> Result<(), anyhow::Error> {
    match event {
        // CREATE: Fix embeds when someone sends a twitter link
        Event::MessageCreate(message) => {
            if message.author.bot {
                return Ok(());
            }

            if let Some(content) = Pass::apply_all(&state.config.passes, &message.content) {
                // If the unfurler has an embed cached, embeds will be included
                if !message.embeds.is_empty() {
                    // `spawn` so that we can suppress embeds and repost in parallel
                    tokio::spawn(
                        state
                            .rest
                            .update_message(message.channel_id, message.id)
                            .flags(MessageFlags::SUPPRESS_EMBEDS)
                            .into_future(),
                    );
                }

                let reply = state
                    .rest
                    .create_message(message.channel_id)
                    .content(&content)?
                    .reply(message.id)
                    .allowed_mentions(Some(&AllowedMentions::default()))
                    .await?
                    .model()
                    .await?;

                state.replies.write().unwrap().insert(message.id, reply.id);
            }
        }

        // DELETE: Delete our reply when someone deletes their source message
        Event::MessageDelete(message) => {
            let reply_id = state.replies.read().unwrap().get_reply(message.id);

            // Temporary extension with `if let` pulls the guard across the await
            // boundary as it keeps the temp. alive for the entire scope, so we need
            // to separate it.
            if let Some(reply_id) = reply_id {
                state
                    .rest
                    .delete_message(message.channel_id, reply_id)
                    .await?;
            }
        }

        // UPDATE: Edit our reply when someone edits a link in/out
        Event::MessageUpdate(message) => {
            let reply_id = state.replies.read().unwrap().get_reply(message.id);
            if let Some(reply_id) = reply_id {
                // Suppress embeds the unfurler provided lazily
                if message.embeds.is_some_and(|embeds| !embeds.is_empty()) {
                    tokio::spawn(
                        state
                            .rest
                            .update_message(message.channel_id, message.id)
                            .flags(MessageFlags::SUPPRESS_EMBEDS)
                            .into_future(),
                    );
                };

                if let Some(content) = message.content {
                    if let Some(content) = Pass::apply_all(&state.config.passes, &content) {
                        state
                            .rest
                            .update_message(message.channel_id, reply_id)
                            .allowed_mentions(Some(&AllowedMentions::default()))
                            .content(Some(&content))?
                            .await?;
                    } else {
                        state.rest.delete_message(message.channel_id, reply_id).await?;
                    }
                }
            }
        }

        _ => {}
    }

    Ok(())
}
