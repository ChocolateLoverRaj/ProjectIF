#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::{
    efuse,
    interrupt::software::SoftwareInterruptControl,
    rng::{Trng, TrngSource},
    timer::timg::TimerGroup,
};
use esp_println::{self as _, println};
use esp_radio::ble::controller::BleConnector;
use esp_storage::FlashStorage;
use trouble_host::{
    Address, HostResources,
    l2cap::L2capChannel,
    prelude::{
        AdStructure, Advertisement, AdvertisementParameters, BR_EDR_NOT_SUPPORTED,
        DefaultPacketPool, ExternalController, LE_GENERAL_DISCOVERABLE, uuid,
    },
};
use trouble_host::{Stack, l2cap::L2capChannelConfig};

esp_bootloader_esp_idf::esp_app_desc!();

/// Max number of connections
const CONNECTIONS_MAX: usize = 1;

/// Max number of L2CAP channels.
const L2CAP_CHANNELS_MAX: usize = 2; // Signal + att

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let _ = spawner;

    let peripherals = esp_hal::init(Default::default());

    // Needed for esp_rtos
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let software_interrupt = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    info!("Hello from a Rust no_std environment with esp_rtos (basically embassy for ESP32).");

    let _trng_source = TrngSource::new(peripherals.RNG, peripherals.ADC1);
    let mut trng = Trng::try_new().unwrap();

    let bluetooth = peripherals.BT;
    let connector = BleConnector::new(bluetooth, Default::default()).unwrap();
    let controller: ExternalController<_, 20> = ExternalController::new(connector);

    // Initialize the flash
    let mut flash =
        embassy_embedded_hal::adapter::BlockingAsync::new(FlashStorage::new(peripherals.FLASH));

    let mut resources =
        HostResources::<_, DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX>::new();

    // Use the hardware bluetooth address
    let our_address = Address::random(
        esp_hal::efuse::interface_mac_address(efuse::InterfaceMacAddress::Bluetooth)
            .as_bytes()
            .try_into()
            .unwrap(),
    );
    info!("Our address = {:?}", our_address);

    let stack = trouble_host::new(controller, &mut resources);
    let stack = stack
        .set_random_address(our_address)
        .set_random_generator_seed(&mut trng)
        .set_io_capabilities(trouble_host::IoCapabilities::DisplayOnly)
        .build();

    join(
        async {
            let mut adv_data = [0; 31];
            let adv_data_len = AdStructure::encode_slice(
                &[AdStructure::Flags(
                    LE_GENERAL_DISCOVERABLE | BR_EDR_NOT_SUPPORTED,
                )],
                &mut adv_data[..],
            )
            .unwrap();

            let mut scan_data = [0; 31];
            let name = "ProjectIF T";
            let scan_data_len = AdStructure::encode_slice(
                &[
                    AdStructure::CompleteLocalName(name.as_bytes()),
                    AdStructure::CompleteServiceUuids128(&[uuid!(
                        "4ae85006-50ec-40e7-91d9-99c56c5c042a"
                    )
                    .as_raw()
                    .try_into()
                    .unwrap()]),
                ],
                &mut scan_data[..],
            )
            .unwrap();

            let advertiser = stack
                .peripheral()
                .advertise(
                    &Default::default(),
                    Advertisement::ConnectableScannableUndirected {
                        adv_data: &adv_data[..adv_data_len],
                        scan_data: &scan_data[..scan_data_len],
                    },
                )
                .await
                .unwrap();
            info!("Advertising as {:?}", name);
            let conn = advertiser.accept().await.unwrap();

            info!("Connection established!");

            /// Max number of connections
            const CONNECTIONS_MAX: usize = 1;
            // Size of payload we're expecting
            const PAYLOAD_LEN: usize = 27;

            let config = L2capChannelConfig {
                mtu: Some(PAYLOAD_LEN as u16),
                ..Default::default()
            };
            let channel = L2capChannel::listen(&stack, &conn)
                .accept(&config)
                .await
                .unwrap();
            let (mut tx, mut rx) = channel.split();
            join(
                async {
                    loop {
                        let mut buffer = [0; PAYLOAD_LEN];
                        let bytes_read = rx.receive(&stack, &mut buffer).await.unwrap();
                        let buffer = &buffer[..bytes_read];
                        let str = str::from_utf8(buffer);
                        println!("Buffer: {:?} {:?}", buffer, str);
                    }
                },
                async {
                    loop {
                        tx.send(&stack, b"Hello from microcontroller")
                            .await
                            .unwrap();
                        Timer::after_secs(1).await;
                    }
                },
            )
            .await;
        },
        async { stack.runner().run().await.unwrap() },
    )
    .await;
}
