use dn_internal::network;
use std::fs;

fn main() {
    let file_str = fs::read_to_string("config.toml").unwrap();
    let config = toml::from_str(&file_str).unwrap();
    let controller = network::init_network(&config).unwrap();
    println!("{:#?}", controller);
    // let the threads live
    loop {}
}
