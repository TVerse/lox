use lox::interpret;

#[test]
fn simple_arithmetic_1() {
    let source = "print 1 + 2 + -3 * 4/(3-5);";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "9\n";
    assert_eq!(&out, expected);
}

#[test]
fn simple_arithmetic_2() {
    let source = "print (-1 + 2) * 3 - -4;";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "7\n";
    assert_eq!(&out, expected);
}
