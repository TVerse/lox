use lox::interpret;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;

static EXPECTED_OUTPUT: Lazy<Regex> = Lazy::new(|| Regex::new("// expect: ?(.*)").unwrap());
static EXPECTED_ERROR: Lazy<Regex> = Lazy::new(|| Regex::new("// (Error.*)").unwrap());
static EXPECTED_ERROR_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new("// \\[((java|c) )?line (\\d+)] (Error.*)").unwrap());
static EXPECTED_RUNTIME_ERROR: Lazy<Regex> =
    Lazy::new(|| Regex::new("// expect runtime error: (.+)").unwrap());
// static EXPECTED_SYNTAX_ERROR: Lazy<Regex> =
//     Lazy::new(|| Regex::new("\\[.*line (\\d+)] (Error.+)").unwrap());
// static EXPECTED_STACK_TRACE: Lazy<Regex> = Lazy::new(|| Regex::new("\\[line (\\d+)\\]").unwrap());

fn execute_test(source: &str) {
    let lines = source.lines();
    let mut expected_output = String::new();
    let mut expected_errors: HashSet<String> = HashSet::new();
    let mut expected_runtime_error: Option<String> = None;
    for (_linenum, line) in lines.enumerate() {
        if let Some(m) = EXPECTED_OUTPUT.captures(line) {
            expected_output.push_str(&m[1]);
            expected_output.push('\n');
        } else if let Some(m) = EXPECTED_ERROR.captures(line) {
            expected_errors.insert(m[1].to_string());
        } else if let Some(m) = EXPECTED_ERROR_LINE.captures(line) {
            expected_errors.insert(format!("[line {}] {}", &m[3], &m[4]));
        } else if let Some(m) = EXPECTED_RUNTIME_ERROR.captures(line) {
            expected_runtime_error = Some(m[1].to_string())
        }
    }
    let mut out = Vec::new();
    let res = interpret(source, &mut out);
    let out = String::from_utf8(out).unwrap();
    if let Some(runtime_error) = expected_runtime_error {
        assert!(res.is_err());
        let res = res.unwrap_err();
        let res = res.to_string();
        assert!(
            res.contains(&runtime_error),
            "Got:\n{res}, expected to find:\n{runtime_error}"
        )
    } else if expected_errors.is_empty() {
        assert!(res.is_ok(), "Expected OK, got {}", res.unwrap_err());
        assert_eq!(out, expected_output);
    } else {
        assert!(res.is_err());
        let res = res.unwrap_err();
        let res = res.to_string();
        for e in expected_errors {
            assert!(res.contains(&e), "Got:\n{res}, expected to find:\n{e}")
        }
    }
}

macro_rules! test_bundled {
    ($folder:literal : $($files:literal),+ ,) => {
        paste::item! {
            mod [< bundled_$folder >] {
                test_bundled_inner!($folder: $($files),+);
            }
        }
    };
}

macro_rules! test_bundled_inner {
    ($folder:literal : $file:literal) => {
        paste::item! {
            #[test]
            #[cfg_attr(miri, ignore)]
            fn [< test_ $file >]() {
                // Embed so Miri can work
                let source = include_str!(concat!("lox/", $folder, "/", $file, ".lox"));
                crate::execute_test(source)
            }
        }
    };
    ($folder:literal : $file:literal, $($files:literal),+ ) => {
        test_bundled_inner!($folder: $file);
        test_bundled_inner!($folder: $($files),+);
    };
}

test_bundled!("assignment":
    "associativity",
    "global",
    "grouping",
    "infix_operator",
    "local",
    "prefix_operator",
    "syntax",
    // "to_this",
    "undefined",
);

test_bundled!("block":
    // "empty",
    "scope",
);

test_bundled!("bool":
    "equality",
    "not",
);

// test_bundled!("call":
//     "bool",
//     "nil",
//     "num",
//     "object",
//     "string",
// );

// test_bundled!("class":
//     "empty",
//     "inherit_self",
//     "inherited_method",
//     "local_inherit_other",
//     "local_inherit_self",
//     "local_reference_self",
//     "reference_self",
// );

// test_bundled!("closure":
//     "assign_to_closure",
//     "assign_to_shadowed_later",
//     "close_over_function_parameter",
//     "close_over_later_variable",
//     "close_over_method_parameter",
//     "closed_closure_in_function",
//     "nested_closure",
//     "open_closure_in_function",
//     "reference_closure_multiple_times",
//     "reuse_closure_slot",
//     "shadow_closure_with_local",
//     "unused_closure",
//     "unused_later_closure",
// );

// test_bundled!("comments":
//     "line_at_eof",
//     "only_line_comment",
//     "only_line_comment_and_line",
//     "unicode",
// );

// test_bundled!("constructor":
//     "arguments",
//     "call_init_early_return",
//     "call_init_explicitly",
//     "default",
//     "default_arguments",
//     "early_return",
//     "extra_arguments",
//     "init_not_method",
//     "missing_arguments",
//     "return_in_nested_function",
//     "return_value",
// );

// test_bundled!("field":
//     "call_function_field",
//     "call_nonfunction_field",
//     "get_and_set_method",
//     "get_on_bool",
//     "get_on_class",
//     "get_on_function",
//     "get_on_nil",
//     "get_on_num",
//     "get_on_string",
//     "many",
//     "method",
//     "method_binds_this",
//     "on_instance",
//     "set_evaluation_order",
//     "set_on_bool",
//     "set_on_class",
//     "set_on_function",
//     "set_on_nil",
//     "set_on_num",
//     "set_on_string",
//     "undefined",
// );

// test_bundled!("for":
//     "class_in_body",
//     "closure_in_body",
//     "fun_in_body",
//     "return_closure",
//     "return_inside",
//     "scope",
//     "statement_condition",
//     "statement_increment",
//     "statement_initializer",
//     "syntax",
//     "var_in_body",
// );

// test_bundled!("function":
//     "body_must_be_block",
//     "empty_body",
//     "extra_arguments",
//     "local_mutual_recursion",
//     "local_recursion",
//     "missing_arguments",
//     "missing_comma_in_parameters",
//     "mutual_recursion",
//     "nested_call_with_arguments",
//     "parameters",
//     "print",
//     "recursion",
//     "too_many_arguments",
//     "too_many_parameters",
// );

// test_bundled!("if":
//     "class_in_else",
//     "class_in_then",
//     "dangling_else",
//     "else",
//     "fun_in_else",
//     "fun_in_then",
//     "if",
//     "truth",
//     "var_in_else",
//     "var_in_then",
// );

// test_bundled!("inheritance":
//     "constructor",
//     "inherit_from_function",
//     "inherit_from_nil",
//     "inherit_from_number",
//     "inherit_methods",
//     "parenthesized_superclass",
//     "set_fields_from_base_class",
// );

// test_bundled!("limit":
// "loop_too_large",
// "no_reuse_constants",
// "stack_overflow",
// "too_many_constants",
// "too_many_locals",
// "too_many_upvalues"
// );

// test_bundled!("logical_operator":
//     "and",
//     "and_truth",
//     "or",
//     "or_truth",
// );

// test_bundled!("method":
//     "arity",
//     "empty_block",
//     "extra_arguments",
//     "missing_arguments",
//     "not_found",
//     "print_bound_method",
//     "refer_to_name",
//     "too_many_arguments",
//     "too_many_parameters",
// );

test_bundled!("nil":
    "literal",
);

test_bundled!("number":
    // "decimal_point_at_eof",
    "leading_dot",
    "literals",
    "nan_equality",
    // "trailing_dot",
);

test_bundled!("operator":
    "add",
    "add_bool_nil",
    "add_bool_num",
    "add_bool_string",
    "add_nil_nil",
    "add_num_nil",
    "add_string_nil",
    "comparison",
    "divide",
    "divide_nonnum_num",
    "divide_num_nonnum",
    "equals",
    // "equals_class",
    // "equals_method",
    "greater_nonnum_num",
    "greater_num_nonnum",
    "greater_or_equal_nonnum_num",
    "greater_or_equal_num_nonnum",
    "less_nonnum_num",
    "less_num_nonnum",
    "less_or_equal_nonnum_num",
    "less_or_equal_num_nonnum",
    "multiply",
    "multiply_nonnum_num",
    "multiply_num_nonnum",
    "negate",
    "negate_nonnum",
    // "not",
    // "not_class",
    "not_equals",
    "subtract",
    "subtract_nonnum_num",
    "subtract_num_nonnum",
);

test_bundled!("print":
    "missing_argument",
);

// test_bundled!("return":
//     "after_else",
//     "after_if",
//     "after_while",
//     "at_top_level",
//     "in_function",
//     "in_method",
//     "return_nil_if_no_value",
// );

test_bundled!("string":
    "error_after_multiline",
    "literals",
    "multiline",
    "unterminated",
);

// test_bundled!("super":
//     "bound_method",
//     "call_other_method",
//     "call_same_method",
//     "closure",
//     "constructor",
//     "extra_arguments",
//     "indirectly_inherited",
//     "missing_arguments",
//     "no_superclass_bind",
//     "no_superclass_call",
//     "no_superclass_method",
//     "parenthesized",
//     "reassign_superclass",
//     "super_at_top_level",
//     "super_in_closure_in_inherited_method",
//     "super_in_inherited_method",
//     "super_in_top_level_function",
//     "super_without_dot",
//     "super_without_name",
//     "this_in_superclass_method",
// );

// test_bundled!("this":
//     "closure",
//     "nested_class",
//     "nested_closure",
//     "this_at_top_level",
//     "this_in_method",
//     "this_in_top_level_function",
// );

test_bundled!("variable":
    // "collide_with_parameter",
    "duplicate_local",
    // "duplicate_parameter",
    // "early_bound",
    "in_middle_of_block",
    "in_nested_block",
    // "local_from_method",
    "redeclare_global",
    "redefine_global",
    "scope_reuse_in_different_blocks",
    "shadow_and_local",
    "shadow_global",
    "shadow_local",
    "undefined_global",
    "undefined_local",
    "uninitialized",
    // "unreached_undefined",
    "use_false_as_var",
    "use_global_in_initializer",
    "use_local_in_initializer",
    "use_nil_as_var",
    "use_this_as_var",
);

// test_bundled!("while":
//     "class_in_body",
//     "closure_in_body",
//     "fun_in_body",
//     "return_closure",
//     "return_inside",
//     "syntax",
//     "var_in_body",
// );
