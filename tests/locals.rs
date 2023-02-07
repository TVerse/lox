use lox::interpret;

#[test]
fn locals_1() {
    let source = r#"
var a = "a";
print a;
{
    var b = "b";
    print b;
    {
        var a = "c";
        print a;
    }
}"#;
    let mut out = Vec::new();
    interpret(source, &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    let expected = "a\nb\nc\n";
    assert_eq!(&out, expected);
}
