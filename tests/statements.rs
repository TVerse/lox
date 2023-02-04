use lox::interpret;

#[test]
fn statements_1() {
    let source = r#"
print "Hi!";
"ignored";
print "How are you!";
    "#;
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "Hi!\nHow are you!\n";
    assert_eq!(&out, expected);
}
