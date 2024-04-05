use regex::Regex;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
pub struct Config {
    pub token: String,
    pub reply_cache_size: usize,
    pub stem: String,
    #[serde(deserialize_with = "de_regex")]
    pub link_pattern: Regex,
}

fn de_regex<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Regex, D::Error> {
    use serde::de::Error as _;

    Regex::new(&String::deserialize(deserializer)?).map_err(D::Error::custom)
}
