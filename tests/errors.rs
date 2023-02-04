use lox::{interpret, InterpretError};

#[test]
fn errors() {
    let source = r#""hi" "i";
!;
naf;
"#;
    let mut out = Vec::new();
    let err = interpret(source, &mut out).unwrap_err();
    let errs = match err {
        InterpretError::CompileErrors(e) => e,
        InterpretError::InterpretError(_) => panic!(),
    };
    assert_eq!(errs.errors().len(), 2);
}
