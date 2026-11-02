//! Windows CNG verifies P-521 certificate signatures not supported by ring.
//! WebPKI still verifies the entire chain, host name, validity and constraints.
use rustls_pki_types::{alg_id, AlgorithmIdentifier, CertificateDer, InvalidSignature, SignatureVerificationAlgorithm};
use sha2::{Digest, Sha384, Sha256};
use std::{ffi::c_void, ptr, sync::{Arc, OnceLock}};
use tokio_rustls::rustls::{self, crypto::{CryptoProvider, WebPkiSupportedAlgorithms}};

type Handle = *mut c_void;
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptOpenAlgorithmProvider(out: *mut Handle, algorithm: *const u16, implementation: *const u16, flags: u32) -> i32;
    fn BCryptCloseAlgorithmProvider(algorithm: Handle, flags: u32) -> i32;
    fn BCryptImportKeyPair(algorithm: Handle, import: Handle, kind: *const u16, out: *mut Handle, blob: *const u8, length: u32, flags: u32) -> i32;
    fn BCryptDestroyKey(key: Handle) -> i32;
    fn BCryptVerifySignature(key: Handle, padding: *const c_void, hash: *const u8, hash_length: u32, signature: *const u8, signature_length: u32, flags: u32) -> i32;
}

#[derive(Debug)]
struct P521Sha384(bool);
static P521_SHA384: P521Sha384 = P521Sha384(false);
static P521_SHA256: P521Sha384 = P521Sha384(true);

// Certificate ECDSA signatures are DER INTEGER pairs; CNG expects fixed-width r||s.
fn cng_signature(der: &[u8]) -> Result<[u8; 132], InvalidSignature> {
    let mut input = der;
    fn byte(input: &mut &[u8]) -> Result<u8, InvalidSignature> {
        let (&first, rest) = input.split_first().ok_or(InvalidSignature)?;
        *input = rest;
        Ok(first)
    }
    if byte(&mut input)? != 0x30 { return Err(InvalidSignature); }
    let length = match byte(&mut input)? {
        0x81 => {
            let length = byte(&mut input)? as usize;
            if length < 128 { return Err(InvalidSignature); }
            length
        }
        length if length < 128 => length as usize,
        _ => return Err(InvalidSignature),
    };
    if length != input.len() { return Err(InvalidSignature); }
    let mut result = [0; 132];
    for output in result.chunks_mut(66) {
        if byte(&mut input)? != 2 { return Err(InvalidSignature); }
        let length = byte(&mut input)? as usize;
        if length == 0 || length > 67 || length > input.len() { return Err(InvalidSignature); }
        let (mut number, rest) = input.split_at(length);
        input = rest;
        if number[0] & 0x80 != 0 { return Err(InvalidSignature); }
        if number[0] == 0 && number.len() > 1 {
            if number[1] & 0x80 == 0 { return Err(InvalidSignature); }
            number = &number[1..];
        }
        if number.len() > 66 || (number.len() == 66 && number[0] > 1) { return Err(InvalidSignature); }
        output[66 - number.len()..].copy_from_slice(number);
    }
    if !input.is_empty() { return Err(InvalidSignature); }
    Ok(result)
}

impl SignatureVerificationAlgorithm for P521Sha384 {
    fn verify_signature(&self, public_key: &[u8], message: &[u8], signature: &[u8]) -> Result<(), InvalidSignature> {
        if public_key.len() != 133 || public_key[0] != 4 { return Err(InvalidSignature); }
        let signature = cng_signature(signature)?;
        let hash = if self.0 { Sha256::digest(message).to_vec() } else { Sha384::digest(message).to_vec() };
        let mut blob = Vec::with_capacity(140);
        blob.extend_from_slice(&0x35534345_u32.to_le_bytes()); // BCRYPT_ECDSA_PUBLIC_P521_MAGIC
        blob.extend_from_slice(&66_u32.to_le_bytes());
        blob.extend_from_slice(&public_key[1..]);
        let algorithm: Vec<u16> = "ECDSA_P521\0".encode_utf16().collect();
        let kind: Vec<u16> = "ECCPUBLICBLOB\0".encode_utf16().collect();
        let mut provider = ptr::null_mut();
        let mut key = ptr::null_mut();
        // All buffers remain alive for these synchronous calls; CNG validates the curve point.
        unsafe {
            if BCryptOpenAlgorithmProvider(&mut provider, algorithm.as_ptr(), ptr::null(), 0) < 0 { return Err(InvalidSignature); }
            let imported = BCryptImportKeyPair(provider, ptr::null_mut(), kind.as_ptr(), &mut key, blob.as_ptr(), blob.len() as u32, 0);
            let verified = imported >= 0 && BCryptVerifySignature(key, ptr::null(), hash.as_ptr(), hash.len() as u32, signature.as_ptr(), signature.len() as u32, 0) >= 0;
            if !key.is_null() { BCryptDestroyKey(key); }
            BCryptCloseAlgorithmProvider(provider, 0);
            if verified { Ok(()) } else { Err(InvalidSignature) }
        }
    }
    fn public_key_alg_id(&self) -> AlgorithmIdentifier { alg_id::ECDSA_P521 }
    fn signature_alg_id(&self) -> AlgorithmIdentifier { if self.0 { alg_id::ECDSA_SHA256 } else { alg_id::ECDSA_SHA384 } }
}

pub(super) fn client_config(extra: Option<CertificateDer<'static>>) -> crate::ResultType<rustls::ClientConfig> {
    static PROVIDER: OnceLock<Arc<CryptoProvider>> = OnceLock::new();
    let provider = PROVIDER.get_or_init(|| {
        let mut provider = rustls::crypto::ring::default_provider();
        let mut algorithms = provider.signature_verification_algorithms.all.to_vec();
        algorithms.push(&P521_SHA384);
        algorithms.push(&P521_SHA256);
        provider.signature_verification_algorithms = WebPkiSupportedAlgorithms {
            all: Box::leak(algorithms.into_boxed_slice()),
            mapping: provider.signature_verification_algorithms.mapping,
        };
        Arc::new(provider)
    }).clone();
    let verifier = crate::verifier::webpki_server_verifier(provider.clone(), extra)?;
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_webpki_verifier(verifier)
        .with_no_client_auth())
}
