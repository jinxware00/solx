mod collection_ops;
pub mod environment;
pub mod error;
mod gc;
pub(crate) mod numeric;
pub mod value;

pub use environment::Environment;
pub use error::RuntimeError;
pub use gc::{CollectionStats, collect_garbage, track_value};
pub use value::Value;

pub(crate) use collection_ops::{read_collection, write_collection};
pub(crate) use gc::maybe_collect;
