use serde::Deserialize;

use crate::pass::Pass;

#[derive(Deserialize)]
pub struct Config {
    pub token: String,
    pub reply_cache_size: usize,
    pub stem: String,
    #[serde(rename = "pass")]
    pub passes: Vec<Pass>,
}
