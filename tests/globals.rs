use lox::{interpret, InterpretError};

#[test]
fn var_declaration_1() {
    let source = "var i = 1; print i;";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "1\n";
    assert_eq!(&out, expected);
}

#[test]
fn var_declaration_2() {
    let source = r#"
var beverage = "cafe au lait";
var breakfast = "beignets with " + beverage;
print breakfast;"#;
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "beignets with cafe au lait\n";
    assert_eq!(&out, expected);
}

#[test]
fn var_declaration_3() {
    let source = r#"
var breakfast = "beignets";
var beverage = "cafe au lait";
breakfast = "beignets with " + beverage;

print breakfast;"#;
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "beignets with cafe au lait\n";
    assert_eq!(&out, expected);
}

#[test]
fn var_declaration_4() {
    let source = r#"
var breakfast = "beignets";
breakfast = "beignets with cafe au lait";

print breakfast;"#;
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "beignets with cafe au lait\n";
    assert_eq!(&out, expected);
}

#[test]
fn assignment_precedence() {
    let source = r#"
var a = 1;
var b = 2;
var c = 3;
var d = 4;
a * b = c + d;
    "#;
    let mut out = Vec::new();
    let err = interpret(source, &mut out).unwrap_err();
    let errs = match err {
        InterpretError::CompileErrors(e) => e,
        InterpretError::InterpretError(_) => panic!(),
    };
    assert_eq!(errs.errors().len(), 1);
}
