#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::{
    interrupt::software::SoftwareInterruptControl, peripherals::EFUSE, timer::timg::TimerGroup,
};
use esp_println as _;
use trouble_host::{HostResources, prelude::DefaultPacketPool};

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
        HostResources::<DefaultPacketPool, CONNECTIONS_MAX, L2CAP_CHANNELS_MAX>::new();

    // Use the hardware bluetooth address
    let our_address = Address::random(
        esp_hal::efuse::interface_mac_address(efuse::InterfaceMacAddress::Bluetooth)
            .as_bytes()
            .try_into()
            .unwrap(),
    );
    info!("Our address = {:?}", our_address);

    let stack = trouble_host::new(controller, &mut resources);
    stack
        .set_random_address(our_address)
        .set_random_generator_seed(&mut trng);
    stack.set_io_capabilities(trouble_host::IoCapabilities::KeyboardDisplay);
}
