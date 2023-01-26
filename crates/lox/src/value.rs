use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum Value {
    Number(f64)
}

impl Value {
    pub fn new(inner: f64) -> Self {
        Self::Number(inner)
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            Value::Number(num) => num
        })
    }
}
