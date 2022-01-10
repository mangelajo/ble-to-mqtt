mod config;

use btleplug::api::{Central, CentralEvent, Manager as _, ScanFilter};
use btleplug::platform::{Adapter, Manager};
use byteorder::{ByteOrder, LittleEndian};
use clap::Parser;
use futures::stream::StreamExt;
use paho_mqtt as mqtt;
use serde_json::json;
use std::collections::HashMap;
use std::error::Error;
use std::{env, process};
extern crate env_logger;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    // Config file to parse
    #[clap(short, long, default_value = "/etc/ble-to-mqtt.yaml")]
    config: String,
}

async fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().await.unwrap();
    adapters.into_iter().nth(0).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    match env::var("RUST_LOG") {
        Err(_e) => env::set_var("RUST_LOG", "info"),
        _ => {}
    }
    pretty_env_logger::init();

    let args = Args::parse();

    let cfg = config::read(args.config)?;

    let cli = mqtt_connect(&cfg);

    let mut sensors = HashMap::new();

    for sensor in cfg.sensors.iter() {
        sensors.insert(sensor.mac.to_uppercase(), sensor);
    }

    let manager = Manager::new().await?;
    let central = get_central(&manager).await;
    let mut events = central.events().await?;

    // start scanning for devices
    central.start_scan(ScanFilter::default()).await?;

    // Print based on whatever the event receiver outputs. Note that the event
    // receiver blocks, so in a real program, this should be run in its own
    // thread (not task, as this library does not yet use async channels).
    while let Some(event) = events.next().await {
        match event {
            CentralEvent::DeviceDiscovered(id) => {
                debug!("DeviceDiscovered: {:?}", id);
            }

            CentralEvent::ManufacturerDataAdvertisement {
                id,
                manufacturer_data,
            } => {
                let peripheralid_mac = format!("{:?}", id).to_uppercase();
                let mac = &peripheralid_mac[13..=13 + 16];
                let mut temp = 0.0;
                let mut hum = 0.0;
                let mut batt: &u8 = &0;
                let mut have_reading = false;

                if let Some(data) = manufacturer_data.get(&0x8801_u16) {
                    temp = LittleEndian::read_i16(&data[4..=5]) as f32 / 100.0;
                    hum = LittleEndian::read_i16(&data[6..=7]) as f32 / 100.0;
                    batt = &data[8];
                    have_reading = true;
                } else if let Some(data) = manufacturer_data.get(&0xec88_u16) {
                    let mut sign = 1.0;
                    let mut data_x =
                        ((data[1] as u32) << 16) + ((data[2] as u32) << 8) + (data[3] as u32);
                    batt = &data[4];

                    if (data_x & 0x800000) > 0 {
                        sign = -1.0;
                        data_x ^= 0x800000;
                    }

                    let temp_u = data_x / 1000;
                    let hum_u = data_x % 1000;

                    temp = sign * (temp_u as f32) / 10.0;
                    hum = (hum_u as f32) / 10.0;
                    have_reading = true;
                } else {
                    debug!(
                        "ManufacturerDataAdvertisement: {:?}, {:02x?}",
                        id, manufacturer_data
                    );
                }

                if have_reading {
                    match sensors.get(&*mac) {
                        Some(sensor) => mqtt_publish_sensor(&cli, sensor, temp, hum, *batt),
                        None => warn!("Received temp for sensor {} not in config", mac),
                    }
                }
            }

            _ => {}
        }
    }
    Ok(())
}

fn mqtt_publish_sensor(
    cli: &Option<mqtt::client::Client>,
    sensor: &config::Sensor,
    temp: f32,
    hum: f32,
    batt: u8,
) {
    let json_data =
        json!({"temperature": format!("{:.1}",temp), "RH": format!("{:.1}",hum), "battery": batt});

    let msg_charge = mqtt::MessageBuilder::new()
        .topic(&sensor.mqtt_publish)
        .payload(json_data.to_string())
        .qos(1)
        .retained(true)
        .finalize();

    match cli {
        Some(mqtt_client) => {
            if let Err(e) = mqtt_client.publish(msg_charge) {
                error!("Error sending message: {:?}", e);
                // exit, as we are managed by systemd or something else, we should re-try the
                // re-connection again on restart
                process::exit(1);
            }
            info!(
                "mqtt published {} -> {}",
                sensor.mqtt_publish,
                json_data.to_string()
            );
        }
        None => info!(
            "mqtt disabled, {} -> {}",
            sensor.mqtt_publish,
            json_data.to_string()
        ),
    }
}

fn mqtt_connect(config: &config::Config) -> Option<mqtt::client::Client> {
    if config.mqtt_config.disabled {
        return None;
    }

    let create_opts = mqtt::CreateOptionsBuilder::new()
        .server_uri(&config.mqtt_config.endpoint)
        .client_id(&config.mqtt_config.client_id)
        .max_buffered_messages(100)
        .finalize();

    let cli = mqtt::Client::new(create_opts).unwrap_or_else(|e| {
        error!("Error creating the client: {:?}", e);
        process::exit(1);
    });

    let conn_opts = mqtt::ConnectOptionsBuilder::new()
        .user_name(&config.mqtt_config.username)
        .password(&config.mqtt_config.password)
        .finalize();

    cli.connect(conn_opts).unwrap();

    info!("Connected to {}", config.mqtt_config.endpoint);

    return Some(cli);
}
