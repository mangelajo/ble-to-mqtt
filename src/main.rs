mod config;

use btleplug::api::{Central, CentralEvent, Manager as _, ScanFilter};
use btleplug::platform::{Adapter, Manager};
use byteorder::{ByteOrder, LittleEndian};
use futures::stream::StreamExt;
use std::error::Error;
use clap::Parser;
use std::collections::HashMap;

/// Simple program to greet a person
#[derive(Parser, Debug)]
#[clap(about, version, author)]
struct Args {
    // Config file to parse
    #[clap(short, long, default_value= "/etc/ble-receiver.yaml")]
    config: String,
}

async fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().await.unwrap();
    adapters.into_iter().nth(0).unwrap()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    pretty_env_logger::init();

    let args = Args::parse();

    let cfg = config::read(args.config)?;

    println!("cfg {:?}", cfg);

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
                println!("DeviceDiscovered: {:?}", id);
            }
           
            CentralEvent::ManufacturerDataAdvertisement {
                id,
                manufacturer_data,
            } => {
                let peripheralid_mac = format!("{:?}", id).to_uppercase();
                let mac = &peripheralid_mac[13..=13+16];

                if let Some(data) = manufacturer_data.get(&0x8801_u16) {
                    let temp = LittleEndian::read_i16(&data[4..=5]) as f32 / 100.0;
                    let hum = LittleEndian::read_i16(&data[6..=7]) as f32 / 100.0;
                    let batt = &data[8];
                    println!("{} -> Temp: {}C RH:{}% batt: {}%", mac, temp, hum, batt);

                    let json = format!("{\"temperature\":\"{}\", \"RH\":\"{}\", battery:\"{}\"}",
                        temp, hum, batt);

                    match sensors.get(&*mac) {
                        Some(sensor) => println!("Sensor: {:?}", sensor),
                        None => println!("Received temp for sensor {} not in config", mac),
                    }
                } else {
                    println!(
                        "ManufacturerDataAdvertisement: {:?}, {:?}",
                        id, manufacturer_data
                    );
                }
            }

            _ => {}
        }
    }
    Ok(())
}