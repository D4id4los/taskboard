// SPDX-License-Identifier: AGPL-3.0-only
//! Length-prefixed JSON framing (ADR 0012 §4.3): a `u32`
//! little-endian byte length, then exactly that many UTF-8 JSON bytes.
//! One frame each way per connection; the writer flushes after the
//! frame. Over-length, truncated, and undecodable frames are typed
//! transport failures — there is nothing salvageable in a garbage
//! stream, so the reader just reports and the connection is dropped.

/// Request cap: argv words for one invocation.
pub const MAX_REQUEST_BYTES: u32 = 64 * 1024;
/// Response cap: rendered output of one invocation (a `tasks list` of a
/// very large board).
pub const MAX_RESPONSE_BYTES: u32 = 16 * 1024 * 1024;

/// Framing failure classes (typed; kind-mapped client-side).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The frame announced more bytes than the direction's cap.
    #[error("frame exceeds the size cap")]
    TooLarge,
    /// The peer closed mid-frame (or sent no frame at all).
    #[error("unexpected end of stream")]
    Eof,
    /// The socket failed.
    #[error("socket i/o failed")]
    Io(#[from] std::io::Error),
    /// The frame's bytes are not UTF-8.
    #[error("frame is not utf-8")]
    Utf8,
}

/// Writes one framed message: `u32` LE length + bytes, then flush.
///
/// # Errors
///
/// [`Error::TooLarge`] when `bytes` exceeds `cap`; [`Error::Io`] on
/// socket failure.
pub async fn write_frame<W: tokio::io::AsyncWrite + Unpin>(
    writer: &mut W,
    bytes: &[u8],
    cap: u32,
) -> Result<(), Error> {
    use tokio::io::AsyncWriteExt as _;
    let len = u32::try_from(bytes.len()).map_err(|_| Error::TooLarge)?;
    if len > cap {
        return Err(Error::TooLarge);
    }
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(bytes).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads one framed message (length + bytes), enforcing `cap` and
/// UTF-8.
///
/// # Errors
///
/// [`Error::Eof`] on a clean close before any byte or mid-frame;
/// [`Error::TooLarge`] on an over-cap length; [`Error::Utf8`] on
/// non-UTF-8 frame bytes; [`Error::Io`] on socket failure.
///
/// # Panics
///
/// Never: the buffer length is a capped `u32` conversion that always
/// fits `usize`.
pub async fn read_frame<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
    cap: u32,
) -> Result<Vec<u8>, Error> {
    use tokio::io::AsyncReadExt as _;
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes).await {
        Ok(_n) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Err(Error::Eof),
        Err(err) => return Err(err.into()),
    }
    let len = u32::from_le_bytes(len_bytes);
    if len > cap {
        return Err(Error::TooLarge);
    }
    let mut bytes = vec![0u8; usize::try_from(len).expect("u32 fits usize")];
    match reader.read_exact(&mut bytes).await {
        Ok(_n) => {}
        Err(err) if err.kind() == std::io::ErrorKind::UnexpectedEof => return Err(Error::Eof),
        Err(err) => return Err(err.into()),
    }
    if std::str::from_utf8(&bytes).is_err() {
        return Err(Error::Utf8);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{Error, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, read_frame, write_frame};
    use tokio::io::DuplexStream;

    /// A memory pipe pair with a payload buffer comfortably above the
    /// test frame sizes.
    fn pipe() -> (DuplexStream, DuplexStream) {
        tokio::io::duplex(4096)
    }

    #[tokio::test]
    async fn frames_round_trip_both_directions() {
        for cap in [MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES] {
            let (mut tx, mut rx) = pipe();
            let payload = b"{\"hello\":[1,2,3]}".to_vec();
            write_frame(&mut tx, &payload, cap).await.expect("write");
            let back = read_frame(&mut rx, cap).await.expect("read");
            assert_eq!(back, payload);
        }
    }

    #[tokio::test]
    async fn empty_frames_round_trip() {
        let (mut tx, mut rx) = pipe();
        write_frame(&mut tx, b"", MAX_REQUEST_BYTES)
            .await
            .expect("write");
        assert_eq!(
            read_frame(&mut rx, MAX_REQUEST_BYTES).await.expect("read"),
            Vec::<u8>::new()
        );
    }

    #[tokio::test]
    async fn over_cap_frames_are_refused_before_the_wire() {
        let (mut tx, _rx) = pipe();
        let big = vec![b'x'; usize::try_from(MAX_REQUEST_BYTES).expect("fits") + 1];
        assert!(matches!(
            write_frame(&mut tx, &big, MAX_REQUEST_BYTES).await,
            Err(Error::TooLarge)
        ));
    }

    #[tokio::test]
    async fn an_announced_over_cap_length_is_refused_without_reading_the_body() {
        use tokio::io::AsyncWriteExt as _;
        let (mut tx, mut rx) = pipe();
        let over = MAX_RESPONSE_BYTES + 1;
        tx.write_all(&over.to_le_bytes()).await.expect("len");
        // No body follows; the reader must refuse on the length alone.
        drop(tx);
        assert!(matches!(
            read_frame(&mut rx, MAX_RESPONSE_BYTES).await,
            Err(Error::TooLarge)
        ));
    }

    #[tokio::test]
    async fn a_truncated_frame_is_an_eof_error() {
        use tokio::io::AsyncWriteExt as _;
        let (mut tx, mut rx) = pipe();
        tx.write_all(&16u32.to_le_bytes()).await.expect("len");
        tx.write_all(b"short").await.expect("short body");
        drop(tx);
        assert!(matches!(
            read_frame(&mut rx, MAX_REQUEST_BYTES).await,
            Err(Error::Eof)
        ));
    }

    #[tokio::test]
    async fn a_close_before_any_byte_is_an_eof_error() {
        let (tx, mut rx) = pipe();
        drop(tx);
        assert!(matches!(
            read_frame(&mut rx, MAX_REQUEST_BYTES).await,
            Err(Error::Eof)
        ));
    }

    #[tokio::test]
    async fn non_utf8_frames_are_typed_utf8_errors() {
        use tokio::io::AsyncWriteExt as _;
        let (mut tx, mut rx) = pipe();
        tx.write_all(&2u32.to_le_bytes()).await.expect("len");
        tx.write_all(&[0xff, 0xfe]).await.expect("body");
        drop(tx);
        assert!(matches!(
            read_frame(&mut rx, MAX_REQUEST_BYTES).await,
            Err(Error::Utf8)
        ));
    }
}
