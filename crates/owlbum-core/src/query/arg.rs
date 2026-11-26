//! A command argument that answers `bad_query` when it cannot be read.
//!
//! A command that takes a [`Query`](super::Query) directly would fail in
//! Tauri's own argument parsing on a value that is not allowed
//! (`sort: "size"`), and the page would see an `internal` error with no
//! code to branch on. [`Arg`] reads any JSON, keeps the parse failure, and
//! turns it into `bad_query` when the command asks for the value. To the
//! page's bindings it is the wrapped type itself.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use ts_rs::{Config, TS, TypeVisitor};

use crate::error::{AppError, AppResult, ErrorCode};
use crate::text;

/// A value read leniently: the value, or why it could not be read.
#[derive(Debug, Clone, PartialEq)]
pub struct Arg<T>(Result<T, String>);

impl<T> Arg<T> {
    /// Wraps a value that was read.
    pub fn new(value: T) -> Self {
        Self(Ok(value))
    }

    /// The value, or `bad_query` naming what could not be read.
    pub fn get(self) -> AppResult<T> {
        self.0.map_err(|cause| {
            AppError::new(ErrorCode::BadQuery, text::query::UNREADABLE)
                .with_details(serde_json::json!({ "cause": cause }))
        })
    }
}

impl<'de, T: DeserializeOwned> Deserialize<'de> for Arg<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(Self(
            serde_json::from_value(value).map_err(|e| e.to_string()),
        ))
    }
}

impl<T: TS> TS for Arg<T> {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn name(cfg: &Config) -> String {
        T::name(cfg)
    }

    fn inline(cfg: &Config) -> String {
        T::inline(cfg)
    }

    fn inline_flattened(cfg: &Config) -> String {
        T::inline_flattened(cfg)
    }

    fn visit_dependencies(v: &mut impl TypeVisitor)
    where
        Self: 'static,
    {
        T::visit_dependencies(v);
    }

    fn visit_generics(v: &mut impl TypeVisitor)
    where
        Self: 'static,
    {
        T::visit_generics(v);
        v.visit::<T>();
    }

    fn decl(_: &Config) -> String {
        panic!("Arg is a wrapper and cannot be declared")
    }

    fn decl_concrete(_: &Config) -> String {
        panic!("Arg is a wrapper and cannot be declared")
    }
}
