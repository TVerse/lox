use lox::interpret;

#[test]
fn comparisons_1() {
    let source = "print nil == true;";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "false\n";
    assert_eq!(&out, expected);
}

#[test]
fn comparisons_2() {
    let source = "print !(5 - 4 > 3 * 2 == !nil);";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "true\n";
    assert_eq!(&out, expected);
}
