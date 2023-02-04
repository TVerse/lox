use lox::interpret;

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
