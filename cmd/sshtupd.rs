use std::sync::Arc;

extern crate env_logger;
extern crate thrussh;
extern crate thrussh_keys;

extern crate sshtup;
use sshtup::server::H;

fn main() {
    env_logger::init().unwrap();
    let mut config = thrussh::server::Config::default();
    config.connection_timeout = Some(std::time::Duration::from_secs(600));
    config.auth_rejection_time = std::time::Duration::from_secs(3);
    config
        .keys
        .push(thrussh_keys::key::KeyPair::generate(thrussh_keys::key::ED25519).unwrap());
    let config = Arc::new(config);
    let sh = H::new();
    thrussh::server::run(config, "0.0.0.0:2222", sh);
}
