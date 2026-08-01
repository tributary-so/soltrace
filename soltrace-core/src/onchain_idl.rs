//! On-chain Anchor IDL fetch via the program-metadata program.
//!
//! Canonical IDL PDA seeds: `[program, "idl" zero-padded to 16 bytes]` under
//! `PROGRAM_METADATA_ID`. Decodes `Metadata` accounts whose `data_source ==
//! Direct` (inflate per compression, decode per encoding, parse JSON).
//! Non-Direct / wrong-program accounts yield `None`; decode failures propagate
//! as `Err` so callers can warn-and-continue without clobbering a known-good
//! parser.
use crate::{
    error::{Result, SoltraceError},
    types::ParsedIdl,
};
use flate2::read::{GzDecoder, ZlibDecoder};
use solana_sdk::{pubkey, pubkey::Pubkey};
use spl_program_metadata_client::{
    accounts::Metadata,
    types::{Compression, DataSource, Encoding},
};
use std::io::Read;

/// program-metadata on-chain program ID.
pub const PROGRAM_METADATA_ID: Pubkey = pubkey!("ProgM6JCCvbYkfKqJYHePx4xxSUSqJp7rh8Lyv7nk7S");

/// Derive the canonical IDL PDA for `program`.
///
/// Seeds `[program, "idl"+zero-pad-16]`. This matches the on-chain program's
/// canonical derivation in `program-metadata/program/tests/setup/initialize.rs`;
/// the authority-scoped form (`[program, &[], seed]`) is equivalent because an
/// empty seed slice contributes nothing to the PDA hash.
pub fn derive_canonical_idl_pda(program: &Pubkey) -> Pubkey {
    let mut seed = [0u8; 16];
    seed[..3].copy_from_slice(b"idl");
    Pubkey::find_program_address(&[program.as_ref(), &seed], &PROGRAM_METADATA_ID).0
}

/// Decode a program-metadata `Metadata` account's raw bytes into a [`ParsedIdl`].
///
/// Returns `Ok(None)` when the account is not a Direct IDL for
/// `expected_program` (mismatched `program` field or non-Direct
/// `data_source`). Returns `Err` on borsh/decompress/JSON decode failure so
/// the caller can log the decode error and retain any last-known-good parser.
// ponytail: takes raw &[u8] not &Account — get_account returns a different
// solana-account crate version than solana_sdk::Account (the metadata client
// pins solana-sdk 2.x, this workspace pins 4.0). `.data` is all we read, so
// bytes at the boundary avoids marshalling two distinct Account types.
pub fn decode_metadata_account(
    data: &[u8],
    expected_program: &Pubkey,
) -> Result<Option<ParsedIdl>> {
    let meta = Metadata::from_bytes(data)?;
    if meta.program.as_ref() != expected_program.as_ref() {
        return Ok(None);
    }
    if !matches!(meta.data_source, DataSource::Direct) {
        return Ok(None);
    }
    let bytes = inflate(meta.compression, &meta.data)?;
    let decoded = decode_bytes(meta.encoding, &bytes)?;
    let idl: ParsedIdl =
        serde_json::from_slice(&decoded).map_err(|e| SoltraceError::IdlParse(e.to_string()))?;
    Ok(Some(idl))
}

/// Fetch the canonical on-chain IDL for `program`.
///
/// RPC failure (account absent, network error) maps to `Ok(None)` — no error
/// propagation, per the startup warn-and-continue contract. A present account
/// that fails to decode propagates `Err`.
pub fn fetch_canonical_idl(
    rpc: &solana_rpc_client::rpc_client::RpcClient,
    program: &Pubkey,
) -> Result<Option<ParsedIdl>> {
    let pda = derive_canonical_idl_pda(program);
    match rpc.get_account(&pda) {
        Ok(account) => decode_metadata_account(&account.data, program),
        Err(_) => Ok(None),
    }
}

fn inflate(compression: Compression, data: &[u8]) -> Result<Vec<u8>> {
    match compression {
        Compression::None => Ok(data.to_vec()),
        Compression::Gzip => read_all(GzDecoder::new(data)),
        Compression::Zlib => read_all(ZlibDecoder::new(data)),
    }
}

fn read_all(mut reader: impl Read) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    reader.read_to_end(&mut out)?;
    Ok(out)
}

fn decode_bytes(encoding: Encoding, data: &[u8]) -> Result<Vec<u8>> {
    match encoding {
        // None and Utf8 both carry the JSON as raw bytes; serde_json validates utf-8.
        Encoding::None | Encoding::Utf8 => Ok(data.to_vec()),
        Encoding::Base58 => {
            let s = std::str::from_utf8(data)
                .map_err(|e| SoltraceError::IdlParse(format!("base58 utf8: {e}")))?;
            solana_sdk::bs58::decode(s)
                .into_vec()
                .map_err(|e| SoltraceError::IdlParse(format!("base58 decode: {e}")))
        }
        Encoding::Base64 => {
            use base64::Engine;
            let s = std::str::from_utf8(data)
                .map_err(|e| SoltraceError::IdlParse(format!("base64 utf8: {e}")))?;
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(|e| SoltraceError::IdlParse(format!("base64 decode: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Synthesize a `Metadata` account blob from the borsh layout:
    /// `disc(1) program(32) authority(32) mutable(1) canonical(1) seed(16)
    /// encoding(1) compression(1) format(1) data_source(1) data_length(4) data`.
    /// Mirrors `spl_program_metadata_client::Metadata` byte-for-byte.
    fn build_metadata_account(
        program: &Pubkey,
        compression: Compression,
        data_source: DataSource,
        idl_plain: &[u8],
    ) -> Vec<u8> {
        let data: Vec<u8> = match compression {
            Compression::None => idl_plain.to_vec(),
            Compression::Zlib => {
                let mut e =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                e.write_all(idl_plain).unwrap();
                e.finish().unwrap()
            }
            Compression::Gzip => {
                let mut e =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                e.write_all(idl_plain).unwrap();
                e.finish().unwrap()
            }
        };
        let mut seed = [0u8; 16];
        seed[..3].copy_from_slice(b"idl");
        let mut buf = Vec::new();
        buf.push(2u8); // AccountDiscriminator::Metadata
        buf.extend_from_slice(program.as_ref()); // program (32)
        buf.extend_from_slice(&[0u8; 32]); // authority (ZeroableOption::None = zeros)
        buf.push(1); // mutable
        buf.push(1); // canonical
        buf.extend_from_slice(&seed); // seed (16)
        buf.push(1u8); // encoding = Utf8
        buf.push(compression as u8);
        buf.push(1u8); // format = Json
        buf.push(data_source as u8);
        buf.extend_from_slice(&(data.len() as u32).to_le_bytes()); // data_length
        buf.extend_from_slice(&data); // trailing data, no length prefix
        buf
    }

    const PROGRAM: Pubkey = pubkey!("TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ");
    const PROGRAM_STR: &str = "TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ";
    fn idl_json() -> &'static [u8] {
        br#"{"name":"Mini","events":[],"address":"TRibg8W8zmPHQqWtyAD1rEBRXEdyU13Mu6qX1Sg42tJ"}"#
    }

    #[test]
    fn pda_matches_independent_seed_construction() {
        // Mirrors program-metadata/program/tests/setup/initialize.rs canonical seeds.
        let mut seed = [0u8; 16];
        seed[..3].copy_from_slice(b"idl");
        let (expected, _) =
            Pubkey::find_program_address(&[PROGRAM.as_ref(), &seed], &PROGRAM_METADATA_ID);
        assert_eq!(derive_canonical_idl_pda(&PROGRAM), expected);
    }

    #[test]
    fn pda_equivalent_to_authority_scoped_empty_form() {
        // The otter-sec/anchor form uses [program, &[], seed]; an empty seed is a
        // no-op, so both derivations must agree byte-for-byte.
        let mut seed = [0u8; 16];
        seed[..3].copy_from_slice(b"idl");
        let (alt, _) =
            Pubkey::find_program_address(&[PROGRAM.as_ref(), &[], &seed], &PROGRAM_METADATA_ID);
        assert_eq!(derive_canonical_idl_pda(&PROGRAM), alt);
    }

    #[test]
    fn decode_zlib_utf8_direct_round_trips() {
        let blob =
            build_metadata_account(&PROGRAM, Compression::Zlib, DataSource::Direct, idl_json());
        let decoded = decode_metadata_account(&blob, &PROGRAM)
            .expect("decode ok")
            .expect("Some idl");
        assert_eq!(decoded.address, PROGRAM_STR);
        assert_eq!(decoded.name.as_deref(), Some("Mini"));
        assert!(decoded.events.is_empty());
    }

    #[test]
    fn decode_gzip_utf8_direct_round_trips() {
        let blob =
            build_metadata_account(&PROGRAM, Compression::Gzip, DataSource::Direct, idl_json());
        assert!(decode_metadata_account(&blob, &PROGRAM).unwrap().is_some());
    }

    #[test]
    fn decode_none_compression_direct_round_trips() {
        let blob =
            build_metadata_account(&PROGRAM, Compression::None, DataSource::Direct, idl_json());
        assert!(decode_metadata_account(&blob, &PROGRAM).unwrap().is_some());
    }

    #[test]
    fn decode_non_direct_data_source_returns_none() {
        let blob = build_metadata_account(&PROGRAM, Compression::Zlib, DataSource::Url, idl_json());
        assert!(decode_metadata_account(&blob, &PROGRAM).unwrap().is_none());
    }

    #[test]
    fn decode_wrong_program_returns_none() {
        let other = pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
        let blob =
            build_metadata_account(&PROGRAM, Compression::Zlib, DataSource::Direct, idl_json());
        assert!(decode_metadata_account(&blob, &other).unwrap().is_none());
    }

    #[test]
    fn fetch_returns_none_when_account_absent() {
        // ponytail: a port nothing listens on gives an instant connection-refused,
        // exercising the get_account Err -> Ok(None) path without a mock harness.
        let rpc = solana_rpc_client::rpc_client::RpcClient::new("http://127.0.0.1:1");
        assert!(fetch_canonical_idl(&rpc, &PROGRAM).unwrap().is_none());
    }
}
