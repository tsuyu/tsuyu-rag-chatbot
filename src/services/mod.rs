//! Lapisan logik perniagaan (business logic) RAG.
//!
//! - `chunk`    : pecah teks panjang jadi chunk bertindih.
//! - `embed`    : jana embedding melalui Ollama.
//! - `ingest`   : baca dokumen -> chunk -> embed -> simpan.
//! - `retrieve` : cari chunk paling relevan dari pgvector.
//! - `generate` : bina prompt + panggil Ollama untuk jawapan.
//! - `character`: persona pembantu (kad watak) yang ditala admin.

pub mod character;
pub mod chunk;
pub mod documents;
pub mod embed;
pub mod generate;
pub mod ingest;
pub mod memory;
pub mod metadata;
pub mod rerank;
pub mod retrieve;
pub mod retry;
