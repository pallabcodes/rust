/// Represents a source of text for the Piece Table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The original, read-only buffer.
    Original,
    /// The append-only buffer for new additions.
    Append,
}

/// A Piece represents a contiguous span of text from one of the source buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub source: Source,
    pub offset: usize,
    pub length: usize,
}

impl Piece {
    pub fn new(source: Source, offset: usize, length: usize) -> Self {
        Self { source, offset, length }
    }
}
