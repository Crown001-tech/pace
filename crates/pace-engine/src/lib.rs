//! pace-engine: pure calculations for timing, motion, effects, and frame plans.
//!
//! Zero file, network, clock, or database I/O to guarantee deterministic execution
//! natively and when compiled to WebAssembly.

pub mod effects;
pub mod fit;
pub mod motion;
pub mod segmentation;
pub mod segments;
pub mod spec;
pub mod stickers;
pub mod subtitles;
pub mod timing;

pub fn engine_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_version_available() {
        assert_eq!(engine_version(), "0.1.0");
    }

    #[test]
    fn test_first_passing_sanity_check() {
        assert_eq!(2 + 2, 4);
    }
}
