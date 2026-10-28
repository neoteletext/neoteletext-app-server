//! The server's half of WebSocket framing (RFC 6455 section 5), without
//! input or output: a reader of the client's masked frames and the
//! header of the server's unmasked ones. The engine has the client's half
//! and the handshake (`neoteletext_web::ws`); this is the half a server
//! needs (`SERVER.md` section 3.2 item 4).

/// The largest message a client may send: INPUT.md's 4,096-byte message
/// with room for a close's reason.
pub const MAX_MESSAGE: usize = 8 * 1024;
/// A Close's status: the endpoint closes because the other broke the
/// protocol (RFC 6455 section 7.4.1).
pub const CLOSE_PROTOCOL_ERROR: u16 = 1002;
/// A Close's status: the service is restarting, and the client may
/// connect again (the IANA WebSocket Close Code registry; requirement 5.1).
pub const CLOSE_SERVICE_RESTART: u16 = 1012;
/// The largest control frame's payload (RFC 6455 section 5.5).
const MAX_CONTROL: usize = 125;
/// A 16-bit length follows (RFC 6455 section 5.2).
const LENGTH_16: u8 = 126;
/// A 64-bit length follows.
const LENGTH_64: u8 = 127;
pub(crate) const FIN: u8 = 0x80;
const RSV: u8 = 0x70;
const OPCODE: u8 = 0x0F;
pub(crate) const MASKED: u8 = 0x80;
const LENGTH_7: u8 = 0x7F;
const MASK_BYTES: usize = 4;

/// A frame's kind (RFC 6455 section 5.2).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opcode {
    /// A continuation of a fragmented message.
    Continuation = 0x0,
    /// Text.
    Text = 0x1,
    /// Binary.
    Binary = 0x2,
    /// Close.
    Close = 0x8,
    /// Ping.
    Ping = 0x9,
    /// Pong.
    Pong = 0xA,
}

impl Opcode {
    fn of(bits: u8) -> Option<Self> {
        Some(match bits {
            0x0 => Self::Continuation,
            0x1 => Self::Text,
            0x2 => Self::Binary,
            0x8 => Self::Close,
            0x9 => Self::Ping,
            0xA => Self::Pong,
            _ => return None,
        })
    }

    const fn is_control(self) -> bool {
        matches!(self, Self::Close | Self::Ping | Self::Pong)
    }
}

/// A whole message from the client.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Message {
    /// Text, such as the engine's `subscribe all`.
    Text(Vec<u8>),
    /// Binary: the input stream.
    Binary(Vec<u8>),
    /// A ping, to answer with a pong of the same payload.
    Ping(Vec<u8>),
    /// A pong.
    Pong,
    /// The client closes.
    Close,
}

/// Why the client's bytes were refused: the connection closes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameError {
    /// Reserved bits, an unknown opcode, or an unmasked frame.
    Malformed,
    /// A message or control frame over its limit.
    TooLong,
    /// A continuation with nothing to continue, or a new message inside one.
    Fragmentation,
}

/// Reads the client's frames from the bytes as they arrive.
#[derive(Debug, Default)]
pub struct Reader {
    buffer: Vec<u8>,
    partial: Option<(Opcode, Vec<u8>)>,
}

impl Reader {
    /// A reader.
    #[must_use]
    pub const fn new() -> Self {
        Self { buffer: Vec::new(), partial: None }
    }

    /// Takes more bytes from the connection.
    pub fn feed(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// The next whole message, if its bytes have all arrived.
    ///
    /// # Errors
    ///
    /// When the bytes break RFC 6455 or the limits.
    pub fn next_message(&mut self) -> Result<Option<Message>, FrameError> {
        loop {
            let Some((opcode, fin, payload)) = self.frame()? else { return Ok(None) };
            if opcode.is_control() {
                return Ok(Some(match opcode {
                    Opcode::Ping => Message::Ping(payload),
                    Opcode::Pong => Message::Pong,
                    _ => Message::Close,
                }));
            }
            let (kind, mut data) = match (opcode, self.partial.take()) {
                (Opcode::Continuation, Some((kind, mut data))) => {
                    data.extend_from_slice(&payload);
                    (kind, data)
                }
                (Opcode::Continuation, None) | (_, Some(_)) => return Err(FrameError::Fragmentation),
                (kind, None) => (kind, payload),
            };
            if data.len() > MAX_MESSAGE {
                return Err(FrameError::TooLong);
            }
            if !fin {
                self.partial = Some((kind, core::mem::take(&mut data)));
                continue;
            }
            return Ok(Some(if kind == Opcode::Text { Message::Text(data) } else { Message::Binary(data) }));
        }
    }

    /// One frame, unmasked, taken from the buffer.
    fn frame(&mut self) -> Result<Option<(Opcode, bool, Vec<u8>)>, FrameError> {
        let (Some(&first), Some(&second)) = (self.buffer.first(), self.buffer.get(1)) else { return Ok(None) };
        if first & RSV != 0 || second & MASKED == 0 {
            return Err(FrameError::Malformed);
        }
        let opcode = Opcode::of(first & OPCODE).ok_or(FrameError::Malformed)?;
        let fin = first & FIN != 0;
        let (length, mut at) = match second & LENGTH_7 {
            LENGTH_16 => {
                let Some(bytes) = self.buffer.get(2..4) else { return Ok(None) };
                (usize::from(u16::from_be_bytes([bytes.first().copied().unwrap_or(0), bytes.get(1).copied().unwrap_or(0)])), 4_usize)
            }
            LENGTH_64 => {
                let Some(bytes) = self.buffer.get(2..10) else { return Ok(None) };
                let mut word = [0_u8; 8];
                word.copy_from_slice(bytes);
                (usize::try_from(u64::from_be_bytes(word)).unwrap_or(usize::MAX), 10_usize)
            }
            short => (usize::from(short), 2_usize),
        };
        if length > MAX_MESSAGE || (opcode.is_control() && (length > MAX_CONTROL || !fin)) {
            return Err(FrameError::TooLong);
        }
        let Some(mask) = self.buffer.get(at..at.saturating_add(MASK_BYTES)) else { return Ok(None) };
        let mask =
            [mask.first().copied().unwrap_or(0), mask.get(1).copied().unwrap_or(0), mask.get(2).copied().unwrap_or(0), mask.get(3).copied().unwrap_or(0)];
        at = at.saturating_add(MASK_BYTES);
        let end = at.saturating_add(length);
        let Some(masked) = self.buffer.get(at..end) else { return Ok(None) };
        let payload: Vec<u8> = masked.iter().zip(mask.iter().cycle()).map(|(&byte, &key)| byte ^ key).collect();
        self.buffer.drain(..end);
        Ok(Some((opcode, fin, payload)))
    }
}

/// The header of a server frame, unmasked, with FIN set and the shortest
/// length form (the engine's client refuses any other): the bytes and how
/// many of them.
#[must_use]
pub fn header(opcode: Opcode, length: usize) -> ([u8; 10], usize) {
    let mut bytes = [0_u8; 10];
    bytes[0] = FIN | opcode as u8;
    match u16::try_from(length) {
        Ok(short) if short < u16::from(LENGTH_16) => {
            bytes[1] = u8::try_from(short).unwrap_or(0);
            (bytes, 2)
        }
        Ok(medium) => {
            bytes[1] = LENGTH_16;
            let [high, low] = medium.to_be_bytes();
            bytes[2] = high;
            bytes[3] = low;
            (bytes, 4)
        }
        Err(_) => {
            bytes[1] = LENGTH_64;
            for (slot, byte) in bytes.iter_mut().skip(2).zip((length as u64).to_be_bytes()) {
                *slot = byte;
            }
            (bytes, 10)
        }
    }
}

#[cfg(test)]
mod tests;
