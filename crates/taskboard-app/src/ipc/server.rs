// SPDX-License-Identifier: AGPL-3.0-only
//! The daemon-side IPC server (ADR 0012): bind the per-database socket,
//! accept connections concurrently, execute requests serially (engine
//! fairness), drain on shutdown. Landed in the next steps of the WI-5
//! plan.
