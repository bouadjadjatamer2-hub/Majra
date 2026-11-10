use serde::{Deserialize, Serialize};

pub const MIN_BLOCK_BYTES: u64 = 1 << 20;
pub const MAX_BLOCK_BYTES: u64 = 16 << 20;
pub const DEFAULT_CONNECTIONS: u32 = 8;
pub const MAX_CONNECTIONS: u32 = 32;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub index: u64,
    pub start: u64,
    /// Inclusive end byte, as HTTP ranges are inclusive.
    pub end: u64,
}

impl Block {
    pub fn len(&self) -> u64 {
        self.end - self.start + 1
    }

    pub fn is_empty(&self) -> bool {
        false
    }
}

/// Small enough to keep the tail short, large enough that per-request overhead disappears.
/// The value scales down for small files and up for large ones, within fixed bounds.
pub fn block_size_for(total_bytes: u64, connections: u32) -> u64 {
    if total_bytes == 0 {
        return MIN_BLOCK_BYTES;
    }
    let connections = connections.clamp(1, MAX_CONNECTIONS) as u64;
    (total_bytes / (connections * 8)).clamp(MIN_BLOCK_BYTES, MAX_BLOCK_BYTES)
}

pub fn plan_blocks(total_bytes: u64, block_size: u64) -> Vec<Block> {
    if total_bytes == 0 {
        return Vec::new();
    }
    let block_size = block_size.max(1);
    let count = total_bytes.div_ceil(block_size);
    (0..count)
        .map(|index| {
            let start = index * block_size;
            let end = ((index + 1) * block_size).min(total_bytes) - 1;
            Block { index, start, end }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_every_byte_once() {
        let total = 10_000_001;
        let blocks = plan_blocks(total, 1 << 20);
        assert_eq!(blocks.first().unwrap().start, 0);
        assert_eq!(blocks.last().unwrap().end, total - 1);
        for pair in blocks.windows(2) {
            assert_eq!(pair[0].end + 1, pair[1].start);
        }
        assert_eq!(blocks.iter().map(Block::len).sum::<u64>(), total);
    }

    #[test]
    fn tiny_files_get_one_block() {
        let blocks = plan_blocks(10, 1 << 20);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].end, 9);
    }

    #[test]
    fn block_size_is_bounded() {
        assert_eq!(block_size_for(1, 32), MIN_BLOCK_BYTES);
        assert_eq!(block_size_for(100 * 1024 * 1024 * 1024, 8), MAX_BLOCK_BYTES);
    }
}
