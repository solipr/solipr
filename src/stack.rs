//! Defines the [`StackVec`] type that is like a standard [Vec] but
//! that is stored on the stack.
//!
//! It works by storing the elements in a fixed size array on the stack along
//! with the number of elements.
//!
//! The type stored in the array must be copyable because it allows some
//! optimizations.
//!
//! The maximum number of elements must be less or equal to 255 because the
//! array size is stored as a [u8].

use std::array::from_fn;
use std::fmt::{self, Debug};
use std::hash::{Hash, Hasher};
use std::io::{self, Read};
use std::mem::MaybeUninit;
use std::ops::{Deref, DerefMut};
use std::slice::{from_raw_parts, from_raw_parts_mut};

use borsh::{BorshDeserialize, BorshSerialize};

/// A [Vec] that is stored on the stack. The maximum number of elements is
/// limited to `MAX`.
///
/// The type `T` must be copyable and `MAX` must be less or equal to `255`.
///
/// # Example
///
/// ```rust
/// use solipr::stack::StackVec;
///
/// let mut numbers = StackVec::from([1, 2, 3]);
///
/// assert_eq!(format!("{:?}", numbers), "[1, 2, 3]");
///
/// assert_eq!(numbers.get(0), Some(1));
/// assert_eq!(numbers.get(1), Some(2));
/// assert_eq!(numbers.get(2), Some(3));
/// assert_eq!(numbers.get(3), None);
///
/// assert_eq!(numbers.pop(), Some(3));
/// assert_eq!(format!("{:?}", numbers), "[1, 2]");
///
/// assert_eq!(numbers.push(4), None);
/// assert_eq!(format!("{:?}", numbers), "[1, 2, 4]");
///
/// numbers.clear();
/// assert_eq!(format!("{:?}", numbers), "[]");
/// ```
#[derive(Clone, Copy)]
pub struct StackVec<T: Copy, const MAX: usize> {
    /// The number of elements in the [`StackVec`].
    len: u8,

    /// The elements of the [`StackVec`].
    data: [MaybeUninit<T>; MAX],
}

impl<T: Copy, const MAX: usize> StackVec<T, MAX> {
    /// Create a new empty [`StackVec`].
    ///
    /// # Panics
    ///
    /// Panics if `MAX` is bigger than 255.
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let numbers = StackVec::<i32, 3>::new();
    ///
    /// assert_eq!(format!("{:?}", numbers), "[]");
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        assert!(MAX <= 255, "MAX must be less or equal to 255");

        Self {
            len: 0,
            // SAFETY:
            // Uninitialized values will not be read because the the length is initialized to 0.
            data: unsafe { MaybeUninit::uninit().assume_init() },
        }
    }

    /// Return `true` if the [`StackVec`] is empty, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::from([1, 2, 3]);
    ///
    /// assert_eq!(numbers.is_empty(), false);
    ///
    /// numbers.clear();
    ///
    /// assert_eq!(numbers.is_empty(), true);
    /// ```
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Return `true` if the [`StackVec`] is full, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::<i32, 2>::new();
    ///
    /// assert_eq!(numbers.is_full(), false);
    ///
    /// numbers.push(1);
    ///
    /// assert_eq!(numbers.is_full(), false);
    ///
    /// numbers.push(2);
    ///
    /// assert_eq!(numbers.is_full(), true);
    /// ```
    pub const fn is_full(&self) -> bool {
        self.len as usize == MAX
    }

    /// Return the number of elements in the [`StackVec`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::from([1, 2, 3]);
    ///
    /// assert_eq!(numbers.len(), 3);
    ///
    /// numbers.pop();
    ///
    /// assert_eq!(numbers.len(), 2);
    /// ```
    pub const fn len(&self) -> usize {
        self.len as usize
    }

    /// Get an element from the [`StackVec`].
    ///
    /// Returns [None] if the index is out of bounds.
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::from([1, 2, 3]);
    ///
    /// assert_eq!(numbers.get(0), Some(1));
    /// assert_eq!(numbers.get(1), Some(2));
    /// assert_eq!(numbers.get(2), Some(3));
    /// assert_eq!(numbers.get(3), None);
    /// ```
    pub const fn get(&self, index: usize) -> Option<T> {
        if index >= self.len as usize {
            return None;
        }
        #[expect(clippy::indexing_slicing, reason = "the index is checked before")]
        // SAFETY:
        // The index is checked to be in bounds. And all values from 0 to len are
        // initialized.
        Some(unsafe { self.data[index].assume_init() })
    }

    /// Push a new element to the [`StackVec`].
    ///
    /// Returns [Some] with the value if the [`StackVec`] is full and [None] if
    /// the value was pushed.
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::<i32, 3>::new();
    ///
    /// assert_eq!(format!("{:?}", numbers), "[]");
    ///
    /// assert_eq!(numbers.push(1), None);
    /// assert_eq!(format!("{:?}", numbers), "[1]");
    ///
    /// assert_eq!(numbers.push(2), None);
    /// assert_eq!(format!("{:?}", numbers), "[1, 2]");
    ///
    /// assert_eq!(numbers.push(3), None);
    /// assert_eq!(format!("{:?}", numbers), "[1, 2, 3]");
    ///
    /// assert_eq!(numbers.push(4), Some(4));
    /// assert_eq!(format!("{:?}", numbers), "[1, 2, 3]");
    /// ```
    pub const fn push(&mut self, value: T) -> Option<T> {
        if (self.len as usize) < MAX {
            self.len = self.len.saturating_add(1);
            #[expect(
                clippy::indexing_slicing,
                clippy::arithmetic_side_effects,
                reason = "self.len is always greater than 1 and less or equal to MAX, self.len - \
                          1 is always in bounds"
            )]
            self.data[self.len as usize - 1] = MaybeUninit::new(value);
            None
        } else {
            Some(value)
        }
    }

    /// Pop an element from the [`StackVec`].
    ///
    /// Returns [None] if the [`StackVec`] is empty.
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::from([1, 2, 3]);
    ///
    /// assert_eq!(numbers.pop(), Some(3));
    /// assert_eq!(format!("{:?}", numbers), "[1, 2]");
    ///
    /// assert_eq!(numbers.pop(), Some(2));
    /// assert_eq!(format!("{:?}", numbers), "[1]");
    ///
    /// assert_eq!(numbers.pop(), Some(1));
    /// assert_eq!(format!("{:?}", numbers), "[]");
    ///
    /// assert_eq!(numbers.pop(), None);
    /// assert_eq!(format!("{:?}", numbers), "[]");
    /// ```
    pub const fn pop(&mut self) -> Option<T> {
        if self.len > 0 {
            self.len = self.len.saturating_sub(1);
            #[expect(clippy::indexing_slicing, reason = "self.len - 1 is always in bounds")]
            // SAFETY:
            // When self.len > 0, the value at self.len - 1 is initialized.
            Some(unsafe { self.data[self.len as usize].assume_init() })
        } else {
            None
        }
    }

    /// Remove an element from the [`StackVec`].
    ///
    /// Does nothing if the index is out of bounds.
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::from([1, 2, 3, 4, 5]);
    ///
    /// assert_eq!(numbers.remove(0), Some(1));
    /// assert_eq!(format!("{:?}", numbers), "[2, 3, 4, 5]");
    ///
    /// assert_eq!(numbers.remove(2), Some(4));
    /// assert_eq!(format!("{:?}", numbers), "[2, 3, 5]");
    ///
    /// assert_eq!(numbers.remove(87), None);
    /// assert_eq!(format!("{:?}", numbers), "[2, 3, 5]");
    /// ```
    pub const fn remove(&mut self, mut index: usize) -> Option<T> {
        if index >= self.len as usize {
            return None;
        }
        #[expect(
            clippy::indexing_slicing,
            reason = "index is checked to be in bounds before"
        )]
        // SAFETY:
        // All values at index smaller than self.len are initialized.
        let result = unsafe { self.data[index].assume_init() };
        while index < (self.len as usize).saturating_sub(1) {
            #[expect(
                clippy::indexing_slicing,
                reason = "index is smaller than self.len - 1 and so it is always in bounds"
            )]
            self.data[index] = {
                #[expect(
                    clippy::indexing_slicing,
                    reason = "index cannot be bigger than self.len - 2 and so index + 1 is always \
                              in bounds"
                )]
                self.data[index.saturating_add(1)]
            };
            index = index.saturating_add(1);
        }
        self.len = self.len.saturating_sub(1);
        Some(result)
    }

    /// Clear the [`StackVec`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::from([1, 2, 3, 4, 5]);
    ///
    /// assert_eq!(format!("{:?}", numbers), "[1, 2, 3, 4, 5]");
    /// assert_eq!(numbers.len(), 5);
    ///
    /// numbers.clear();
    ///
    /// assert_eq!(format!("{:?}", numbers), "[]");
    /// assert_eq!(numbers.len(), 0);
    /// ```
    pub const fn clear(&mut self) {
        self.len = 0;
    }

    /// Returns a slice representing the [`StackVec`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let numbers = StackVec::from([1, 2, 3, 4, 5]);
    ///
    /// assert_eq!(format!("{:?}", numbers.as_slice()), "[1, 2, 3, 4, 5]");
    /// ```
    pub const fn as_slice(&self) -> &[T] {
        // SAFETY:
        // All values from 0 to self.len are initialized.
        unsafe { from_raw_parts(self.data.as_ptr().cast::<T>(), self.len as usize) }
    }

    /// Returns a mutable slice representing the [`StackVec`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let mut numbers = StackVec::from([1, 2, 3, 4, 5]);
    ///
    /// let slice = numbers.as_mut_slice();
    ///
    /// slice[2] = 42;
    ///
    /// assert_eq!(format!("{:?}", numbers.as_mut_slice()), "[1, 2, 42, 4, 5]");
    /// ```
    pub const fn as_mut_slice(&mut self) -> &mut [T] {
        // SAFETY:
        // All values from 0 to self.len are initialized.
        unsafe { from_raw_parts_mut(self.data.as_mut_ptr().cast::<T>(), self.len as usize) }
    }

    /// Returns an iterator over the [`StackVec`].
    ///
    /// # Example
    ///
    /// ```rust
    /// use solipr::stack::StackVec;
    ///
    /// let numbers = StackVec::from([1, 2, 3, 4, 5]);
    ///
    /// let mut iter = numbers.iter();
    ///
    /// assert_eq!(iter.next(), Some(1));
    /// assert_eq!(iter.next(), Some(2));
    /// assert_eq!(iter.next(), Some(3));
    /// assert_eq!(iter.next(), Some(4));
    /// assert_eq!(iter.next(), Some(5));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub const fn iter(&self) -> StackVecIter<T, MAX> {
        StackVecIter {
            vec: self,
            index: 0,
        }
    }
}

impl<T: Default + Copy, const MAX: usize> Default for StackVec<T, MAX> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy, const MAX: usize> From<[T; MAX]> for StackVec<T, MAX> {
    fn from(value: [T; MAX]) -> Self {
        assert!(value.len() <= MAX, "MAX cannot be bigger than 255");

        Self {
            #[expect(clippy::cast_possible_truncation, reason = "MAX is smaller than 256")]
            len: MAX as u8,

            #[expect(clippy::indexing_slicing, reason = "value contains MAX elements")]
            data: from_fn(|i| MaybeUninit::new(value[i])),
        }
    }
}

impl<T: Copy + Debug, const MAX: usize> Debug for StackVec<T, MAX> {
    #[expect(clippy::min_ident_chars, reason = "The trait is made that way")]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries((0..self.len).map(|i| {
                #[expect(
                    clippy::indexing_slicing,
                    reason = "All values from 0 to self.len are in bounds"
                )]
                // SAFETY:
                // All values from 0 to self.len are initialized.
                unsafe {
                    self.data[i as usize].assume_init()
                }
            }))
            .finish()
    }
}

impl<T: PartialEq + Copy, const A: usize, const B: usize> PartialEq<StackVec<T, B>>
    for StackVec<T, A>
{
    fn eq(&self, other: &StackVec<T, B>) -> bool {
        self.len == other.len && {
            for i in 0..self.len as usize {
                #[expect(
                    clippy::indexing_slicing,
                    reason = "Both StackVec have the same length and all values from 0 to \
                              self.len are in bounds"
                )]
                // SAFETY:
                // All values from 0 to self.len are initialized.
                if unsafe { self.data[i].assume_init() } != unsafe { other.data[i].assume_init() } {
                    return false;
                }
            }
            true
        }
    }
}

impl<T: Copy + Eq, const MAX: usize> Eq for StackVec<T, MAX> {}

impl<T: Copy + Hash, const MAX: usize> Hash for StackVec<T, MAX> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for i in 0..self.len as usize {
            #[expect(
                clippy::indexing_slicing,
                reason = "All values from 0 to self.len are in bounds"
            )]
            // SAFETY:
            // All values from 0 to self.len are initialized.
            unsafe { self.data[i].assume_init() }.hash(state);
        }
    }
}

impl<T: Copy, const MAX: usize> Deref for StackVec<T, MAX> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T: Copy, const MAX: usize> DerefMut for StackVec<T, MAX> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T: Copy + BorshDeserialize, const MAX: usize> BorshDeserialize for StackVec<T, MAX> {
    fn deserialize_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
        assert!(MAX <= 255, "MAX cannot be bigger than 255");
        let len = u8::deserialize_reader(reader)?;
        if len as usize > MAX {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("len ({len}) cannot be bigger than MAX ({MAX})"),
            ));
        }
        let mut vec = Self::new();
        for _ in 0..len {
            vec.push(T::deserialize_reader(reader)?);
        }
        Ok(vec)
    }
}

impl<T: Copy + BorshSerialize, const MAX: usize> BorshSerialize for StackVec<T, MAX> {
    fn serialize<W: io::Write>(&self, writer: &mut W) -> io::Result<()> {
        self.len.serialize(writer)?;
        for i in 0..self.len as usize {
            #[expect(
                clippy::indexing_slicing,
                reason = "All values from 0 to self.len are in bounds"
            )]
            // SAFETY:
            // All values from 0 to self.len are initialized.
            unsafe { self.data[i].assume_init() }.serialize(writer)?;
        }
        Ok(())
    }
}

impl<'vec, T: Copy, const MAX: usize> IntoIterator for &'vec StackVec<T, MAX> {
    type Item = T;

    type IntoIter = StackVecIter<'vec, T, MAX>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<T: Copy, const MAX: usize> IntoIterator for StackVec<T, MAX> {
    type Item = T;

    type IntoIter = StackVecIntoIter<T, MAX>;

    fn into_iter(self) -> Self::IntoIter {
        StackVecIntoIter {
            data: self.data,
            len: self.len,
            index: 0,
        }
    }
}

/// An iterator over the elements of a [`StackVec`].
pub struct StackVecIter<'vec, T: Copy, const MAX: usize> {
    /// The [`StackVec`] being iterated over.
    vec: &'vec StackVec<T, MAX>,

    /// The current index of the iterator.
    index: u8,
}

impl<T: Copy, const MAX: usize> Iterator for StackVecIter<'_, T, MAX> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        (self.index < self.vec.len).then(|| {
            #[expect(
                clippy::indexing_slicing,
                reason = "All values from 0 to self.vec.len are in bounds"
            )]
            // SAFETY:
            // All values from 0 to self.vec.len are initialized.
            let value = unsafe { self.vec.data[self.index as usize].assume_init() };
            self.index = self.index.saturating_add(1);
            value
        })
    }
}

/// An owned iterator over the elements of a [`StackVec`].
pub struct StackVecIntoIter<T: Copy, const MAX: usize> {
    /// The data of the [`StackVec`].
    data: [MaybeUninit<T>; MAX],

    /// The length of the [`StackVec`].
    len: u8,

    /// The current index of the iterator.
    index: u8,
}

impl<T: Copy, const MAX: usize> Iterator for StackVecIntoIter<T, MAX> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        (self.index < self.len).then(|| {
            #[expect(
                clippy::indexing_slicing,
                reason = "All values from 0 to self.vec.len are in bounds"
            )]
            // SAFETY:
            // All values from 0 to self.vec.len are initialized.
            let value = unsafe { self.data[self.index as usize].assume_init() };
            self.index = self.index.saturating_add(1);
            value
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_eq() {
        let mut vec1 = StackVec::<i32, 3>::from([1_i32, 2_i32, 3_i32]);
        let mut vec2 = StackVec::<i32, 3>::from([1_i32, 2_i32, 3_i32]);
        let mut vec3 = StackVec::<i32, 5>::from([1_i32, 2_i32, 3_i32, 4_i32, 5_i32]);
        assert_eq!(vec1, vec2);
        assert_ne!(vec1, vec3);

        assert_eq!(vec2.pop(), Some(3_i32));
        assert_ne!(vec1, vec2);
        assert_eq!(vec1.pop(), Some(3_i32));
        assert_eq!(vec1, vec2);

        assert_eq!(vec3.pop(), Some(5_i32));
        assert_eq!(vec3.pop(), Some(4_i32));
        assert_eq!(vec3.pop(), Some(3_i32));
        assert_eq!(vec1, vec3);
    }

    #[test]
    fn into_iter() {
        let numbers = StackVec::<i32, 3>::from([1_i32, 2_i32, 3_i32]);

        let mut iter = numbers.into_iter();
        assert_eq!(iter.next(), Some(1_i32));
        assert_eq!(iter.next(), Some(2_i32));
        assert_eq!(iter.next(), Some(3_i32));
        assert_eq!(iter.next(), None);
    }
}
