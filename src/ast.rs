extern crate rustupolis;
use rustupolis::tuple::Tuple;

#[derive(Debug, PartialEq)]
pub enum Statement {
    In(Tuple),
    Rd(Tuple),
    Out(Tuple),
}
