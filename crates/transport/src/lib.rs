//! Transport-agnostic contracts and state transitions for NexDeck.
//!
//! Application logic interacts exclusively with the [`Transport`] trait and
//! [`Envelope`] framing without coupling to Wi-Fi WebSocket, USB, or Bluetooth
//! specifics.

use std::fmt;

/// Protocol version supported by the Stage 0 transport contract.
pub const PROTOCOL_VERSION: u16 = 1;

/// Physical or logical channel used to exchange messages with a mobile client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransportKind {
    /// Local network WebSocket transport (MVP primary transport).
    WifiWebSocket,
    /// Direct USB / ADB accessory channel (planned Stage 7).
    Usb,
    /// Bluetooth channel (planned Stage 7).
    Bluetooth,
}

/// Lifecycle state of an abstract transport connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportState {
    /// No active connection to a peer device.
    Disconnected,
    /// Handshake or pairing negotiation in progress.
    Connecting,
    /// Authenticated channel ready to send and receive frames.
    Connected,
    /// Temporary loss of connectivity while attempting recovery.
    Reconnecting,
}

/// Logical channel topic carried inside a transport [`Envelope`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageKind {
    /// System telemetry frame.
    Telemetry,
    /// Control command triggered from a client panel.
    ControlCommand,
    /// Notification center event.
    Notification,
    /// Remote approval prompt or user decision.
    Approval,
    /// Heartbeat / keepalive frame.
    Heartbeat,
}

/// Framed message transferred across any [`Transport`] implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    /// Wire protocol version.
    pub protocol_version: u16,
    /// Correlation identifier unique per session.
    pub message_id: u64,
    /// Logical category of the payload.
    pub kind: MessageKind,
    /// Serialized payload bytes.
    pub payload: Vec<u8>,
}

impl Envelope {
    /// Constructs a new envelope using [`PROTOCOL_VERSION`].
    #[must_use]
    pub fn new(message_id: u64, kind: MessageKind, payload: Vec<u8>) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            message_id,
            kind,
            payload,
        }
    }

    /// Validates protocol version compatibility and bounded payload invariants.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::UnsupportedProtocol`] if `protocol_version` does
    /// not match [`PROTOCOL_VERSION`], or [`TransportError::EmptyPayload`] when
    /// a non-heartbeat frame has an empty payload.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.protocol_version != PROTOCOL_VERSION {
            return Err(TransportError::UnsupportedProtocol(self.protocol_version));
        }
        if self.kind != MessageKind::Heartbeat && self.payload.is_empty() {
            return Err(TransportError::EmptyPayload);
        }
        Ok(())
    }
}

/// Errors returned by transport operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// Attempted to send while disconnected.
    NotConnected,
    /// Received or constructed an unsupported protocol version.
    UnsupportedProtocol(u16),
    /// Non-heartbeat message carried no payload bytes.
    EmptyPayload,
    /// Underlying channel failure.
    ChannelFailure(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "transport is not connected"),
            Self::UnsupportedProtocol(ver) => write!(f, "unsupported protocol version: {ver}"),
            Self::EmptyPayload => write!(f, "non-heartbeat envelope cannot have an empty payload"),
            Self::ChannelFailure(msg) => write!(f, "transport channel error: {msg}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Abstraction implemented by Wi-Fi WebSocket, USB, and Bluetooth backends.
pub trait Transport {
    /// Returns the kind of underlying transport channel.
    fn kind(&self) -> TransportKind;

    /// Returns the current connection state.
    fn state(&self) -> TransportState;

    /// Transitions the transport into an active connected session.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] if establishing the session fails.
    fn connect(&mut self) -> Result<(), TransportError>;

    /// Closes the active transport session gracefully.
    fn disconnect(&mut self);

    /// Sends a validated [`Envelope`] over the active transport.
    ///
    /// # Errors
    ///
    /// Returns [`TransportError`] if the transport is disconnected or the
    /// envelope fails validation.
    fn send(&mut self, envelope: &Envelope) -> Result<(), TransportError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heartbeat_allows_empty_payload() {
        let env = Envelope::new(1, MessageKind::Heartbeat, Vec::new());
        assert_eq!(env.validate(), Ok(()));
    }

    #[test]
    fn telemetry_rejects_empty_payload() {
        let env = Envelope::new(2, MessageKind::Telemetry, Vec::new());
        assert_eq!(env.validate(), Err(TransportError::EmptyPayload));
    }

    #[test]
    fn rejects_unknown_protocol_version() {
        let mut env = Envelope::new(3, MessageKind::Notification, b"{}".to_vec());
        env.protocol_version = 99;
        assert_eq!(env.validate(), Err(TransportError::UnsupportedProtocol(99)));
    }
}
