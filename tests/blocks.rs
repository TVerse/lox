use lox::interpret;

#[test]
fn blocks_1() {
    let source = r#"
print "a";
{
    print "b";
    { print "c";}
}"#;
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "a\nb\nc\n";
    assert_eq!(&out, expected);
}
