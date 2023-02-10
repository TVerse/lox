use crate::heap::Object;
use std::fmt::{Display, Formatter};

#[derive(Debug, Copy, Clone)]
pub enum Value {
    Number(f64),
    Boolean(bool),
    Nil,
    Obj(Object),
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
            (Value::Obj(a), Value::Obj(b)) => *a == *b,
            (Value::Nil, Value::Nil) => true,
            _ => false,
        }
    }
}

impl Value {
    pub fn is_falsey(&self) -> bool {
        matches!(self, Value::Boolean(false) | Value::Nil)
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Value::Number(num) => num.to_string(),
                Value::Boolean(bool) => bool.to_string(),
                Value::Nil => "nil".to_string(),
                Value::Obj(object) => object.to_string(),
            }
        )
    }
}
