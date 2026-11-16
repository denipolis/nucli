use std::io::{self, Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use snow::{Builder, HandshakeState, StatelessTransportState};
use spake2::{Ed25519Group, Identity as SpakeIdentity, Password, Spake2};

pub type Key = [u8; 32];

pub const VERSION: u8 = 1;
const MAGIC: &[u8; 7] = b"NUCLIUP";
const SESSION: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";
const PAIRING: &str = "Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s";
const JOINER: &[u8] = b"nucli-uplink-joiner";
const HOST: &[u8] = b"nucli-uplink-host";
const MAX_FRAME: usize = 65535;
const TAG: usize = 16;
const CHUNK: usize = MAX_FRAME - TAG - 1;
pub const MAX_MESSAGE: usize = 16777216;
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const CODE_LEN: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Session,
    Pair,
}

pub struct Identity {
    pub secret: Key,
    pub public: Key,
}

impl Identity {
    pub fn generate() -> Identity {
        let pair = Builder::new(params(SESSION)).generate_keypair().expect("the system random source works");
        Identity { secret: key(&pair.private).expect("32 bytes"), public: key(&pair.public).expect("32 bytes") }
    }
}

fn params(pattern: &str) -> snow::params::NoiseParams {
    pattern.parse().expect("a valid noise pattern")
}

fn key(bytes: &[u8]) -> Option<Key> {
    bytes.try_into().ok()
}

fn failed(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, error.to_string())
}

pub(crate) fn preamble(mode: Mode) -> [u8; 9] {
    let mut bytes = [0u8; 9];
    bytes[..7].copy_from_slice(MAGIC);
    bytes[7] = VERSION;
    bytes[8] = match mode {
        Mode::Session => 0,
        Mode::Pair => 1,
    };
    bytes
}

pub fn read_preamble(stream: &mut TcpStream) -> io::Result<Mode> {
    let mut bytes = [0u8; 9];
    stream.read_exact(&mut bytes)?;
    if &bytes[..7] != MAGIC {
        return Err(failed("not a Nucli Uplink connection"));
    }
    if bytes[7] != VERSION {
        return Err(failed(format!("Uplink protocol {} is not supported", bytes[7])));
    }
    match bytes[8] {
        0 => Ok(Mode::Session),
        1 => Ok(Mode::Pair),
        _ => Err(failed("unknown connection mode")),
    }
}

fn send_frame(stream: &mut TcpStream, bytes: &[u8]) -> io::Result<()> {
    let length = u16::try_from(bytes.len()).map_err(|_| failed("frame too large"))?;
    let mut frame = Vec::with_capacity(bytes.len() + 2);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(bytes);
    stream.write_all(&frame)
}

fn read_frame(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut length = [0u8; 2];
    stream.read_exact(&mut length)?;
    let mut bytes = vec![0u8; u16::from_be_bytes(length) as usize];
    stream.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn write_handshake(stream: &mut TcpStream, noise: &mut HandshakeState) -> io::Result<()> {
    let mut buffer = vec![0u8; MAX_FRAME];
    let length = noise.write_message(&[], &mut buffer).map_err(failed)?;
    send_frame(stream, &buffer[..length])
}

fn read_handshake(stream: &mut TcpStream, noise: &mut HandshakeState) -> io::Result<()> {
    let frame = read_frame(stream)?;
    let mut buffer = vec![0u8; MAX_FRAME];
    noise.read_message(&frame, &mut buffer).map_err(failed)?;
    Ok(())
}

fn remote_key(noise: &HandshakeState) -> io::Result<Key> {
    noise.get_remote_static().and_then(key).ok_or_else(|| failed("the other device sent no key"))
}

fn finish(stream: TcpStream, noise: HandshakeState, remote: Key) -> io::Result<Channel> {
    let sas = comparison(noise.get_handshake_hash());
    let transport = noise.into_stateless_transport_mode().map_err(failed)?;
    Ok(Channel { stream, transport: Arc::new(transport), remote, sas })
}

pub fn comparison(hash: &[u8]) -> String {
    let number = hash.iter().take(8).fold(0u64, |number, byte| number << 8 | *byte as u64) % 1000000;
    format!("{:03} {:03}", number / 1000, number % 1000)
}

pub struct Channel {
    pub stream: TcpStream,
    transport: Arc<StatelessTransportState>,
    pub remote: Key,
    pub sas: String,
}

impl Channel {
    pub fn split(self) -> io::Result<(Reader, Writer)> {
        let reader = Reader {
            stream: self.stream.try_clone()?,
            transport: self.transport.clone(),
            nonce: 0,
            buffer: Vec::new(),
        };
        Ok((reader, Writer { stream: self.stream, transport: self.transport, nonce: 0 }))
    }
}

pub struct Reader {
    pub stream: TcpStream,
    transport: Arc<StatelessTransportState>,
    nonce: u64,
    buffer: Vec<u8>,
}

impl Reader {
    pub fn read(&mut self) -> io::Result<Vec<u8>> {
        let mut message = Vec::new();
        loop {
            let frame = read_frame(&mut self.stream)?;
            self.buffer.resize(frame.len(), 0);
            let length = self.transport.read_message(self.nonce, &frame, &mut self.buffer).map_err(failed)?;
            self.nonce += 1;
            let Some((&more, body)) = self.buffer[..length].split_first() else {
                return Err(failed("empty frame"));
            };
            if message.len() + body.len() > MAX_MESSAGE {
                return Err(failed("message too large"));
            }
            message.extend_from_slice(body);
            if more == 0 {
                return Ok(message);
            }
        }
    }
}

pub struct Writer {
    pub stream: TcpStream,
    transport: Arc<StatelessTransportState>,
    nonce: u64,
}

impl Writer {
    pub fn write(&mut self, message: &[u8]) -> io::Result<()> {
        if message.len() > MAX_MESSAGE {
            return Err(failed("message too large"));
        }
        let mut chunks = message.chunks(CHUNK).peekable();
        let mut plain = Vec::with_capacity(CHUNK + 1);
        let mut sealed = vec![0u8; MAX_FRAME];
        let mut out = Vec::new();
        loop {
            let chunk = chunks.next().unwrap_or_default();
            plain.clear();
            plain.push(u8::from(chunks.peek().is_some()));
            plain.extend_from_slice(chunk);
            let length = self.transport.write_message(self.nonce, &plain, &mut sealed).map_err(failed)?;
            self.nonce += 1;
            out.extend_from_slice(&(length as u16).to_be_bytes());
            out.extend_from_slice(&sealed[..length]);
            if chunks.peek().is_none() {
                break;
            }
        }
        self.stream.write_all(&out)?;
        self.stream.flush()
    }
}

pub fn connect(mut stream: TcpStream, identity: &Identity, expected: &Key) -> io::Result<Channel> {
    let preamble = preamble(Mode::Session);
    stream.write_all(&preamble)?;
    let mut noise = Builder::new(params(SESSION))
        .local_private_key(&identity.secret)
        .and_then(|builder| builder.prologue(&preamble))
        .and_then(|builder| builder.build_initiator())
        .map_err(failed)?;
    write_handshake(&mut stream, &mut noise)?;
    read_handshake(&mut stream, &mut noise)?;
    let remote = remote_key(&noise)?;
    if remote != *expected {
        return Err(failed("the device at this address has a different key"));
    }
    write_handshake(&mut stream, &mut noise)?;
    finish(stream, noise, remote)
}

pub fn accept(mut stream: TcpStream, identity: &Identity, trusted: impl Fn(&Key) -> bool) -> io::Result<Channel> {
    let preamble = preamble(Mode::Session);
    let mut noise = Builder::new(params(SESSION))
        .local_private_key(&identity.secret)
        .and_then(|builder| builder.prologue(&preamble))
        .and_then(|builder| builder.build_responder())
        .map_err(failed)?;
    read_handshake(&mut stream, &mut noise)?;
    write_handshake(&mut stream, &mut noise)?;
    read_handshake(&mut stream, &mut noise)?;
    let remote = remote_key(&noise)?;
    if !trusted(&remote) {
        return Err(failed("this device is not paired"));
    }
    finish(stream, noise, remote)
}

fn exchange_secret(theirs: &[u8], spake: Spake2<Ed25519Group>) -> io::Result<Key> {
    let secret = spake.finish(theirs).map_err(|_| failed("malformed pairing message"))?;
    key(&secret).ok_or_else(|| failed("unexpected pairing secret"))
}

pub fn join(mut stream: TcpStream, identity: &Identity, code: &str) -> io::Result<Channel> {
    let preamble = preamble(Mode::Pair);
    stream.write_all(&preamble)?;
    let (spake, outbound) = Spake2::<Ed25519Group>::start_a(
        &Password::new(code.as_bytes()),
        &SpakeIdentity::new(JOINER),
        &SpakeIdentity::new(HOST),
    );
    send_frame(&mut stream, &outbound)?;
    let inbound = read_frame(&mut stream)?;
    let secret = exchange_secret(&inbound, spake)?;
    let prologue = [&preamble[..], &outbound, &inbound].concat();
    let mut noise = Builder::new(params(PAIRING))
        .local_private_key(&identity.secret)
        .and_then(|builder| builder.psk(3, &secret))
        .and_then(|builder| builder.prologue(&prologue))
        .and_then(|builder| builder.build_initiator())
        .map_err(failed)?;
    write_handshake(&mut stream, &mut noise)?;
    read_handshake(&mut stream, &mut noise)?;
    write_handshake(&mut stream, &mut noise)?;
    let remote = remote_key(&noise)?;
    finish(stream, noise, remote)
}

pub fn host_pairing(mut stream: TcpStream, identity: &Identity, code: &str) -> io::Result<Channel> {
    let inbound = read_frame(&mut stream)?;
    let (spake, outbound) = Spake2::<Ed25519Group>::start_b(
        &Password::new(code.as_bytes()),
        &SpakeIdentity::new(JOINER),
        &SpakeIdentity::new(HOST),
    );
    send_frame(&mut stream, &outbound)?;
    let secret = exchange_secret(&inbound, spake)?;
    let prologue = [&preamble(Mode::Pair)[..], &inbound, &outbound].concat();
    let mut noise = Builder::new(params(PAIRING))
        .local_private_key(&identity.secret)
        .and_then(|builder| builder.psk(3, &secret))
        .and_then(|builder| builder.prologue(&prologue))
        .and_then(|builder| builder.build_responder())
        .map_err(failed)?;
    read_handshake(&mut stream, &mut noise)?;
    write_handshake(&mut stream, &mut noise)?;
    read_handshake(&mut stream, &mut noise)
        .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "wrong pairing code"))?;
    let remote = remote_key(&noise)?;
    finish(stream, noise, remote)
}

pub fn new_code() -> String {
    let mut bytes = [0u8; 5];
    getrandom::fill(&mut bytes).expect("the system random source works");
    let bits = bytes.iter().fold(0u64, |bits, byte| bits << 8 | *byte as u64);
    (0..CODE_LEN).map(|index| ALPHABET[(bits >> (35 - 5 * index) & 31) as usize] as char).collect()
}

pub fn normalize_code(input: &str) -> Option<String> {
    let code: String = input
        .chars()
        .filter(|c| !matches!(c, '-' | ' ' | '\t'))
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect();
    (code.len() == CODE_LEN && code.bytes().all(|byte| ALPHABET.contains(&byte))).then_some(code)
}

pub fn format_code(code: &str) -> String {
    let (first, second) = code.split_at(code.len().min(4));
    format!("{first}-{second}")
}

pub fn encode_key(key: &Key) -> String {
    let mut out = String::with_capacity(52);
    let mut bits = 0u32;
    let mut count = 0;
    for byte in key {
        bits = bits << 8 | *byte as u32;
        count += 8;
        while count >= 5 {
            count -= 5;
            out.push(ALPHABET[(bits >> count & 31) as usize] as char);
        }
    }
    if count > 0 {
        out.push(ALPHABET[(bits << (5 - count) & 31) as usize] as char);
    }
    out
}

pub fn decode_key(text: &str) -> Option<Key> {
    let mut bytes = Vec::with_capacity(32);
    let mut bits = 0u32;
    let mut count = 0;
    for character in text.bytes() {
        let value = ALPHABET.iter().position(|symbol| *symbol == character)? as u32;
        bits = (bits << 5 | value) & 0xffff;
        count += 5;
        if count >= 8 {
            count -= 8;
            bytes.push((bits >> count) as u8);
        }
    }
    key(&bytes).filter(|key| encode_key(key) == text)
}

pub fn fingerprint(key: &Key) -> String {
    let text = encode_key(key);
    let groups: Vec<&str> = (0..4).map(|index| &text[index * 4..index * 4 + 4]).collect();
    groups.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = std::thread::spawn(move || TcpStream::connect(address).unwrap());
        let (server, _) = listener.accept().unwrap();
        (client.join().unwrap(), server)
    }

    fn host(server: TcpStream, identity: Identity, code: &'static str) -> std::thread::JoinHandle<io::Result<Key>> {
        std::thread::spawn(move || {
            let mut server = server;
            assert_eq!(read_preamble(&mut server)?, Mode::Pair);
            host_pairing(server, &identity, code).map(|channel| channel.remote)
        })
    }

    #[test]
    fn pairs_with_the_right_code_and_exchanges_keys() {
        let (joiner, hoster) = (Identity::generate(), Identity::generate());
        let (joiner_public, hoster_public) = (joiner.public, hoster.public);
        let (client, server) = pair();
        let hosting = host(server, hoster, "K7QM4XRP");
        let channel = join(client, &joiner, "K7QM4XRP").unwrap();
        assert_eq!(channel.remote, hoster_public);
        assert_eq!(hosting.join().unwrap().unwrap(), joiner_public);
    }

    #[test]
    fn a_wrong_code_fails_on_both_sides() {
        let (client, server) = pair();
        let hosting = host(server, Identity::generate(), "K7QM4XRP");
        let joined = join(client, &Identity::generate(), "K7QM4XRQ");
        let hosted = hosting.join().unwrap();
        assert!(hosted.is_err());
        let confirmed = joined.and_then(|channel| {
            let (mut reader, _) = channel.split()?;
            reader.read()
        });
        assert!(confirmed.is_err());
    }

    #[test]
    fn sessions_check_both_keys() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let (alice_public, bob_public) = (alice.public, bob.public);
        let (client, server) = pair();
        let accepting = std::thread::spawn(move || {
            let mut server = server;
            read_preamble(&mut server)?;
            accept(server, &bob, |key| *key == alice_public)
        });
        let channel = connect(client, &alice, &bob_public).unwrap();
        let accepted = accepting.join().unwrap().unwrap();
        assert_eq!(accepted.remote, alice_public);
        let (_, mut writer) = channel.split().unwrap();
        let (mut reader, _) = accepted.split().unwrap();
        let big: Vec<u8> = (0..200000u32).map(|index| index as u8).collect();
        writer.write(b"hello").unwrap();
        writer.write(&big).unwrap();
        writer.write(&[]).unwrap();
        assert_eq!(reader.read().unwrap(), b"hello");
        assert_eq!(reader.read().unwrap(), big);
        assert_eq!(reader.read().unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn refuses_unknown_devices_and_impostors() {
        let (stranger, bob) = (Identity::generate(), Identity::generate());
        let bob_public = bob.public;
        let (client, server) = pair();
        let accepting = std::thread::spawn(move || {
            let mut server = server;
            read_preamble(&mut server)?;
            accept(server, &bob, |_| false)
        });
        let _ = connect(client, &stranger, &bob_public);
        assert!(accepting.join().unwrap().is_err());

        let (impostor, alice) = (Identity::generate(), Identity::generate());
        let (client, server) = pair();
        std::thread::spawn(move || {
            let mut server = server;
            let _ = read_preamble(&mut server);
            let _ = accept(server, &impostor, |_| true);
        });
        assert!(connect(client, &alice, &bob_public).is_err());
    }

    #[test]
    fn tampered_frames_are_rejected() {
        let (alice, bob) = (Identity::generate(), Identity::generate());
        let (alice_public, bob_public) = (alice.public, bob.public);
        let (client, server) = pair();
        let accepting = std::thread::spawn(move || {
            let mut server = server;
            read_preamble(&mut server)?;
            accept(server, &bob, |key| *key == alice_public)
        });
        let channel = connect(client, &alice, &bob_public).unwrap();
        let (mut reader, _) = accepting.join().unwrap().unwrap().split().unwrap();
        let (_, mut writer) = channel.split().unwrap();
        let mut raw = writer.stream.try_clone().unwrap();
        writer.write(b"first").unwrap();
        raw.write_all(&[0, 20]).unwrap();
        raw.write_all(&[7u8; 20]).unwrap();
        assert_eq!(reader.read().unwrap(), b"first");
        assert!(reader.read().is_err());
    }

    #[test]
    fn codes_are_random_and_forgiving() {
        let code = new_code();
        assert_eq!(code.len(), CODE_LEN);
        assert_ne!(code, new_code());
        assert_eq!(normalize_code(&format_code(&code)).as_deref(), Some(code.as_str()));
        assert_eq!(normalize_code("k7qm-4xrp").as_deref(), Some("K7QM4XRP"));
        assert_eq!(normalize_code("ol0i 1111").as_deref(), Some("01011111"));
        assert_eq!(normalize_code("K7QM4XR"), None);
        assert_eq!(normalize_code("K7QM4XRU"), None);
    }

    #[test]
    fn keys_round_trip_as_text() {
        let identity = Identity::generate();
        let text = encode_key(&identity.public);
        assert_eq!(text.len(), 52);
        assert_eq!(decode_key(&text), Some(identity.public));
        assert_eq!(decode_key(&text[..51]), None);
        assert_eq!(fingerprint(&identity.public).len(), 19);
    }
}
