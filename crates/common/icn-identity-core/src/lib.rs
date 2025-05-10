pub mod did;
pub mod vc;

pub use did::Error as DidError;
pub use did::{did_key_from_pk, pk_from_did_key};
pub use icn_types::error::VcError;
pub use vc::Result as VcResult;
