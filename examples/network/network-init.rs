use dn_internal::network;
use std::{fs, thread, time::Duration};

fn main() {
    let file_str = fs::read_to_string("examples/network/config.toml").unwrap();
    let config = toml::from_str(&file_str).unwrap();
    let mut controller = network::init_network(&config).unwrap();
    println!("{:#?}", controller);
    controller.crash_drone(5).unwrap();

    thread::sleep(Duration::from_secs(4));
}
