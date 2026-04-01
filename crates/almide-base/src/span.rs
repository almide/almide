use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Span {
    pub line: usize,
    pub col: usize,
    #[serde(default)]
    pub end_col: usize,
}
