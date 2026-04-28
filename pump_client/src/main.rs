use std::{collections::HashSet, time::Duration};

use bluer::{
    AdapterEvent, DiscoveryFilter, DiscoveryTransport,
    l2cap::{PSM_LE_DYN_START, SocketAddr, Stream},
};
use futures::{StreamExt, future::join, pin_mut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    time::sleep,
};
use uuid::uuid;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let session = bluer::Session::new().await.unwrap();
    let adapter = session.default_adapter().await.unwrap();
    adapter.set_powered(true).await.unwrap();

    let service = uuid!("4ae85006-50ec-40e7-91d9-99c56c5c042a");
    adapter
        .set_discovery_filter(DiscoveryFilter {
            uuids: HashSet::from_iter([service]),
            transport: DiscoveryTransport::Le,
            ..Default::default()
        })
        .await
        .unwrap();
    let devices = adapter.discover_devices_with_changes().await.unwrap();
    pin_mut!(devices);
    let device = loop {
        if let Some(event) = devices.next().await {
            match event {
                AdapterEvent::DeviceAdded(address) => {
                    let device = adapter.device(address).unwrap();
                    // Make sure it is actually in range
                    let rssi = device.rssi().await.unwrap();
                    if rssi.is_none() {
                        continue;
                    }
                    let uuids = device.uuids().await.unwrap();
                    if !uuids.is_some_and(|uuids| uuids.contains(&service)) {
                        continue;
                    }
                    break device;
                }
                _ => {}
            }
        } else {
            unreachable!("Unexpected end of devices stream");
        }
    };
    let name = device.name().await.unwrap();
    println!("Found tower microcontroller: {name:?}");

    let mut stream = Stream::connect(SocketAddr::new(
        device.address(),
        bluer::AddressType::LeRandom,
        PSM_LE_DYN_START,
    ))
    .await
    .unwrap();
    let (mut rx, mut tx) = stream.split();
    join(
        async move {
            loop {
                let mut buffer = [0; 0x1000];
                let bytes_read = rx.read(&mut buffer).await.unwrap();
                let buffer = &buffer[..bytes_read];
                let str = str::from_utf8(buffer);
                println!("Buffer: {buffer:?} {str:?}");
            }
        },
        async {
            loop {
                tx.write_all(b"Hello from the central computer")
                    .await
                    .unwrap();
                sleep(Duration::from_secs(1)).await;
            }
        },
    )
    .await;
}
