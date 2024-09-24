//! An example Rust library project

/// Adds two numbers together
#[must_use]
#[inline]
pub const fn add(left: u64, right: u64) -> u64 {
    left.wrapping_add(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(add(2, 2), 4);
        assert_eq!(add(u64::MAX, 3), 2);
    }
}
