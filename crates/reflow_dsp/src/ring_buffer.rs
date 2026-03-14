//! Fixed-size ring buffer for overlap-add, delay lines, and windowed processing.
//!
//! Used by FFT actors (overlap-add/save), convolution, and any actor that
//! needs to accumulate a fixed window of samples across stream chunks.

/// A fixed-capacity ring buffer of `f32` samples.
///
/// Supports push, read-back (delay line), and bulk drain-to-slice operations
/// needed for windowed processing (FFT, convolution).
#[derive(Debug, Clone)]
pub struct RingBuffer {
    buf: Vec<f32>,
    /// Write position (next index to write to).
    write_pos: usize,
    /// Number of samples currently in the buffer.
    len: usize,
}

impl RingBuffer {
    /// Create a ring buffer with the given capacity, filled with zeros.
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity],
            write_pos: 0,
            len: 0,
        }
    }

    /// Buffer capacity.
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Number of samples currently stored.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether the buffer is full.
    pub fn is_full(&self) -> bool {
        self.len == self.buf.len()
    }

    /// Push a single sample. If full, the oldest sample is overwritten.
    #[inline]
    pub fn push(&mut self, sample: f32) {
        self.buf[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % self.buf.len();
        if self.len < self.buf.len() {
            self.len += 1;
        }
    }

    /// Push a slice of samples. Oldest samples are overwritten if the buffer fills.
    pub fn push_slice(&mut self, samples: &[f32]) {
        for &s in samples {
            self.push(s);
        }
    }

    /// Read the sample at `delay` samples ago (0 = most recent).
    ///
    /// Returns 0.0 if delay exceeds the number of stored samples.
    #[inline]
    pub fn read_delay(&self, delay: usize) -> f32 {
        if delay >= self.len {
            return 0.0;
        }
        let cap = self.buf.len();
        let idx = (self.write_pos + cap - 1 - delay) % cap;
        self.buf[idx]
    }

    /// Copy the contents into `output` in chronological order (oldest first).
    ///
    /// Copies `min(self.len, output.len)` samples. Remaining slots are zeroed.
    pub fn read_ordered(&self, output: &mut [f32]) {
        let n = self.len.min(output.len());
        let cap = self.buf.len();
        let start = (self.write_pos + cap - self.len) % cap;

        for i in 0..n {
            output[i] = self.buf[(start + i) % cap];
        }
        for i in n..output.len() {
            output[i] = 0.0;
        }
    }

    /// Reset the buffer to empty (all zeros).
    pub fn clear(&mut self) {
        self.buf.fill(0.0);
        self.write_pos = 0;
        self.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_read() {
        let mut rb = RingBuffer::new(4);
        rb.push(1.0);
        rb.push(2.0);
        rb.push(3.0);

        assert_eq!(rb.len(), 3);
        assert_eq!(rb.read_delay(0), 3.0); // most recent
        assert_eq!(rb.read_delay(1), 2.0);
        assert_eq!(rb.read_delay(2), 1.0);
        assert_eq!(rb.read_delay(3), 0.0); // beyond stored
    }

    #[test]
    fn test_overwrite_oldest() {
        let mut rb = RingBuffer::new(3);
        rb.push(1.0);
        rb.push(2.0);
        rb.push(3.0);
        assert!(rb.is_full());

        rb.push(4.0); // overwrites 1.0
        assert_eq!(rb.len(), 3);
        assert_eq!(rb.read_delay(0), 4.0);
        assert_eq!(rb.read_delay(1), 3.0);
        assert_eq!(rb.read_delay(2), 2.0);
    }

    #[test]
    fn test_read_ordered() {
        let mut rb = RingBuffer::new(4);
        rb.push_slice(&[10.0, 20.0, 30.0, 40.0]);
        rb.push(50.0); // overwrites 10.0

        let mut out = [0.0f32; 4];
        rb.read_ordered(&mut out);
        assert_eq!(out, [20.0, 30.0, 40.0, 50.0]);
    }

    #[test]
    fn test_read_ordered_partial() {
        let mut rb = RingBuffer::new(8);
        rb.push_slice(&[1.0, 2.0, 3.0]);

        let mut out = [0.0f32; 5];
        rb.read_ordered(&mut out);
        assert_eq!(out, [1.0, 2.0, 3.0, 0.0, 0.0]);
    }

    #[test]
    fn test_clear() {
        let mut rb = RingBuffer::new(4);
        rb.push_slice(&[1.0, 2.0, 3.0]);
        rb.clear();
        assert!(rb.is_empty());
        assert_eq!(rb.read_delay(0), 0.0);
    }
}
