/// Tracks line offsets within a piece of text to enable efficient 
/// (Line, Column) <-> Absolute Offset translation.
#[derive(Debug, Clone)]
pub struct LineMap {
    /// Absolute offsets of the start of each line.
    /// line_starts[0] is always 0.
    line_starts: Vec<usize>,
    /// Total length of the tracked text in bytes.
    total_len: usize,
}

impl LineMap {
    /// Builds a LineMap from a string slice in O(N) time.
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (i, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            line_starts,
            total_len: text.len(),
        }
    }

    /// Converts an absolute byte offset into a (line, column) tuple.
    /// Lines and columns are 0-indexed. O(log N) where N is number of lines.
    pub fn offset_to_point(&self, offset: usize) -> Option<(usize, usize)> {
        if offset > self.total_len {
            return None;
        }
        
        let line = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        
        let line_start = self.line_starts[line];
        let column = offset - line_start;
        Some((line, column))
    }

    /// Converts a (line, column) tuple into an absolute byte offset.
    /// Returns None if the line or column is out of bounds.
    pub fn point_to_offset(&self, line: usize, column: usize) -> Option<usize> {
        if line >= self.line_starts.len() {
            return None;
        }
        
        let line_start = self.line_starts[line];
        let offset = line_start + column;
        
        // Ensure the offset doesn't spill over into the next line
        // or past the end of the text.
        let line_end = if line + 1 < self.line_starts.len() {
            self.line_starts[line + 1] - 1 // -1 for the newline character
        } else {
            self.total_len
        };
        
        if offset > line_end + 1 { // allow pointing exactly at the newline
             return None;
        }
        
        if offset > self.total_len {
            return None;
        }
        
        Some(offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_map() {
        let text = "Hello\nWorld\r\nRust";
        let map = LineMap::new(text);
        
        assert_eq!(map.offset_to_point(0), Some((0, 0))); // H
        assert_eq!(map.offset_to_point(5), Some((0, 5))); // \n
        assert_eq!(map.offset_to_point(6), Some((1, 0))); // W
        assert_eq!(map.offset_to_point(13), Some((2, 0))); // R
        
        assert_eq!(map.point_to_offset(0, 0), Some(0));
        assert_eq!(map.point_to_offset(1, 0), Some(6));
        assert_eq!(map.point_to_offset(2, 0), Some(13));
    }
}
