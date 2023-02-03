use lox::interpret;

#[test]
fn strings_1() {
    let source = "\"Hi!\"";
    let mut out = Vec::new();
    interpret(&source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "Hi!\n";
    assert_eq!(&out, expected);
}

#[test]
fn strings_2() {
    let source = "\"Hello\" + \" \" + \"World!\"";
    let mut out = Vec::new();
    interpret(&source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "Hello World!\n";
    assert_eq!(&out, expected);
}
