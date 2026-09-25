//! Everything that talks to the USBGuard D-Bus bridge.
//!
//! | Module        | Role                                                      |
//! |---------------|-----------------------------------------------------------|
//! | `proxies`     | Raw interface declarations — the only place wire types appear (§8) |
//! | `client`      | Typed wrapper: domain types in, domain types out          |
//! | `errors`      | D-Bus errors → `AppError` / `AccessState` (§4.3, §6.3)    |
//! | `coalesce`    | Burst merging, as pure logic (§5.4)                       |
//! | `worker`      | The long-lived signal task (§5.4)                         |
//! | `supervisor`  | Restart and reconnection with backoff (§5.5)              |
//! | `commands`    | Cancellable user operations (§5.7)                        |
//! | `diagnostics` | The access probe sequence (§4.2)                          |
//!
//! Depends on `model` and `rules`, never on the interface.

pub mod bus;
pub mod client;
pub mod coalesce;
pub mod commands;
pub mod diagnostics;
pub mod errors;
pub mod proxies;
pub mod supervisor;
pub mod worker;

pub use client::Client;
