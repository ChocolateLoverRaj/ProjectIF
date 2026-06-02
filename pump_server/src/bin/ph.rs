#![no_std]
#![no_main]

use defmt::{Debug2Format, info};
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embedded_io_async::Write;
use esp_backtrace as _;
use esp_hal::{
    interrupt::software::SoftwareInterruptControl,
    timer::timg::TimerGroup,
    uart::{self, Uart, uhci::Uhci},
};

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    let _ = spawner;

    let p = esp_hal::init(Default::default());

    // Needed for esp_rtos
    let timg0 = TimerGroup::new(p.TIMG0);
    let software_interrupt = SoftwareInterruptControl::new(p.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, software_interrupt.software_interrupt0);

    esp_alloc::heap_allocator!(size: 72 * 1024);

    info!("PH Sensor Soon");

    let uart = Uart::new(
        p.UART1,
        uart::Config::default()
            .with_baudrate(9600)
            .with_data_bits(uart::DataBits::_8)
            .with_stop_bits(uart::StopBits::_1)
            .with_parity(uart::Parity::None),
    )
    .unwrap()
    .with_tx(p.GPIO6)
    .with_rx(p.GPIO4)
    .into_async();
    let (mut rx, mut tx) = uart.split();
    join(
        async {
            tx.write_all(b"i\r\n").await.unwrap();
            info!("Requested Info");
            tx.write_all(b"I2C,99\r\n").await.unwrap();
            info!("Switched to I2C mode");
            // tx.write_all(b"C,1\r\n").await.unwrap();
            // info!("Enabled continuous reading for every 1s");
            // tx.write_all(b"R\r").await.unwrap();
            // info!("Did single read");
            // tx.write_all(b"R\r").await.unwrap();
            // info!("Did single read");
        },
        async {
            let mut buffer = [Default::default(); 1024];
            loop {
                let bytes_read = rx.read_async(&mut buffer).await.unwrap();
                let data = &buffer[..bytes_read];
                let data_str = str::from_utf8(data);
                info!("Data: {:X} {}", data, Debug2Format(&data_str));
            }
        },
    )
    .await;
}
