#[macro_use]
extern crate rustupolis;
use rustupolis::tuple::E;

extern crate sshtup;
use sshtup::ast;
use sshtup::grammar;

#[test]
fn test_valid() {
    let t = grammar::statement("in []").unwrap();
    assert_eq!(t, ast::Statement::In(tuple![]));
    let t = grammar::statement("in [1, 2, 3]").unwrap();
    assert_eq!(t, ast::Statement::In(tuple![E::I(1), E::I(2), E::I(3)]));
    let t = grammar::statement("in [1,2,3]").unwrap();
    assert_eq!(t, ast::Statement::In(tuple![E::I(1), E::I(2), E::I(3)]));
    let t = grammar::statement("in [_,foo,3.1415]").unwrap();
    assert_eq!(
        t,
        ast::Statement::In(tuple![E::Any, E::str("foo"), E::D(3.1415)])
    );
    let t = grammar::statement("in [command,2,[_,_]]").unwrap();
    assert_eq!(
        t,
        ast::Statement::In(tuple![
            E::str("command"),
            E::I(2),
            E::T(tuple![E::Any, E::Any])
        ])
    );
    let t = grammar::statement(r#"in ["space command",2,[1.5,2.5]]"#).unwrap();
    assert_eq!(
        t,
        ast::Statement::In(tuple![
            E::str("space command"),
            E::I(2),
            E::T(tuple![E::D(1.5), E::D(2.5)])
        ])
    );
    let t = grammar::statement(r#"rd [foo,bar,baz]"#).unwrap();
    assert_eq!(
        t,
        ast::Statement::Rd(tuple![E::str("foo"), E::str("bar"), E::str("baz")])
    );
    let t = grammar::statement(r#"out [foo,bar,baz]"#).unwrap();
    assert_eq!(
        t,
        ast::Statement::Out(tuple![E::str("foo"), E::str("bar"), E::str("baz")])
    );
    let t = grammar::statement(r#"in ["12.34.45"]"#).unwrap();
    assert_eq!(t, ast::Statement::In(tuple![E::str("12.34.45")]));
}

#[test]
fn test_invalid() {
    for ex in &[
        "in [",
        "in ][",
        "nope [1,2,3]",
        "out [,,]",
        "rd [12.34.45]",
        "",
        "in",
        "rd",
        "out",
    ] {
        let result = grammar::statement(ex);
        assert!(!result.is_ok());
    }
}
