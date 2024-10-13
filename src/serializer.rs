//! Defines the [`Serializable`] and [`Deserializable`] traits that can be used
//! to serialize and deserialize objects to and from an `AsyncRead` or
//! `AsyncWrite`.

use core::slice;
use std::io;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Serializable objects can be serialized to an `AsyncWrite`.
#[async_trait]
pub trait Serializable {
    /// Serialize the object to the given writer.
    ///
    /// # Errors
    ///
    /// This function will return an error if an I/O error occurs.
    async fn serialize(&self, writer: impl AsyncWrite + Unpin + Send) -> io::Result<()>;

    /// Returns the serialized length of the object.
    async fn serialize_len(&self) -> usize;
}

/// Implement `Serializable` for all integer type.
macro_rules! impl_serializable_for_integers {
    ($($t:ty)*) => {$(
        #[async_trait]
        impl Serializable for $t {
            #[inline]
            async fn serialize(
                &self,
                mut writer: impl AsyncWrite + Unpin + Send
            ) -> io::Result<()> {
                writer.write_all(self.to_be_bytes().as_ref()).await
            }

            #[inline]
            async fn serialize_len(&self) -> usize {
                <$t>::BITS as usize / 8
            }
        }
    )*};
}

impl_serializable_for_integers!(i8 i16 i32 i64 i128 u8 u16 u32 u64 u128);

#[async_trait]
impl Serializable for bool {
    #[inline]
    async fn serialize(&self, mut writer: impl AsyncWrite + Unpin + Send) -> io::Result<()> {
        writer.write_all(slice::from_ref(&u8::from(*self))).await
    }

    #[inline]
    async fn serialize_len(&self) -> usize {
        1
    }
}

/// Deserializable objects can be deserialized from an `AsyncRead`.
#[async_trait]
pub trait Deserializable: Sized {
    /// Deserialize the object from the given reader.
    ///
    /// # Errors
    ///
    /// This function will return an error if an I/O error occurs or if the
    /// object cannot be deserialized from the given reader.
    async fn deserialize(reader: impl AsyncRead + Unpin + Send) -> io::Result<Self>;
}

/// Implement `Deserializable` for all integer type.
macro_rules! impl_deserializable_for_integers {
    ($($t:ty)*) => {$(
        #[async_trait]
        impl Deserializable for $t {
            #[inline]
            async fn deserialize(mut reader: impl AsyncRead + Unpin + Send) -> io::Result<Self> {
                let mut buf = [0_u8; <$t>::BITS as usize / 8];
                reader.read_exact(&mut buf).await?;
                Ok(<$t>::from_be_bytes(buf))
            }
        }
    )*};
}

impl_deserializable_for_integers!(i8 i16 i32 i64 i128 u8 u16 u32 u64 u128);

#[async_trait]
impl Deserializable for bool {
    #[inline]
    async fn deserialize(mut reader: impl AsyncRead + Unpin + Send) -> io::Result<Self> {
        let mut buf = [0_u8; 1];
        reader.read_exact(&mut buf).await?;
        Ok(buf[0] != 0)
    }
}
