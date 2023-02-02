use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum Value {
    Number(f64),
    Boolean(bool),
    Nil,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Boolean(a), Value::Boolean(b)) => a == b,
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
            }
        )
    }
}
