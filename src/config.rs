use serde_derive::{Deserialize, Serialize};
use serde_yaml;
use std::io;
use thiserror::Error;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub mqtt_config: MQTTConfig,
    pub sensors: Vec<Sensor>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MQTTConfig {
    pub disabled: bool,
    pub endpoint: String,
    pub client_id: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Sensor {
    pub name: String,
    pub mac: String,
    pub mqtt_publish: String,
}

#[derive(Debug, Error)]
pub enum ParsingError {
    #[error("Failed to open config file: {0}")]
    FailedToOpen(#[from] io::Error),

    #[error("Failed to parse config file: {0}")]
    FailedToParse(#[from] serde_yaml::Error),
}

pub fn read(filename: String) -> Result<Config, ParsingError> {
    let f = std::fs::File::open(filename)?;
    let cfg = serde_yaml::from_reader(f)?;
    Ok(cfg)
}
