//! Post formats for all post types supported by cable.
//!
//! Includes type definitions for all post types, as well as post header and
//! body types. Helper methods are included, some of which provide cryptographic
//! functions such as post signing and hashing.
//!
//! Also includes implementations of the `CountBytes`, `FromBytes` and `ToBytes`
//! traits for `Post`. This forms the core of the cable protocol.

use std::fmt;

use desert::{varint, CountBytes, FromBytes, ToBytes};
use sodiumoxide::{
    crypto::{
        generichash, sign,
        sign::{PublicKey, SecretKey, Signature},
    },
    hex,
};

use crate::{
    constants::{DELETE_POST, INFO_POST, JOIN_POST, LEAVE_POST, TEXT_POST, TOPIC_POST},
    error::{CableErrorKind, Error},
    validation, Channel, Hash, Text, Topic, UserInfo,
};

#[derive(Clone, Debug)]
/// The header of a post.
pub struct PostHeader {
    /// Public key that authored this post.
    pub public_key: [u8; 32],
    /// Signature of the fields that follow.
    pub signature: [u8; 64],
    /// Hashes of the latest posts in this channel/context.
    pub links: Vec<Hash>,
    /// Post type.
    pub post_type: u64,
    /// Time at which the post was created (in milliseconds since the UNIX Epoch).
    pub timestamp: u64,
}

impl PostHeader {
    /// Convenience method to construct a `PostHeader`.
    pub fn new(
        public_key: [u8; 32],
        signature: [u8; 64],
        links: Vec<Hash>,
        post_type: u64,
        timestamp: u64,
    ) -> Self {
        PostHeader {
            public_key,
            signature,
            links,
            post_type,
            timestamp,
        }
    }
}

/// Print a post header with byte arrays formatted as hex strings.
impl fmt::Display for PostHeader {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let public_key_hex = hex::encode(self.public_key);
        let signature_hex = hex::encode(self.signature);
        let links_hex: &Vec<String> = &self.links.iter().map(hex::encode).collect();

        write!(
            f,
            "\"public_key\": {:?}, \"signature\": {:?}, \"links\": {:?}, \"post_type\": {}, \"timestamp\": {}",
            public_key_hex, signature_hex, links_hex, &self.post_type, &self.timestamp
        )
    }
}

// TODO: remember to write validators for post type data.
// E.g. "A topic field MUST be a valid UTF-8 string, between 0 and 512 codepoints."

#[derive(Clone, Debug)]
/// The body of a post.
pub enum PostBody {
    /// Post a chat message to a channel.
    Text {
        /// Channel name (UTF-8).
        channel: Channel,
        /// Chat message text (UTF-8).
        text: Text,
    },
    /// Request that peers encountering this post delete the referenced posts
    /// from their local storage, and not store the referenced posts in the future.
    Delete {
        /// Concatenated hashes of posts to be deleted.
        hashes: Vec<Hash>,
    },
    /// Set public information about oneself.
    Info {
        /// The complete description of a user's self-published information.
        info: Vec<UserInfo>,
    },
    /// Set a topic for a channel.
    Topic {
        /// Channel name (UTF-8).
        channel: Channel,
        /// Topic content (UTF-8).
        topic: Topic,
    },
    /// Publicly announce membership in a channel.
    Join {
        /// Channel name (UTF-8).
        channel: Channel,
    },
    /// Publicly announce termination of membership in a channel.
    Leave {
        /// Channel name (UTF-8).
        channel: Channel,
    },
    /// A post type which is not recognised as part of the cable specification.
    Unrecognized { post_type: u64 },
}

/// Print a post body with byte arrays formatted as hex strings.
impl fmt::Display for PostBody {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PostBody::Text { channel, text } => {
                write!(f, "\"channel\": {:?}, \"text\": {:?}", channel, text)
            }
            PostBody::Delete { hashes } => {
                let hashes_hex: Vec<String> = hashes.iter().map(hex::encode).collect();
                write!(f, "\"hashes\": {:?}", hashes_hex)
            }
            PostBody::Info { info } => {
                write!(f, "\"info\": {:?}", info)
            }
            PostBody::Topic { channel, topic } => {
                write!(f, "\"channel\": {:?}, \"topic\": {:?}", channel, topic)
            }
            PostBody::Join { channel } => {
                write!(f, "\"channel\": {:?}", channel)
            }
            PostBody::Leave { channel } => {
                write!(f, "\"channel\": {:?}", channel)
            }
            PostBody::Unrecognized { post_type: _ } => {
                write!(f, "\"post_type\": \"unrecognized\"")
            }
        }
    }
}

/// A complete post including header and body values.
#[derive(Clone, Debug)]
pub struct Post {
    pub header: PostHeader,
    pub body: PostBody,
}

impl Post {
    /// Construct a `Post` from a header and body.
    pub fn new(header: PostHeader, body: PostBody) -> Self {
        Post { header, body }
    }

    /// Construct an unsigned text `Post` with the given parameters.
    pub fn text(
        public_key: [u8; 32],
        links: Vec<Hash>,
        timestamp: u64,
        channel: Channel,
        text: Text,
    ) -> Self {
        let header = PostHeader::new(public_key, [0; 64], links, TEXT_POST, timestamp);
        let body = PostBody::Text { channel, text };

        Post { header, body }
    }

    /// Construct an unsigned delete `Post` with the given parameters.
    pub fn delete(
        public_key: [u8; 32],
        links: Vec<Hash>,
        timestamp: u64,
        hashes: Vec<Hash>,
    ) -> Self {
        let header = PostHeader::new(public_key, [0; 64], links, DELETE_POST, timestamp);
        let body = PostBody::Delete { hashes };

        Post { header, body }
    }

    /// Construct an unsigned info `Post` with the given parameters.
    pub fn info(
        public_key: [u8; 32],
        links: Vec<Hash>,
        timestamp: u64,
        info: Vec<UserInfo>,
    ) -> Self {
        let header = PostHeader::new(public_key, [0; 64], links, INFO_POST, timestamp);
        let body = PostBody::Info { info };

        Post { header, body }
    }

    /// Construct an unsigned topic `Post` with the given parameters.
    pub fn topic(
        public_key: [u8; 32],
        links: Vec<Hash>,
        timestamp: u64,
        channel: Channel,
        topic: Topic,
    ) -> Self {
        let header = PostHeader::new(public_key, [0; 64], links, TOPIC_POST, timestamp);
        let body = PostBody::Topic { channel, topic };

        Post { header, body }
    }

    /// Construct an unsigned join `Post` with the given parameters.
    pub fn join(public_key: [u8; 32], links: Vec<Hash>, timestamp: u64, channel: Channel) -> Self {
        let header = PostHeader::new(public_key, [0; 64], links, JOIN_POST, timestamp);
        let body = PostBody::Join { channel };

        Post { header, body }
    }

    /// Construct an unsigned leave `Post` with the given parameters.
    pub fn leave(public_key: [u8; 32], links: Vec<Hash>, timestamp: u64, channel: Channel) -> Self {
        let header = PostHeader::new(public_key, [0; 64], links, LEAVE_POST, timestamp);
        let body = PostBody::Leave { channel };

        Post { header, body }
    }

    /// Return the channel name associated with a post.
    pub fn get_channel(&self) -> Option<&Channel> {
        match &self.body {
            PostBody::Text { channel, .. } => Some(channel),
            PostBody::Delete { .. } => None,
            PostBody::Info { .. } => None,
            PostBody::Topic { channel, .. } => Some(channel),
            PostBody::Join { channel, .. } => Some(channel),
            PostBody::Leave { channel, .. } => Some(channel),
            PostBody::Unrecognized { .. } => None,
        }
    }

    /// Return the timestamp of the post.
    pub fn get_timestamp(&self) -> u64 {
        let PostHeader { timestamp, .. } = &self.header;

        *timestamp
    }

    /// Return the hash of the post.
    pub fn hash(&self) -> Result<Hash, Error> {
        let buf = self.to_bytes()?;

        // Compute a hash for the post.
        let digest = if let Ok(hash) = generichash::hash(&buf, Some(32), None) {
            hash
        } else {
            return CableErrorKind::PostHashingFailed {}.raise();
        };

        Ok(digest.as_ref().try_into()?)
    }

    /// Check if the post has a signature.
    pub fn is_signed(&self) -> bool {
        for i in 0..self.header.signature.len() {
            if self.header.signature[i] != 0 {
                return true;
            }
        }

        false
    }

    /// Return the numeric type identifier for the post.
    pub fn post_type(&self) -> u64 {
        match &self.body {
            PostBody::Text { .. } => TEXT_POST,
            PostBody::Delete { .. } => DELETE_POST,
            PostBody::Info { .. } => INFO_POST,
            PostBody::Topic { .. } => TOPIC_POST,
            PostBody::Join { .. } => JOIN_POST,
            PostBody::Leave { .. } => LEAVE_POST,
            PostBody::Unrecognized { post_type } => *post_type,
        }
    }

    /// Sign a post using the given secret key.
    pub fn sign(&mut self, secret_key: &[u8; 64]) -> Result<(), Error> {
        let buf = self.to_bytes()?;

        // Decode the secret key from the byte slice.
        let sk = if let Some(key) = SecretKey::from_slice(secret_key) {
            key
        } else {
            return CableErrorKind::NoneError {
                context: "failed to decode secret key from slice".to_string(),
            }
            .raise();
        };

        // Sign the post bytes and update the signature field of the post header.
        self.header.signature = sign::sign_detached(&buf[32 + 64..], &sk).to_bytes();

        Ok(())
    }

    /// Verify the signature of an encoded post.
    pub fn verify(buf: &[u8]) -> bool {
        // Since the public key is 32 bytes and the signature is 64 bytes,
        // a valid message must be greater than 32 + 64 bytes.
        if buf.len() < 32 + 64 {
            return false;
        }

        let public_key = PublicKey::from_slice(&buf[0..32]);
        let signature = Signature::from_bytes(&buf[32..32 + 64]);

        match (public_key, signature) {
            (Some(pk), Ok(sig)) => sign::verify_detached(&sig, &buf[32 + 64..], &pk),
            _ => false,
        }
    }
}

/// Print a post with byte arrays formatted as hex strings.
impl fmt::Display for Post {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{ {}, {} }}", &self.header, &self.body)
    }
}

impl ToBytes for Post {
    /// Convert a `Post` data type to bytes.
    fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        let mut buf = vec![0; self.count_bytes()];
        self.write_bytes(&mut buf)?;

        Ok(buf)
    }

    /// Write bytes to the given buffer (mutable byte array).
    fn write_bytes(&self, buf: &mut [u8]) -> Result<usize, Error> {
        let mut offset = 0;

        /* POST HEADER BYTES */

        // Write the public key bytes to the buffer and increment the offset.
        buf[offset..offset + 32].copy_from_slice(&self.header.public_key);
        offset += self.header.public_key.len();

        // Write the signature bytes to the buffer and increment the offset.
        buf[offset..offset + 64].copy_from_slice(&self.header.signature);
        offset += self.header.signature.len();

        // Encode num_links as a varint, write the resulting bytes to the
        // buffer and increment the offset.
        offset += varint::encode(self.header.links.len() as u64, &mut buf[offset..])?;

        // Write the links bytes to the buffer and increment the offset.
        // Each `link` is a hash.
        for link in self.header.links.iter() {
            if offset + link.len() > buf.len() {
                return CableErrorKind::DstTooSmall {
                    required: offset + link.len(),
                    provided: buf.len(),
                }
                .raise();
            }
            buf[offset..offset + link.len()].copy_from_slice(link);
            offset += link.len();
        }

        // Encode the post type as a varint, write the resulting bytes to the
        // buffer and increment the offset.
        offset += varint::encode(self.post_type(), &mut buf[offset..])?;

        // Encode the timestamp as a varint, write the resulting bytes to the
        // buffer and increment the offset.
        offset += varint::encode(self.header.timestamp, &mut buf[offset..])?;

        /* POST BODY BYTES */

        match &self.body {
            PostBody::Text { channel, text } => {
                offset += varint::encode(channel.len() as u64, &mut buf[offset..])?;
                buf[offset..offset + channel.len()].copy_from_slice(channel.as_bytes());
                offset += channel.len();

                offset += varint::encode(text.len() as u64, &mut buf[offset..])?;
                buf[offset..offset + text.len()].copy_from_slice(text.as_bytes());
                offset += text.len();
            }
            PostBody::Delete { hashes } => {
                offset += varint::encode(hashes.len() as u64, &mut buf[offset..])?;
                for hash in hashes {
                    if offset + hash.len() > buf.len() {
                        return CableErrorKind::DstTooSmall {
                            required: offset + hash.len(),
                            provided: buf.len(),
                        }
                        .raise();
                    }
                    buf[offset..offset + hash.len()].copy_from_slice(hash);
                    offset += hash.len();
                }
            }
            PostBody::Info { info } => {
                for UserInfo { key, val } in info {
                    offset += varint::encode(key.len() as u64, &mut buf[offset..])?;
                    buf[offset..offset + key.len()].copy_from_slice(key.as_bytes());
                    offset += key.len();

                    offset += varint::encode(val.len() as u64, &mut buf[offset..])?;
                    buf[offset..offset + val.len()].copy_from_slice(val.as_bytes());
                    offset += val.len();
                }

                // Indicate the end of the key-value pairs by setting the final
                // key_len to 0.
                offset += varint::encode(0, &mut buf[offset..])?;
            }
            PostBody::Topic { channel, topic } => {
                offset += varint::encode(channel.len() as u64, &mut buf[offset..])?;
                buf[offset..offset + channel.len()].copy_from_slice(channel.as_bytes());
                offset += channel.len();
                offset += varint::encode(topic.len() as u64, &mut buf[offset..])?;
                buf[offset..offset + topic.len()].copy_from_slice(topic.as_bytes());
                offset += topic.len();
            }
            PostBody::Join { channel } => {
                offset += varint::encode(channel.len() as u64, &mut buf[offset..])?;
                buf[offset..offset + channel.len()].copy_from_slice(channel.as_bytes());
                offset += channel.len();
            }
            PostBody::Leave { channel } => {
                offset += varint::encode(channel.len() as u64, &mut buf[offset..])?;
                buf[offset..offset + channel.len()].copy_from_slice(channel.as_bytes());
                offset += channel.len();
            }
            PostBody::Unrecognized { post_type } => {
                return CableErrorKind::PostWriteUnrecognizedType {
                    post_type: *post_type,
                }
                .raise();
            }
        }

        Ok(offset)
    }
}

impl FromBytes for Post {
    /// Read bytes from the given buffer (byte array), returning the total
    /// number of bytes and the decoded `Post` type.
    fn from_bytes(buf: &[u8]) -> Result<(usize, Self), Error> {
        let mut offset = 0;

        /* POST HEADER BYTES */

        // Read the public key bytes from the buffer and increment the offset.
        let mut public_key = [0; 32];
        public_key.copy_from_slice(&buf[offset..offset + 32]);
        offset += 32;

        // Read the signature bytes from the buffer and increment the offset.
        let mut signature = [0; 64];
        signature.copy_from_slice(&buf[offset..offset + 64]);
        offset += 64;

        // Read the number of links byte from the buffer and increment the offset.
        // This value encodes the number of links to follow.
        // TODO: Consider renaming `num_links` to `hash_count`.
        let (s, num_links) = varint::decode(&buf[offset..])?;
        offset += s;

        let mut links = Vec::with_capacity(num_links as usize);

        // Iterate over the links (hashes), reading the bytes from the buffer
        // and incrementing the offset for each one.
        for _ in 0..num_links {
            if offset + 32 > buf.len() {
                return CableErrorKind::MessageHashResponseEnd {}.raise();
            }

            let mut link = [0; 32];
            link.copy_from_slice(&buf[offset..offset + 32]);
            offset += 32;

            links.push(link);
        }

        // Read the post-type byte from the buffer and increment the offset.
        let (s, post_type) = varint::decode(&buf[offset..])?;
        offset += s;

        // Read the timestamp byte from the buffer and increment the offset.
        let (s, timestamp) = varint::decode(&buf[offset..])?;
        offset += s;

        // Construct the post header.
        let header = PostHeader {
            public_key,
            signature,
            links,
            post_type,
            timestamp,
        };

        /* POST BODY BYTES */

        // Read post body field bytes.
        let body = match post_type {
            TEXT_POST => {
                // Read the channel length byte and increment the offset.
                let (s, channel_len) = varint::decode(&buf[offset..])?;
                offset += s;

                // Read the channel bytes.
                let channel =
                    String::from_utf8(buf[offset..offset + channel_len as usize].to_vec())?;
                // Validate the length of the channel name.
                validation::validate_channel(&channel)?;
                // Increment the offset.
                offset += channel_len as usize;

                // Read the text length byte and increment the offset.
                let (s, text_len) = varint::decode(&buf[offset..])?;
                offset += s;

                // Read the text bytes and increment the offset.
                let text = String::from_utf8(buf[offset..offset + text_len as usize].to_vec())?;
                offset += text_len as usize;

                PostBody::Text { channel, text }
            }
            DELETE_POST => {
                // Read the number of hashes byte and increment the offset.
                let (s, num_hashes) = varint::decode(&buf[offset..])?;
                offset += s;

                let mut hashes = Vec::with_capacity(num_hashes as usize);

                // Iterate over the hashes, reading the bytes from the buffer
                // and incrementing the offset for each one.
                for _ in 0..num_hashes {
                    if offset + 32 > buf.len() {
                        return CableErrorKind::MessageHashResponseEnd {}.raise();
                    }

                    let mut hash = [0; 32];
                    hash.copy_from_slice(&buf[offset..offset + 32]);
                    offset += 32;

                    hashes.push(hash);
                }

                PostBody::Delete { hashes }
            }
            INFO_POST => {
                // Create an empty vector to store key-value pairs.
                let mut info: Vec<UserInfo> = Vec::new();

                // Since there may be several key-value pairs, we use a loop
                // to iterate over the bytes.
                loop {
                    // Read the key length byte and increment the offset.
                    let (s, key_len) = varint::decode(&buf[offset..])?;
                    offset += s;

                    // A key length value of 0 indicates that there are no
                    // more key-value pairs to come.
                    if key_len == 0 {
                        // Break out of the loop.
                        break;
                    }

                    // Read the key bytes and increment the offset.
                    let key = String::from_utf8(buf[offset..offset + key_len as usize].to_vec())?;
                    offset += key_len as usize;

                    // Read the val length byte and increment the offset.
                    let (s, val_len) = varint::decode(&buf[offset..])?;
                    offset += s;

                    // Read the val bytes and increment the offset.
                    let val = String::from_utf8(buf[offset..offset + val_len as usize].to_vec())?;
                    offset += val_len as usize;

                    let key_val = if key == "name" {
                        // This method also performs validation.
                        UserInfo::name(val)?
                    } else {
                        UserInfo::new(key, val)
                    };

                    info.push(key_val);
                }

                PostBody::Info { info }
            }
            TOPIC_POST => {
                // Read the channel length byte and increment the offset.
                let (s, channel_len) = varint::decode(&buf[offset..])?;
                offset += s;

                // Read the channel bytes.
                let channel =
                    String::from_utf8(buf[offset..offset + channel_len as usize].to_vec())?;
                // Validate the length of the channel name.
                validation::validate_channel(&channel)?;
                // Increment the offset.
                offset += channel_len as usize;

                // Read the topic length byte and increment the offset.
                let (s, topic_len) = varint::decode(&buf[offset..])?;
                offset += s;

                // Read the topic bytes.
                let topic = String::from_utf8(buf[offset..offset + topic_len as usize].to_vec())?;
                // Validate the length of the topic.
                validation::validate_topic(&topic)?;
                // Increment the offset.
                offset += topic_len as usize;

                PostBody::Topic { channel, topic }
            }
            JOIN_POST => {
                // Read the channel length byte and increment the offset.
                let (s, channel_len) = varint::decode(&buf[offset..])?;
                offset += s;

                // Read the channel bytes.
                let channel =
                    String::from_utf8(buf[offset..offset + channel_len as usize].to_vec())?;
                // Validate the length of the channel name.
                validation::validate_channel(&channel)?;
                // Increment the offset.
                offset += channel_len as usize;

                PostBody::Join { channel }
            }
            LEAVE_POST => {
                // Read the channel length byte and increment the offset.
                let (s, channel_len) = varint::decode(&buf[offset..])?;
                offset += s;

                // Read the channel bytes.
                let channel =
                    String::from_utf8(buf[offset..offset + channel_len as usize].to_vec())?;
                // Validate the length of the channel name.
                validation::validate_channel(&channel)?;
                // Increment the offset.
                offset += channel_len as usize;

                PostBody::Leave { channel }
            }
            // Unrecognized.
            post_type => PostBody::Unrecognized { post_type },
        };

        Ok((offset, Post { header, body }))
    }
}

impl CountBytes for Post {
    /// Calculate the total number of bytes comprising the encoded post.
    fn count_bytes(&self) -> usize {
        let post_type = self.post_type();

        // Count the post header bytes.
        let header_size = 32 // Public key.
            + 64 // Signature.
            + varint::length(self.header.links.len() as u64) // Number of links.
            + self.header.links.len() * 32 // Links.
            + varint::length(post_type) // Post type.
            + varint::length(self.header.timestamp); // Timestamp.

        // Count the post body bytes.
        let body_size = match &self.body {
            PostBody::Text { channel, text } => {
                varint::length(channel.len() as u64)
                    + channel.len()
                    + varint::length(text.len() as u64)
                    + text.len()
            }
            PostBody::Delete { hashes } => varint::length(hashes.len() as u64) + hashes.len() * 32,
            PostBody::Info { info } => {
                info.iter().fold(0, |sum, info| {
                    sum + varint::length(info.key.len() as u64)
                        + info.key.len()
                        + varint::length(info.val.len() as u64)
                        + info.val.len()
                }) + varint::length(0)
            }
            PostBody::Topic { channel, topic } => {
                varint::length(channel.len() as u64)
                    + channel.len()
                    + varint::length(topic.len() as u64)
                    + topic.len()
            }
            PostBody::Join { channel } => varint::length(channel.len() as u64) + channel.len(),
            PostBody::Leave { channel } => varint::length(channel.len() as u64) + channel.len(),
            PostBody::Unrecognized { .. } => 0,
        };

        header_size + body_size
    }

    fn count_from_bytes(buf: &[u8]) -> Result<usize, Error> {
        if buf.is_empty() {
            return CableErrorKind::MessageEmpty {}.raise();
        }

        let (sum, post_len) = varint::decode(buf)?;

        Ok(sum + (post_len as usize))
    }
}

#[cfg(test)]
mod test {
    use super::{
        Error, FromBytes, Hash, Post, PostBody, PostHeader, ToBytes, UserInfo, DELETE_POST,
        INFO_POST, JOIN_POST, LEAVE_POST, TEXT_POST, TOPIC_POST,
    };

    use hex::FromHex;

    // Field values sourced from https://github.com/cabal-club/cable.js#examples.

    const PUBLIC_KEY: &str = "25b272a71555322d40efe449a7f99af8fd364b92d350f1664481b2da340a02d0";
    const POST_HASH: &str = "5049d089a650aa896cb25ec35258653be4df196b4a5e5b6db7ed024aaa89e1b3";
    const HASH_1: &str = "15ed54965515babf6f16be3f96b04b29ecca813a343311dae483691c07ccf4e5";
    const HASH_2: &str = "97fc63631c41384226b9b68d9f73ffaaf6eac54b71838687f48f112e30d6db68";
    const HASH_3: &str = "9c2939fec6d47b00bafe6967aeff697cf4b5abca01b04ba1b31a7e3752454bfa";

    const TEXT_POST_HEX_BINARY: &str = "25b272a71555322d40efe449a7f99af8fd364b92d350f1664481b2da340a02d06725733046b35fa3a7e8dc0099a2b3dff10d3fd8b0f6da70d094352e3f5d27a8bc3f5586cf0bf71befc22536c3c50ec7b1d64398d43c3f4cde778e579e88af05015049d089a650aa896cb25ec35258653be4df196b4a5e5b6db7ed024aaa89e1b300500764656661756c740d68e282ac6c6c6f20776f726c64";
    const DELETE_POST_HEX_BINARY: &str = "25b272a71555322d40efe449a7f99af8fd364b92d350f1664481b2da340a02d0affe77e3b3156cda7feea042269bb7e93f5031662c70610d37baa69132b4150c18d67cb2ac24fb0f9be0a6516e53ba2f3bbc5bd8e7a1bff64d9c78ce0c2e4205015049d089a650aa896cb25ec35258653be4df196b4a5e5b6db7ed024aaa89e1b301500315ed54965515babf6f16be3f96b04b29ecca813a343311dae483691c07ccf4e597fc63631c41384226b9b68d9f73ffaaf6eac54b71838687f48f112e30d6db689c2939fec6d47b00bafe6967aeff697cf4b5abca01b04ba1b31a7e3752454bfa";
    const INFO_POST_HEX_BINARY: &str = "25b272a71555322d40efe449a7f99af8fd364b92d350f1664481b2da340a02d04ccb1c0063ef09a200e031ee89d874bcc99f3e6fd8fd667f5e28f4dbcf4b7de6bb1ce37d5f01cc055a7b70cef175d30feeb34531db98c91fa8b3fa4d7c5fd307015049d089a650aa896cb25ec35258653be4df196b4a5e5b6db7ed024aaa89e1b30250046e616d65066361626c657200";
    const TOPIC_POST_HEX_BINARY: &str = "25b272a71555322d40efe449a7f99af8fd364b92d350f1664481b2da340a02d0bf7578e781caee4ca708281645b291a2100c4f2138f0e0ac98bc2b4a414b4ba8dca08285751114b05f131421a1745b648c43b17b05392593237dfacc8dff5208015049d089a650aa896cb25ec35258653be4df196b4a5e5b6db7ed024aaa89e1b303500764656661756c743b696e74726f6475636520796f757273656c6620746f2074686520667269656e646c792063726f7764206f66206c696b656d696e64656420666f6c78";
    const JOIN_POST_HEX_BINARY: &str = "25b272a71555322d40efe449a7f99af8fd364b92d350f1664481b2da340a02d064425f10fa34c1e14b6101491772d3c5f15f720a952dd56c27d5ad52f61f695130ce286de73e332612b36242339b61c9e12397f5dcc94c79055c7e1cb1dbfb08015049d089a650aa896cb25ec35258653be4df196b4a5e5b6db7ed024aaa89e1b304500764656661756c74";
    const LEAVE_POST_HEX_BINARY: &str = "25b272a71555322d40efe449a7f99af8fd364b92d350f1664481b2da340a02d0abb083ecdca569f064564942ddf1944fbf550dc27ea36a7074be798d753cb029703de77b1a9532b6ca2ec5706e297dce073d6e508eeb425c32df8431e4677805015049d089a650aa896cb25ec35258653be4df196b4a5e5b6db7ed024aaa89e1b305500764656661756c74";

    #[test]
    fn verify_post() -> Result<(), Error> {
        // Encoded text post.
        let buffer = <Vec<u8>>::from_hex(TEXT_POST_HEX_BINARY)?;

        let result = Post::verify(&buffer);
        assert_eq!(result, true);

        Ok(())
    }

    #[test]
    fn get_channel_from_join_post() -> Result<(), Error> {
        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("64425f10fa34c1e14b6101491772d3c5f15f720a952dd56c27d5ad52f61f695130ce286de73e332612b36242339b61c9e12397f5dcc94c79055c7e1cb1dbfb08")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = JOIN_POST;
        let timestamp = 80;

        // Construct a new post header.
        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        let channel = "default".to_string();

        // Construct a new post body.
        let body = PostBody::Join {
            channel: channel.clone(),
        };

        // Construct a new post.
        let post = Post::new(header, body);

        if let Some(retrieved_channel) = post.get_channel() {
            assert_eq!(retrieved_channel, &channel)
        } else {
            panic!("Failed to retrieve channel from join post");
        }

        Ok(())
    }

    #[test]
    fn get_timestamp_from_leave_post() -> Result<(), Error> {
        // TODO: Write a helper function to provide shared values.
        // E.g. public_key, links and timestamp.

        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("abb083ecdca569f064564942ddf1944fbf550dc27ea36a7074be798d753cb029703de77b1a9532b6ca2ec5706e297dce073d6e508eeb425c32df8431e4677805")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = LEAVE_POST;
        let timestamp = 80;

        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        let body = PostBody::Leave {
            channel: "default".to_string(),
        };

        let post = Post { header, body };

        let retrieved_timestamp = post.get_timestamp();

        assert_eq!(retrieved_timestamp, timestamp);

        Ok(())
    }

    #[test]
    fn is_signed_text_post() -> Result<(), Error> {
        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("6725733046b35fa3a7e8dc0099a2b3dff10d3fd8b0f6da70d094352e3f5d27a8bc3f5586cf0bf71befc22536c3c50ec7b1d64398d43c3f4cde778e579e88af05")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = TEXT_POST;
        let timestamp = 80;

        // Construct a new post header.
        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        let channel = "default".to_string();
        let text = "h€llo world".to_string();

        // Construct a new post body.
        let body = PostBody::Text { channel, text };

        // Construct a new post.
        let post = Post::new(header, body);

        // Ensure the post is signed.
        assert!(post.is_signed());

        Ok(())
    }

    /* POST TO BYTES TESTS */

    #[test]
    fn text_post_to_bytes() -> Result<(), Error> {
        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("6725733046b35fa3a7e8dc0099a2b3dff10d3fd8b0f6da70d094352e3f5d27a8bc3f5586cf0bf71befc22536c3c50ec7b1d64398d43c3f4cde778e579e88af05")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = TEXT_POST;
        let timestamp = 80;

        // Construct a new post header.
        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        let channel = "default".to_string();
        let text = "h€llo world".to_string();

        // Construct a new post body.
        let body = PostBody::Text { channel, text };

        // Construct a new post.
        let post = Post::new(header, body);
        // Convert the post to bytes.
        let post_bytes = post.to_bytes()?;

        // Test vector binary.
        let expected_bytes = <Vec<u8>>::from_hex(TEXT_POST_HEX_BINARY)?;

        // Ensure the number of generated post bytes matches the number of
        // expected bytes.
        assert_eq!(post_bytes.len(), expected_bytes.len());

        // Ensure the generated post bytes match the expected bytes.
        assert_eq!(post_bytes, expected_bytes);

        Ok(())
    }

    #[test]
    fn delete_post_to_bytes() -> Result<(), Error> {
        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("affe77e3b3156cda7feea042269bb7e93f5031662c70610d37baa69132b4150c18d67cb2ac24fb0f9be0a6516e53ba2f3bbc5bd8e7a1bff64d9c78ce0c2e4205")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = DELETE_POST;
        let timestamp = 80;

        // Construct a new post header.
        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        // Create a vector of hashes.
        let hashes: Vec<Hash> = vec![
            <[u8; 32]>::from_hex(HASH_1)?,
            <[u8; 32]>::from_hex(HASH_2)?,
            <[u8; 32]>::from_hex(HASH_3)?,
        ];

        // Construct a new post body.
        let body = PostBody::Delete { hashes };

        // Construct a new post.
        let post = Post::new(header, body);
        // Convert the post to bytes.
        let post_bytes = post.to_bytes()?;

        // Test vector binary.
        let expected_bytes = <Vec<u8>>::from_hex(DELETE_POST_HEX_BINARY)?;

        // Ensure the number of generated post bytes matches the number of
        // expected bytes.
        assert_eq!(post_bytes.len(), expected_bytes.len());

        // Ensure the generated post bytes match the expected bytes.
        assert_eq!(post_bytes, expected_bytes);

        Ok(())
    }

    #[test]
    fn info_post_to_bytes() -> Result<(), Error> {
        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("4ccb1c0063ef09a200e031ee89d874bcc99f3e6fd8fd667f5e28f4dbcf4b7de6bb1ce37d5f01cc055a7b70cef175d30feeb34531db98c91fa8b3fa4d7c5fd307")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = INFO_POST;
        let timestamp = 80;

        // Construct a new post header.
        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        let key = "name";
        let val = "cabler";
        let user_info = UserInfo::new(key, val);

        // Construct a new post body.
        let body = PostBody::Info {
            info: vec![user_info],
        };

        // Construct a new post.
        let post = Post::new(header, body);
        // Convert the post to bytes.
        let post_bytes = post.to_bytes()?;

        // Test vector binary.
        let expected_bytes = <Vec<u8>>::from_hex(INFO_POST_HEX_BINARY)?;

        // Ensure the number of generated post bytes matches the number of
        // expected bytes.
        assert_eq!(post_bytes.len(), expected_bytes.len());

        // Ensure the generated post bytes match the expected bytes.
        assert_eq!(post_bytes, expected_bytes);

        Ok(())
    }

    #[test]
    fn topic_post_to_bytes() -> Result<(), Error> {
        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("bf7578e781caee4ca708281645b291a2100c4f2138f0e0ac98bc2b4a414b4ba8dca08285751114b05f131421a1745b648c43b17b05392593237dfacc8dff5208")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = TOPIC_POST;
        let timestamp = 80;

        // Construct a new post header.
        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        let channel = "default".to_string();
        let topic = "introduce yourself to the friendly crowd of likeminded folx".to_string();

        // Construct a new post body.
        let body = PostBody::Topic {
            channel: channel.into(),
            topic: topic.into(),
        };

        // Construct a new post.
        let post = Post::new(header, body);
        // Convert the post to bytes.
        let post_bytes = post.to_bytes()?;

        // Test vector binary.
        let expected_bytes = <Vec<u8>>::from_hex(TOPIC_POST_HEX_BINARY)?;

        // Ensure the number of generated post bytes matches the number of
        // expected bytes.
        assert_eq!(post_bytes.len(), expected_bytes.len());

        // Ensure the generated post bytes match the expected bytes.
        assert_eq!(post_bytes, expected_bytes);

        Ok(())
    }

    #[test]
    fn join_post_to_bytes() -> Result<(), Error> {
        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("64425f10fa34c1e14b6101491772d3c5f15f720a952dd56c27d5ad52f61f695130ce286de73e332612b36242339b61c9e12397f5dcc94c79055c7e1cb1dbfb08")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = JOIN_POST;
        let timestamp = 80;

        // Construct a new post header.
        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        let channel = "default".to_string();

        // Construct a new post body.
        let body = PostBody::Join {
            channel: channel.into(),
        };

        // Construct a new post.
        let post = Post::new(header, body);
        // Convert the post to bytes.
        let post_bytes = post.to_bytes()?;

        // Test vector binary.
        let expected_bytes = <Vec<u8>>::from_hex(JOIN_POST_HEX_BINARY)?;

        // Ensure the number of generated post bytes matches the number of
        // expected bytes.
        assert_eq!(post_bytes.len(), expected_bytes.len());

        // Ensure the generated post bytes match the expected bytes.
        assert_eq!(post_bytes, expected_bytes);

        Ok(())
    }

    #[test]
    fn leave_post_to_bytes() -> Result<(), Error> {
        /* HEADER FIELD VALUES */

        let public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let signature = <[u8; 64]>::from_hex("abb083ecdca569f064564942ddf1944fbf550dc27ea36a7074be798d753cb029703de77b1a9532b6ca2ec5706e297dce073d6e508eeb425c32df8431e4677805")?;
        let links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let post_type = LEAVE_POST;
        let timestamp = 80;

        // Construct a new post header.
        let header = PostHeader::new(public_key, signature, links, post_type, timestamp);

        /* BODY FIELD VALUES */

        let channel = "default".to_string();

        // Construct a new post body.
        let body = PostBody::Leave {
            channel: channel.into(),
        };

        // Construct a new post.
        let post = Post::new(header, body);
        // Convert the post to bytes.
        let post_bytes = post.to_bytes()?;

        // Test vector binary.
        let expected_bytes = <Vec<u8>>::from_hex(LEAVE_POST_HEX_BINARY)?;

        // Ensure the number of generated post bytes matches the number of
        // expected bytes.
        assert_eq!(post_bytes.len(), expected_bytes.len());

        // Ensure the generated post bytes match the expected bytes.
        assert_eq!(post_bytes, expected_bytes);

        Ok(())
    }

    /* BYTES TO POST TESTS */

    #[test]
    fn bytes_to_text_post() -> Result<(), Error> {
        // Test vector binary.
        let post_bytes = <Vec<u8>>::from_hex(TEXT_POST_HEX_BINARY)?;

        // Decode the byte slice to a `Post`.
        let (_, post) = Post::from_bytes(&post_bytes)?;

        /* HEADER FIELD VALUES */

        let expected_public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let expected_signature = <[u8; 64]>::from_hex("6725733046b35fa3a7e8dc0099a2b3dff10d3fd8b0f6da70d094352e3f5d27a8bc3f5586cf0bf71befc22536c3c50ec7b1d64398d43c3f4cde778e579e88af05")?;
        let expected_links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let expected_post_type = TEXT_POST;
        let expected_timestamp = 80;

        let PostHeader {
            public_key,
            signature,
            links,
            post_type,
            timestamp,
        } = post.header;

        // Ensure the post header fields are correct.
        assert_eq!(public_key, expected_public_key);
        assert_eq!(signature, expected_signature);
        assert_eq!(links, expected_links);
        assert_eq!(post_type, expected_post_type);
        assert_eq!(timestamp, expected_timestamp);

        /* BODY FIELD VALUES */

        let expected_channel = "default".to_string();
        let expected_text = "h€llo world".to_string();

        // Ensure the post body fields are correct.
        if let PostBody::Text { channel, text } = post.body {
            assert_eq!(channel, expected_channel);
            assert_eq!(text, expected_text);
        } else {
            panic!("Incorrect post type: expected text");
        }

        Ok(())
    }

    #[test]
    fn bytes_to_delete_post() -> Result<(), Error> {
        // Test vector binary.
        let post_bytes = <Vec<u8>>::from_hex(DELETE_POST_HEX_BINARY)?;

        // Decode the byte slice to a `Post`.
        let (_, post) = Post::from_bytes(&post_bytes)?;

        /* HEADER FIELD VALUES */

        let expected_public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let expected_signature = <[u8; 64]>::from_hex("affe77e3b3156cda7feea042269bb7e93f5031662c70610d37baa69132b4150c18d67cb2ac24fb0f9be0a6516e53ba2f3bbc5bd8e7a1bff64d9c78ce0c2e4205")?;
        let expected_links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let expected_post_type = DELETE_POST;
        let expected_timestamp = 80;

        let PostHeader {
            public_key,
            signature,
            links,
            post_type,
            timestamp,
        } = post.header;

        // Ensure the post header fields are correct.
        assert_eq!(public_key, expected_public_key);
        assert_eq!(signature, expected_signature);
        assert_eq!(links, expected_links);
        assert_eq!(post_type, expected_post_type);
        assert_eq!(timestamp, expected_timestamp);

        /* BODY FIELD VALUES */

        // Create a vector of hashes.
        let expected_hashes: Vec<Hash> = vec![
            <[u8; 32]>::from_hex(HASH_1)?,
            <[u8; 32]>::from_hex(HASH_2)?,
            <[u8; 32]>::from_hex(HASH_3)?,
        ];

        // Ensure the post body fields are correct.
        if let PostBody::Delete { hashes } = post.body {
            assert_eq!(hashes, expected_hashes);
        } else {
            panic!("Incorrect post type: expected delete");
        }

        Ok(())
    }

    #[test]
    fn bytes_to_info_post() -> Result<(), Error> {
        // Test vector binary.
        let post_bytes = <Vec<u8>>::from_hex(INFO_POST_HEX_BINARY)?;

        // Decode the byte slice to a `Post`.
        let (_, post) = Post::from_bytes(&post_bytes)?;

        /* HEADER FIELD VALUES */

        let expected_public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        //let expected_signature = <[u8; 64]>::from_hex("f70273779147a3b756407d5660ed2e8e2975abc5ab224fb152aa2bfb3dd331740a66e0718cd580bc94978c1c3cd4524ad8cb2f4cca80df481010c3ef834ac700")?;
        let expected_signature = <[u8; 64]>::from_hex("4ccb1c0063ef09a200e031ee89d874bcc99f3e6fd8fd667f5e28f4dbcf4b7de6bb1ce37d5f01cc055a7b70cef175d30feeb34531db98c91fa8b3fa4d7c5fd307")?;
        let expected_links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let expected_post_type = INFO_POST;
        let expected_timestamp = 80;

        let PostHeader {
            public_key,
            signature,
            links,
            post_type,
            timestamp,
        } = post.header;

        // Ensure the post header fields are correct.
        assert_eq!(public_key, expected_public_key);
        assert_eq!(signature, expected_signature);
        assert_eq!(links, expected_links);
        assert_eq!(post_type, expected_post_type);
        assert_eq!(timestamp, expected_timestamp);

        /* BODY FIELD VALUES */

        let key = "name";
        let val = "cabler";
        let expected_info = vec![UserInfo::new(key, val)];

        // Ensure the post body fields are correct.
        if let PostBody::Info { info } = post.body {
            assert_eq!(info, expected_info);
        } else {
            panic!("Incorrect post type: expected info");
        }

        Ok(())
    }

    #[test]
    fn bytes_to_topic_post() -> Result<(), Error> {
        // Test vector binary.
        let post_bytes = <Vec<u8>>::from_hex(TOPIC_POST_HEX_BINARY)?;

        // Decode the byte slice to a `Post`.
        let (_, post) = Post::from_bytes(&post_bytes)?;

        /* HEADER FIELD VALUES */

        let expected_public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let expected_signature = <[u8; 64]>::from_hex("bf7578e781caee4ca708281645b291a2100c4f2138f0e0ac98bc2b4a414b4ba8dca08285751114b05f131421a1745b648c43b17b05392593237dfacc8dff5208")?;
        let expected_links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let expected_post_type = TOPIC_POST;
        let expected_timestamp = 80;

        let PostHeader {
            public_key,
            signature,
            links,
            post_type,
            timestamp,
        } = post.header;

        // Ensure the post header fields are correct.
        assert_eq!(public_key, expected_public_key);
        assert_eq!(signature, expected_signature);
        assert_eq!(links, expected_links);
        assert_eq!(post_type, expected_post_type);
        assert_eq!(timestamp, expected_timestamp);

        /* BODY FIELD VALUES */

        let expected_channel = "default".to_string();
        let expected_topic =
            "introduce yourself to the friendly crowd of likeminded folx".to_string();

        // Ensure the post body fields are correct.
        if let PostBody::Topic { channel, topic } = post.body {
            assert_eq!(channel, expected_channel);
            assert_eq!(topic, expected_topic);
        } else {
            panic!("Incorrect post type: expected topic");
        }

        Ok(())
    }

    #[test]
    fn bytes_to_join_post() -> Result<(), Error> {
        // Test vector binary.
        let post_bytes = <Vec<u8>>::from_hex(JOIN_POST_HEX_BINARY)?;

        // Decode the byte slice to a `Post`.
        let (_, post) = Post::from_bytes(&post_bytes)?;

        /* HEADER FIELD VALUES */

        let expected_public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let expected_signature = <[u8; 64]>::from_hex("64425f10fa34c1e14b6101491772d3c5f15f720a952dd56c27d5ad52f61f695130ce286de73e332612b36242339b61c9e12397f5dcc94c79055c7e1cb1dbfb08")?;
        let expected_links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let expected_post_type = JOIN_POST;
        let expected_timestamp = 80;

        let PostHeader {
            public_key,
            signature,
            links,
            post_type,
            timestamp,
        } = post.header;

        // Ensure the post header fields are correct.
        assert_eq!(public_key, expected_public_key);
        assert_eq!(signature, expected_signature);
        assert_eq!(links, expected_links);
        assert_eq!(post_type, expected_post_type);
        assert_eq!(timestamp, expected_timestamp);

        /* BODY FIELD VALUES */

        let expected_channel = "default".to_string();

        // Ensure the post body fields are correct.
        if let PostBody::Join { channel } = post.body {
            assert_eq!(channel, expected_channel);
        } else {
            panic!("Incorrect post type: expected join");
        }

        Ok(())
    }

    #[test]
    fn bytes_to_leave_post() -> Result<(), Error> {
        // Test vector binary.
        let post_bytes = <Vec<u8>>::from_hex(LEAVE_POST_HEX_BINARY)?;

        // Decode the byte slice to a `Post`.
        let (_, post) = Post::from_bytes(&post_bytes)?;

        /* HEADER FIELD VALUES */

        let expected_public_key = <[u8; 32]>::from_hex(PUBLIC_KEY)?;
        let expected_signature = <[u8; 64]>::from_hex("abb083ecdca569f064564942ddf1944fbf550dc27ea36a7074be798d753cb029703de77b1a9532b6ca2ec5706e297dce073d6e508eeb425c32df8431e4677805")?;
        let expected_links = vec![<[u8; 32]>::from_hex(POST_HASH)?];
        let expected_post_type = LEAVE_POST;
        let expected_timestamp = 80;

        let PostHeader {
            public_key,
            signature,
            links,
            post_type,
            timestamp,
        } = post.header;

        // Ensure the post header fields are correct.
        assert_eq!(public_key, expected_public_key);
        assert_eq!(signature, expected_signature);
        assert_eq!(links, expected_links);
        assert_eq!(post_type, expected_post_type);
        assert_eq!(timestamp, expected_timestamp);

        /* BODY FIELD VALUES */

        let expected_channel = "default".to_string();

        // Ensure the post body fields are correct.
        if let PostBody::Leave { channel } = post.body {
            assert_eq!(channel, expected_channel);
        } else {
            panic!("Incorrect post type: expected leave");
        }

        Ok(())
    }
}
