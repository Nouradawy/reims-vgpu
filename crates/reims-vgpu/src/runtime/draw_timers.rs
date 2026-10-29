//! Inert stand-in for the C35 lab draw timers (excluded from the 1017 icon-stack port).
//! Same API as the measured version; every call is a no-op and nothing reads the environment.
#![allow(dead_code)]

/// Bucket names kept so call sites compile unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bucket { Encode = 0, Resolve, Pipeline, Upload, Record, Submit, Fence, Settle, Commit }

#[inline(always)]
pub fn on() -> bool { false }

pub struct DrainScope;
#[inline(always)]
pub fn drain_scope() -> DrainScope { DrainScope }

pub struct Span;
#[inline(always)]
pub fn span(_bucket: Bucket) -> Option<Span> { None }

#[inline(always)]
pub fn take_fields() -> String { String::new() }