//! badgery — shields-compatible SVG badge generator for airgapped CI.
//!
//! Everything is pure `std`: a small strict JSON parser ([`json`]), a
//! JSONPath subset ([`jsonpath`]), shields color tables ([`color`]),
//! deterministic text measurement ([`text`]), the badge model and shields
//! path syntax ([`badge`]), four SVG styles ([`render`]), the shields
//! endpoint schema ([`endpoint`]), a batch manifest ([`manifest`]) and a
//! loopback HTTP server ([`server`]). The CLI in [`cli`] wires them up.

pub mod badge;
pub mod cli;
pub mod color;
pub mod endpoint;
pub mod json;
pub mod jsonpath;
pub mod manifest;
pub mod render;
pub mod server;
pub mod text;
