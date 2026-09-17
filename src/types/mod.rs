//! Machine-neutral type model.
//!
//! Both JVM class files and DEX describe types with the same grammar
//! (`Lcom/foo/Bar;`, `[I`, `(ILjava/lang/String;)V`), so one model serves
//! every Java-family front-end. Generic signatures ([`signature`]) mirror the
//! JVM `Signature` attribute — a front-end whose container does not record
//! them (e.g. DEX) simply leaves that information absent, and the emitter
//! falls back to erased types.

mod access;
mod descriptor;
mod signature;

pub use access::*;
pub use descriptor::*;
pub use signature::*;
