extern crate env_logger;
#[macro_use]
extern crate error_chain;
extern crate futures;
extern crate rustupolis;
extern crate thrussh;
extern crate thrussh_keys;
extern crate tokio_core;

pub mod ast;
pub mod error;
pub mod grammar {
    include!(concat!(env!("OUT_DIR"), "/grammar.rs"));
}
pub mod server;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
