use lox::interpret;

#[test]
fn strings_1() {
    let source = "print \"Hi!\";";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "Hi!\n";
    assert_eq!(&out, expected);
}

#[test]
fn strings_2() {
    let source = "print \"Hello\" + \" \" + \"World!\";";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "Hello World!\n";
    assert_eq!(&out, expected);
}

#[test]
fn strings_compare_1() {
    let source = "print \"Hello\" == \"World!\";";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "false\n";
    assert_eq!(&out, expected);
}

#[test]
fn strings_compare_2() {
    let source = "print \"Hello\" == \"Hello\";";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "true\n";
    assert_eq!(&out, expected);
}

#[test]
fn strings_expression_statement() {
    let source = "\"Hi\";";
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.is_empty());
}
