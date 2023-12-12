#[macro_use]
mod utils;

use std::fmt::{self, Display};

use desert::{FromBytes, ToBytes};
use log::warn;
use snow::{
    Builder as NoiseBuilder, HandshakeState as NoiseHandshakeState,
    TransportState as NoiseTransportState,
};

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, PartialEq)]
pub enum HandshakeError {
    /// The received major server version does not match that of the client.
    // TODO: Add `received` and `expected` context.
    IncompatibleServerVersion,
}

impl std::error::Error for HandshakeError {}

impl Display for HandshakeError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            HandshakeError::IncompatibleServerVersion => {
                write!(
                    f,
                    "Received major server version does not match major client version"
                )
            }
        }
    }
}

/// Size of the version message.
pub const VERSION_BYTES_LEN: usize = 2;

/// Number of bytes that will be written to the `send_buf` and `recv_buf`
/// during the version exchange.
pub const fn version_bytes_len() -> usize {
    VERSION_BYTES_LEN
}

/// The initialization data of a handshake that exists in every state of the
/// handshake.
#[derive(Debug, PartialEq)]
pub struct HandshakeBase {
    version: Version,
    psk: [u8; 32],
    private_key: Vec<u8>,
    // remote_static_key: Option<Vec<u8>>,
}

/// The `Handshake` type maintains the different states that happen in each
/// step of the handshake, allowing it to advance to completion.
///
/// The `Handshake` follows the [typestate pattern](http://cliffle.com/blog/rust-typestate/).
#[derive(Debug, PartialEq)]
pub struct Handshake<S: State> {
    pub base: HandshakeBase,
    pub state: S,
}

// Client states. The client acts as the handshake initiator.

/// The client state that can send the version.
#[derive(Debug)]
pub struct ClientSendVersion;

/// The client state that can receive the version.
#[derive(Debug)]
pub struct ClientRecvVersion;

/// The client state that can build the Noise handshake state machine.
#[derive(Debug)]
pub struct ClientBuildNoiseStateMachine;

/// The client state that can send the ephemeral key.
#[derive(Debug)]
pub struct ClientSendEphemeralKey(NoiseHandshakeState);

/// The client state that can receive the ephemeral and static keys.
#[derive(Debug)]
pub struct ClientRecvEphemeralAndStaticKeys(NoiseHandshakeState);

/// The client state that can send the static key.
#[derive(Debug)]
pub struct ClientSendStaticKey(NoiseHandshakeState);

/// The client state that can initialise transport mode.
#[derive(Debug)]
pub struct ClientInitTransportMode(NoiseHandshakeState);

// Server states. The server acts as the handshake responder.

/// The server state that can receive the version.
#[derive(Debug)]
pub struct ServerRecvVersion;

/// The server state that can receive the version.
#[derive(Debug)]
pub struct ServerSendVersion;

/// The server state that can build the Noise handshake state machine.
#[derive(Debug)]
pub struct ServerBuildNoiseStateMachine;

/// The server state that can receive the ephemeral key.
#[derive(Debug)]
pub struct ServerRecvEphemeralKey(NoiseHandshakeState);

/// The server state that can send the ephemeral and static keys.
#[derive(Debug)]
pub struct ServerSendEphemeralAndStaticKeys(NoiseHandshakeState);

/// The server state that can receive the static key.
#[derive(Debug)]
pub struct ServerRecvStaticKey(NoiseHandshakeState);

/// The server state that can initialise transport mode.
#[derive(Debug)]
pub struct ServerInitTransportMode(NoiseHandshakeState);

// Shared client / server states.

/// The client / server state that has completed the handshake.
#[derive(Debug)]
pub struct HandshakeComplete(NoiseTransportState);

/// The `State` trait is used to implement the typestate pattern for the
/// `Handshake`.
///
/// The state machine is as follows:
///
/// Client:
///
/// - [`ClientSendVersion`] - `send_client_version()` -> [`ClientRecvVersion`]
/// - [`ClientRecvVersion`] - `recv_server_version()` -> [`ClientBuildNoiseStateMachine`]
/// - [`ClientBuildNoiseStateMachine`] - `build_client_noise_state_machine()` -> [`ClientSendEphemeralKey`]
/// - [`ClientSendEphemeralKey`] - `send_client_ephemeral_key()` -> [`ClientRecvEphemeralAndStaticKeys`]
/// - [`ClientRecvEphemeralAndStaticKeys`] - `recv_server_ephemeral_and_static_keys()` -> [`ClientSendStaticKey`]
/// - [`ClientSendStaticKey`] - `send_client_static_key()` -> [`ClientInitTransportMode`]
/// - [`ClientInitTransportMode`] - `init_client_transport_mode()` -> [`HandshakeComplete`]
///
/// Server:
///
/// - [`ServerRecvVersion`] - `recv_client_version()` -> [`ServerSendVersion`]
/// - [`ServerSendVersion`] - `send_server_version()` -> [`ServerBuildNoiseStateMachine`]
/// - [`ServerBuildNoiseStateMachine`] - `build_server_noise_state_machine()` -> [`ServerRecvEphemeralKey`]
/// - [`ServerRecvEphemeralKey`] - `recv_client_ephemeral_key()` -> [`ServerSendEphemeralAndStaticKeys`]
/// - [`ServerSendEphemeralAndStaticKeys`] - `send_server_ephemeral_and_static_keys()` -> [`ServerRecvStaticKey`]
/// - [`ServerRecvStaticKey`] - `recv_client_static_key()` -> [`ServerInitTransportMode`]
/// - [`ServerInitTransportMode`] - `init_server_transport_mode()` -> [`HandshakeComplete`]
pub trait State {}

impl State for ClientSendVersion {}
impl State for ClientRecvVersion {}
impl State for ClientBuildNoiseStateMachine {}
impl State for ClientSendEphemeralKey {}
impl State for ClientRecvEphemeralAndStaticKeys {}
impl State for ClientSendStaticKey {}
impl State for ClientInitTransportMode {}

impl State for ServerRecvVersion {}
impl State for ServerSendVersion {}
impl State for ServerBuildNoiseStateMachine {}
impl State for ServerRecvEphemeralKey {}
impl State for ServerSendEphemeralAndStaticKeys {}
impl State for ServerRecvStaticKey {}
impl State for ServerInitTransportMode {}

impl State for HandshakeComplete {}

#[derive(Debug)]
enum Role {
    Initiator,
    Responder,
}

/// Initialise the Noise handshake state machine according to the given role,
/// ie. initiator or responder.
fn build_noise_state_machine(
    role: Role,
    psk: [u8; 32],
    private_key: Vec<u8>,
) -> Result<NoiseHandshakeState> {
    let handshake_state = match role {
        Role::Initiator => NoiseBuilder::new("Noise_XXpsk0_25519_ChaChaPoly_BLAKE2b".parse()?)
            .local_private_key(&private_key)
            .prologue("CABLE".as_bytes())
            .psk(0, &psk)
            .build_initiator()?,
        Role::Responder => NoiseBuilder::new("Noise_XXpsk0_25519_ChaChaPoly_BLAKE2b".parse()?)
            .local_private_key(&private_key)
            .prologue("CABLE".as_bytes())
            .psk(0, &psk)
            .build_responder()?,
    };

    Ok(handshake_state)
}

// Client state implementations.

impl Handshake<ClientSendVersion> {
    /// Create a new handshake client that can send the version data.
    pub fn new_client(
        version: Version,
        psk: [u8; 32],
        private_key: Vec<u8>,
    ) -> Handshake<ClientSendVersion> {
        let base = HandshakeBase {
            version,
            psk,
            private_key,
        };
        let state = ClientSendVersion;

        Handshake { base, state }
    }

    /// Send the client version data to the server and advance to the next
    /// client state.
    pub fn send_client_version(self, send_buf: &mut [u8]) -> Result<Handshake<ClientRecvVersion>> {
        concat_into!(send_buf, &self.base.version.to_bytes()?);
        let state = ClientRecvVersion;
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ClientRecvVersion> {
    /// Receive the version data from the server and validate it before
    /// advancing to the next client state.
    ///
    /// Terminate the handshake with an error if the major version of the
    /// responder differs from that of the initiator.
    pub fn recv_server_version(
        self,
        recv_buf: &mut [u8],
    ) -> Result<Handshake<ClientBuildNoiseStateMachine>> {
        let (_n, server_version) = Version::from_bytes(recv_buf)?;
        if server_version.major != self.base.version.major {
            warn!("Received incompatible major version from handshake responder");
            return Err(HandshakeError::IncompatibleServerVersion.into());
        }
        let state = ClientBuildNoiseStateMachine;
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ClientBuildNoiseStateMachine> {
    /// Build the Noise handshake state machine for the client with the PSK and
    /// private key.
    fn build_client_noise_state_machine(self) -> Result<Handshake<ClientSendEphemeralKey>> {
        let noise_state_machine = build_noise_state_machine(
            Role::Initiator,
            self.base.psk,
            // TODO: Get rid of clone.
            self.base.private_key.clone(),
        )?;

        let state = ClientSendEphemeralKey(noise_state_machine);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ClientSendEphemeralKey> {
    /// Send the client ephemeral key to the server and advance to the next client state.
    fn send_client_ephemeral_key(
        mut self,
        send_buf: &mut [u8],
    ) -> Result<(usize, Handshake<ClientRecvEphemeralAndStaticKeys>)> {
        // TODO: Figure out the optimal size for the Noise handshake buffer.
        let mut write_buf = [0u8; 1024];

        // Send the client ephemeral key to the server.
        let len = self.state.0.write_message(&[], &mut write_buf)?;

        concat_into!(send_buf, &write_buf[..len]);

        let state = ClientRecvEphemeralAndStaticKeys(self.state.0);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok((len, handshake))
    }
}

impl Handshake<ClientRecvEphemeralAndStaticKeys> {
    /// Receive the ephemeral and static keys from the server and advance to
    /// the next client state.
    fn recv_server_ephemeral_and_static_keys(
        mut self,
        recv_buf: &mut [u8],
    ) -> Result<Handshake<ClientSendStaticKey>> {
        let mut read_buf = [0u8; 1024];

        // Receive the ephemeral and static keys from the server.
        self.state.0.read_message(recv_buf, &mut read_buf)?;

        let state = ClientSendStaticKey(self.state.0);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ClientSendStaticKey> {
    /// Send the client static key to the server and advance to the next client state.
    fn send_client_static_key(
        mut self,
        send_buf: &mut [u8],
    ) -> Result<(usize, Handshake<ClientInitTransportMode>)> {
        let mut write_buf = [0u8; 1024];

        // Send the client static key to the server.
        let len = self.state.0.write_message(&[], &mut write_buf)?;

        concat_into!(send_buf, &write_buf[..len]);

        let state = ClientInitTransportMode(self.state.0);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok((len, handshake))
    }
}

impl Handshake<ClientInitTransportMode> {
    /// Complete the client handshake by initialising the encrypted transport.
    fn init_client_transport_mode(self) -> Result<Handshake<HandshakeComplete>> {
        let transport_state = self.state.0.into_transport_mode()?;

        let state = HandshakeComplete(transport_state);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

// Server state implementations.

impl Handshake<ServerRecvVersion> {
    /// Create a new handshake server that can receive the version data.
    pub fn new_server(
        version: Version,
        psk: [u8; 32],
        private_key: Vec<u8>,
    ) -> Handshake<ServerRecvVersion> {
        let base = HandshakeBase {
            version,
            psk,
            private_key,
        };
        let state = ServerRecvVersion;

        Handshake { base, state }
    }

    /// Receive the version data from the client and validate it before
    /// advancing to the next client state.
    pub fn recv_client_version(self, recv_buf: &mut [u8]) -> Result<Handshake<ServerSendVersion>> {
        let (_n, client_version) = Version::from_bytes(recv_buf)?;
        if client_version.major != self.base.version.major {
            warn!("Received incompatible major version from handshake initiator");
            // There is no error returned here because the server must still
            // respond with it's own version data. The client will then error
            // and terminate the handshake.
        }
        let state = ServerSendVersion;
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ServerSendVersion> {
    /// Send server version data to the client and advance to the next server
    /// state.
    pub fn send_server_version(
        self,
        send_buf: &mut [u8],
    ) -> Result<Handshake<ServerBuildNoiseStateMachine>> {
        concat_into!(send_buf, &self.base.version.to_bytes()?);
        let state = ServerBuildNoiseStateMachine;
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ServerBuildNoiseStateMachine> {
    /// Build the Noise handshake state machine for the server with the PSK and
    /// private key.
    fn build_server_noise_state_machine(self) -> Result<Handshake<ServerRecvEphemeralKey>> {
        let noise_state_machine = build_noise_state_machine(
            Role::Responder,
            self.base.psk,
            self.base.private_key.clone(),
        )?;

        let state = ServerRecvEphemeralKey(noise_state_machine);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ServerRecvEphemeralKey> {
    /// Receive the ephemeral key from the client and advance to the next server state.
    fn recv_client_ephemeral_key(
        mut self,
        recv_buf: &mut [u8],
    ) -> Result<Handshake<ServerSendEphemeralAndStaticKeys>> {
        let mut read_buf = [0u8; 1024];

        // Receive the ephemeral key from the client.
        self.state.0.read_message(recv_buf, &mut read_buf)?;

        let state = ServerSendEphemeralAndStaticKeys(self.state.0);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ServerSendEphemeralAndStaticKeys> {
    /// Send the ephemeral and static keys to the client and advance to
    /// the next server state.
    fn send_server_ephemeral_and_static_keys(
        mut self,
        send_buf: &mut [u8],
    ) -> Result<(usize, Handshake<ServerRecvStaticKey>)> {
        let mut write_buf = [0u8; 1024];

        // Send the ephemeral and static keys to the client.
        let len = self.state.0.write_message(&[], &mut write_buf)?;

        concat_into!(send_buf, &write_buf[..len]);

        let state = ServerRecvStaticKey(self.state.0);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok((len, handshake))
    }
}

impl Handshake<ServerRecvStaticKey> {
    /// Receive the static key from the clientand advance to the next server
    /// state.
    fn recv_client_static_key(
        mut self,
        recv_buf: &mut [u8],
    ) -> Result<Handshake<ServerInitTransportMode>> {
        let mut read_buf = [0u8; 1024];

        // Receive the static key to the client.
        self.state.0.read_message(recv_buf, &mut read_buf)?;

        let state = ServerInitTransportMode(self.state.0);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<ServerInitTransportMode> {
    /// Complete the server handshake by initialising the encrypted transport.
    fn init_server_transport_mode(self) -> Result<Handshake<HandshakeComplete>> {
        let transport_state = self.state.0.into_transport_mode()?;

        let state = HandshakeComplete(transport_state);
        let handshake = Handshake {
            base: self.base,
            state,
        };

        Ok(handshake)
    }
}

impl Handshake<HandshakeComplete> {
    /// Read an encrypted message from the receive buffer, decrypt and write it
    /// to the message buffer - returning the byte size of the written payload.
    fn read_message(mut self, recv_buf: &[u8], msg: &mut [u8]) -> Result<usize> {
        let len = self.state.0.read_message(recv_buf, msg)?;

        Ok(len)
    }

    /// Encrypt and write a message to the send buffer, returning the byte size
    /// of the written payload.
    fn write_message(mut self, msg: &[u8], send_buf: &mut [u8]) -> Result<usize> {
        let len = self.state.0.write_message(msg, send_buf)?;

        Ok(len)
    }
}

#[derive(Debug, PartialEq)]
/// Major and minor identifiers for a particular version of the Cable Handshake
/// protocol.
pub struct Version {
    major: u8,
    minor: u8,
}

impl Version {
    /// Initialise a new version instance.
    pub fn init(major: u8, minor: u8) -> Self {
        Version { major, minor }
    }

    /// Return the major version identifier.
    pub fn major(&self) -> u8 {
        self.major
    }

    /// Return the minor version identifier.
    pub fn minor(&self) -> u8 {
        self.minor
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl ToBytes for Version {
    /// Convert a `Version` data type to bytes.
    fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0; 2];
        self.write_bytes(&mut buf)?;

        Ok(buf)
    }

    /// Write bytes to the given buffer (mutable byte array).
    fn write_bytes(&self, buf: &mut [u8]) -> Result<usize> {
        buf[0..1].copy_from_slice(&self.major.to_be_bytes());
        buf[1..2].copy_from_slice(&self.minor.to_be_bytes());

        Ok(2)
    }
}

impl FromBytes for Version {
    /// Read bytes from the given buffer (byte array), returning the total
    /// number of bytes and the decoded `Version` type.
    fn from_bytes(buf: &[u8]) -> Result<(usize, Self)> {
        let major = buf[0];
        let minor = buf[1];

        let version = Version { major, minor };

        Ok((2, version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_handshakers(
        client_version: (u8, u8),
        server_version: (u8, u8),
    ) -> Result<(Handshake<ClientSendVersion>, Handshake<ServerRecvVersion>)> {
        let psk: [u8; 32] = [1; 32];

        let builder = NoiseBuilder::new("Noise_XXpsk0_25519_ChaChaPoly_BLAKE2b".parse()?);

        let client_keypair = builder.generate_keypair()?;
        let client_private_key = client_keypair.private;

        let server_keypair = builder.generate_keypair()?;
        let server_private_key = server_keypair.private;

        let client_version = Version::init(client_version.0, client_version.1);
        let server_version = Version::init(server_version.0, server_version.1);

        let hs_client = Handshake::new_client(client_version, psk, client_private_key);
        let hs_server = Handshake::new_server(server_version, psk, server_private_key);

        Ok((hs_client, hs_server))
    }

    #[test]
    fn version_to_bytes() -> Result<()> {
        let version = Version { major: 0, minor: 1 };

        let version_to_bytes = version.to_bytes()?;
        let version_from_bytes = Version::from_bytes(&version_to_bytes)?;

        assert_eq!(version, version_from_bytes.1);

        Ok(())
    }

    #[test]
    fn version_exchange_success() -> Result<()> {
        let (hs_client, hs_server) = init_handshakers((1, 0), (1, 0))?;

        let mut buf = [0; 8];

        let mut client_buf = &mut buf[..version_bytes_len()];
        let hs_client = hs_client.send_client_version(&mut client_buf)?;

        let mut server_buf = &mut buf[..version_bytes_len()];
        let hs_server = hs_server.recv_client_version(&mut server_buf)?;

        let mut server_buf = &mut buf[..version_bytes_len()];
        hs_server.send_server_version(&mut server_buf)?;

        let mut client_buf = &mut buf[..version_bytes_len()];
        hs_client.recv_server_version(&mut client_buf)?;

        Ok(())
    }

    /* Refuses to compile.

    #[test]
    fn version_exchange_failure() -> Result<()> {
        let (hs_client, hs_server) = init_handshakers((3, 7), (1, 0))?;

        let mut buf = [0; 8];

        let mut client_buf = &mut buf[..version_bytes_len()];
        let hs_client = hs_client.send_client_version(&mut client_buf)?;

        let mut server_buf = &mut buf[..version_bytes_len()];
        let hs_server = hs_server.recv_client_version(&mut server_buf)?;

        let mut server_buf = &mut buf[..version_bytes_len()];
        let hs_server = hs_server.send_server_version(&mut server_buf)?;

        let mut client_buf = &mut buf[..version_bytes_len()];
        let hs_client = hs_client.recv_server_version(&mut client_buf)?;

        assert_eq!(hs_client, HandshakeError::IncompatibleServerVersion);

        Ok(())
    }
    */

    #[test]
    fn handshake() -> Result<()> {
        // Build the handshake client and server.
        let (hs_client, hs_server) = init_handshakers((1, 0), (1, 0))?;

        // Define a shared buffer for sending and receiving messages.
        let mut buf = [0; 1024];

        // Send and receive client version.
        let (hs_client, hs_server) = {
            let mut client_buf = &mut buf[..version_bytes_len()];
            let hs_client = hs_client.send_client_version(&mut client_buf)?;
            let mut server_buf = &mut buf[..version_bytes_len()];
            let hs_server = hs_server.recv_client_version(&mut server_buf)?;
            (hs_client, hs_server)
        };

        // Send and receive server version.
        let (hs_client, hs_server) = {
            let mut server_buf = &mut buf[..version_bytes_len()];
            let hs_server = hs_server.send_server_version(&mut server_buf)?;
            let mut client_buf = &mut buf[..version_bytes_len()];
            let hs_client = hs_client.recv_server_version(&mut client_buf)?;
            (hs_client, hs_server)
        };

        // Build client and server Noise state machines.
        let (hs_client, hs_server) = {
            let hs_client = hs_client.build_client_noise_state_machine()?;
            let hs_server = hs_server.build_server_noise_state_machine()?;
            (hs_client, hs_server)
        };

        // Send and receive client ephemeral key.
        let (hs_client, hs_server) = {
            let (ephemeral_key_bytes_len, hs_client) =
                hs_client.send_client_ephemeral_key(&mut buf)?;
            let mut server_buf = &mut buf[..ephemeral_key_bytes_len];
            let hs_server = hs_server.recv_client_ephemeral_key(&mut server_buf)?;
            (hs_client, hs_server)
        };

        // Send and receive server ephemeral and static keys.
        let (hs_client, hs_server) = {
            let (ephemeral_and_static_key_bytes_len, hs_server) =
                hs_server.send_server_ephemeral_and_static_keys(&mut buf)?;
            let mut client_buf = &mut buf[..ephemeral_and_static_key_bytes_len];
            let hs_client = hs_client.recv_server_ephemeral_and_static_keys(&mut client_buf)?;
            (hs_client, hs_server)
        };

        // Send and receive client static key.
        let (hs_client, hs_server) = {
            let (static_key_bytes_len, hs_client) = hs_client.send_client_static_key(&mut buf)?;
            let mut server_buf = &mut buf[..static_key_bytes_len];
            let hs_server = hs_server.recv_client_static_key(&mut server_buf)?;
            (hs_client, hs_server)
        };

        // Initialise client and server transport mode.
        let hs_client = hs_client.init_client_transport_mode()?;
        let hs_server = hs_server.init_server_transport_mode()?;

        // Write an encrypted message.
        let msg_text = b"An impeccably polite pangolin";
        let write_len = hs_client.write_message(msg_text, &mut buf)?;

        // Read an encrypted message.
        let mut msg_buf = [0u8; 48];
        let read_len = hs_server.read_message(&buf[..write_len], &mut msg_buf)?;

        assert_eq!(msg_text, &msg_buf[..read_len]);

        Ok(())
    }

    /*
    TODO: Rather test with TCP connection in integration tests.

    fn _setup_tcp_connection() {}

    #[test]
    fn version_exchange_works() -> std::io::Result<()> {
        // Deploy a TCP listener.
        //
        // Assigning port to 0 means that the OS selects an available port for us.
        let listener = TcpListener::bind("127.0.0.1:0")?;

        // Retrieve the address of the TCP listener to be able to connect later on.
        let addr = listener.local_addr()?;

        thread::spawn(move || {
            // Accept connections and process them serially.
            for stream in listener.incoming() {
                stream.read(&mut [0; 8])?;
            }
        });

        let mut stream = TcpStream::connect(addr)?;

        stream.write(&[1])?;
    }
    */
}
