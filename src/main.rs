use dn_internal::network;

fn main() {
    let mut controller = network::init_network("config.toml");
    controller.crash_all();

    while let Some(handle) = controller.handles.pop() {
        handle.join().unwrap();
    }
}
