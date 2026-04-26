use std::sync::Arc;
use tracing::{info, debug, instrument};

pub mod piece_table;
use piece_table::{Piece, Source};

/// A snapshot of the buffer at a specific point in time.
/// Highly efficient to clone and thread-safe for background processing (AI/LSP).
#[derive(Clone)]
pub struct Snapshot {
    original: Arc<str>,
    append: Arc<str>,
    pieces: Arc<[Piece]>,
}

impl Snapshot {
    pub fn to_string(&self) -> String {
        let mut result = String::new();
        for piece in self.pieces.iter() {
            let source_text = match piece.source {
                Source::Original => &self.original,
                Source::Append => &self.append,
            };
            result.push_str(&source_text[piece.offset..piece.offset + piece.length]);
        }
        result
    }

    pub fn len(&self) -> usize {
        self.pieces.iter().map(|p| p.length).sum()
    }
}

/// A production-grade IDE text buffer implementation using a Piece Table.
pub struct Buffer {
    original: Arc<str>,
    append: String,
    pieces: Vec<Piece>,
}

impl Buffer {
    pub fn new(original: &str) -> Self {
        info!(len = original.len(), "Initializing new buffer");
        let original: Arc<str> = Arc::from(original);
        let pieces = vec![Piece::new(Source::Original, 0, original.len())];
        
        Self {
            original,
            append: String::new(),
            pieces,
        }
    }

    #[instrument(skip(self, text))]
    pub fn insert(&mut self, offset: usize, text: &str) {
        if text.is_empty() {
            return;
        }

        let append_offset = self.append.len();
        self.append.push_str(text);
        let new_piece = Piece::new(Source::Append, append_offset, text.len());

        let (piece_idx, offset_in_piece) = self.find_piece(offset);
        debug!(piece_idx, offset_in_piece, "Splitting piece for insertion");
        
        let target_piece = self.pieces[piece_idx];
        
        if offset_in_piece == 0 {
            self.pieces.insert(piece_idx, new_piece);
        } else if offset_in_piece == target_piece.length {
            self.pieces.insert(piece_idx + 1, new_piece);
        } else {
            let left = Piece::new(target_piece.source, target_piece.offset, offset_in_piece);
            let right = Piece::new(
                target_piece.source, 
                target_piece.offset + offset_in_piece, 
                target_piece.length - offset_in_piece
            );
            
            self.pieces[piece_idx] = left;
            self.pieces.insert(piece_idx + 1, new_piece);
            self.pieces.insert(piece_idx + 2, right);
        }
    }

    #[instrument(skip(self))]
    pub fn delete(&mut self, offset: usize, length: usize) {
        if length == 0 {
            return;
        }

        let mut remaining_to_delete = length;
        let current_offset = offset;

        while remaining_to_delete > 0 {
            let (piece_idx, offset_in_piece) = self.find_piece(current_offset);
            let target_piece = self.pieces[piece_idx];
            let available_in_piece = target_piece.length - offset_in_piece;
            let delete_in_this_piece = std::cmp::min(remaining_to_delete, available_in_piece);

            if offset_in_piece == 0 && delete_in_this_piece == target_piece.length {
                self.pieces.remove(piece_idx);
            } else if offset_in_piece == 0 {
                self.pieces[piece_idx].offset += delete_in_this_piece;
                self.pieces[piece_idx].length -= delete_in_this_piece;
            } else if offset_in_piece + delete_in_this_piece == target_piece.length {
                self.pieces[piece_idx].length -= delete_in_this_piece;
            } else {
                let left = Piece::new(target_piece.source, target_piece.offset, offset_in_piece);
                let right = Piece::new(
                    target_piece.source,
                    target_piece.offset + offset_in_piece + delete_in_this_piece,
                    target_piece.length - offset_in_piece - delete_in_this_piece
                );
                self.pieces[piece_idx] = left;
                self.pieces.insert(piece_idx + 1, right);
            }

            remaining_to_delete -= delete_in_this_piece;
        }
    }

    /// Takes a thread-safe snapshot of the current buffer state.
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            original: Arc::clone(&self.original),
            append: Arc::from(self.append.as_str()),
            pieces: Arc::from(self.pieces.as_slice()),
        }
    }

    fn find_piece(&self, mut offset: usize) -> (usize, usize) {
        for (idx, piece) in self.pieces.iter().enumerate() {
            if offset <= piece.length {
                return (idx, offset);
            }
            offset -= piece.length;
        }
        (self.pieces.len() - 1, self.pieces.last().unwrap().length)
    }

    pub fn to_string(&self) -> String {
        self.snapshot().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_insertion() {
        let mut buffer = Buffer::new("Hello World");
        buffer.insert(5, ",");
        assert_eq!(buffer.to_string(), "Hello, World");
    }

    #[test]
    fn test_multiple_insertions() {
        let mut buffer = Buffer::new("Hello");
        buffer.insert(5, " World");
        buffer.insert(0, "Greetings: ");
        assert_eq!(buffer.to_string(), "Greetings: Hello World");
    }

    #[test]
    fn test_deletion() {
        let mut buffer = Buffer::new("Hello, World");
        buffer.delete(5, 1);
        assert_eq!(buffer.to_string(), "Hello World");
    }

    #[test]
    fn test_snapshot_consistency() {
        let mut buffer = Buffer::new("Initial");
        let snapshot_1 = buffer.snapshot();
        
        buffer.insert(7, " state");
        let snapshot_2 = buffer.snapshot();
        
        assert_eq!(snapshot_1.to_string(), "Initial");
        assert_eq!(snapshot_2.to_string(), "Initial state");
        assert_eq!(buffer.to_string(), "Initial state");
    }
}
