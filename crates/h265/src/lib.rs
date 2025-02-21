// TODO: #![deny(missing_docs)]
#![deny(unsafe_code)]

mod config;
mod sps;

pub use self::config::{HEVCDecoderConfigurationRecord, NaluArray, NaluType};
pub use self::sps::{ColorConfig, Sps};

#[cfg(test)]
mod tests;
