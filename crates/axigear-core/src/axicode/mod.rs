//! AxiCode: AxiForge's share codes. Build codes are `<AxiForge:Label:z85>`,
//! comp codes are `<AxiForge:Comp:base64url(zlib(json))>`.

pub mod bits;
pub mod build;
pub mod comp;
pub mod tables;
pub mod z85;

pub use build::decode_build_code;
pub use comp::{decode_comp_code, is_comp_code};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("That isn't an AxiForge code.")]
    InvalidFormat,
    #[error("That code is corrupted.")]
    Corrupt,
    #[error("That code is cut off - copy the whole code.")]
    Truncated,
    #[error("Code made with a newer AxiForge - update axigear.")]
    NewerVersion,
}
