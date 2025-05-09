mod did;
mod vc;

pub use did::Error as DidError;
pub use did::{did_key_from_pk, pk_from_did_key};
pub use vc::{VcError, Result as VcResult};
