//! Database backends. Backends own connections but never accept caller-provided SQL.

pub mod postgres;
