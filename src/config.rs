use serde::Deserialize;
use twilight_model::id::{marker::UserMarker, Id};

use crate::pass::Pass;

#[derive(Deserialize)]
pub struct Config {
    pub token: String,
    pub reply_cache_size: usize,
    pub seen_cache_size: Option<usize>,
    #[serde(default)]
    pub ignored_users: Vec<Id<UserMarker>>,
    #[serde(default)]
    pub suppress_delay_millis: u64,
    #[serde(rename = "pass")]
    pub passes: Vec<Pass>,
}
