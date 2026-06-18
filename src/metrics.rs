//! Metrik aplikasi ringkas (kaunter atomik) + render format Prometheus.
//!
//! Tanpa crate Prometheus — selaras falsafah projek (dependency minimum). Kaunter ialah
//! `AtomicU64` jadi instrumentasi bebas-kunci dan murah. Endpoint `/metrics` menjana
//! teks eksposisi Prometheus yang boleh dikikis oleh Prometheus/Grafana/Alloy.

use std::sync::atomic::{AtomicU64, Ordering};

/// Kaunter & jumlah masa terkumpul untuk pemerhatian. Semua medan atomik.
#[derive(Default)]
pub struct Metrics {
    /// Jumlah permintaan chat diterima (kedua-dua /chat & /chat/stream).
    pub chat_requests: AtomicU64,
    /// Jumlah permintaan chat yang gagal (ralat sebelum/semasa menjawab).
    pub chat_errors: AtomicU64,
    /// Jumlah ingest dicetuskan (POST /ingest).
    pub ingest_runs: AtomicU64,
    /// Bilangan operasi retrieval selesai + jumlah masa (ms) — untuk purata.
    pub retrieval_count: AtomicU64,
    pub retrieval_ms_total: AtomicU64,
    /// Bilangan operasi generation selesai + jumlah masa (ms) — untuk purata.
    pub generate_count: AtomicU64,
    pub generate_ms_total: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_chat(&self) {
        self.chat_requests.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_chat_error(&self) {
        self.chat_errors.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_ingest(&self) {
        self.ingest_runs.fetch_add(1, Ordering::Relaxed);
    }

    /// Rekod satu operasi retrieval yang mengambil `ms` milisaat.
    pub fn observe_retrieval(&self, ms: u64) {
        self.retrieval_count.fetch_add(1, Ordering::Relaxed);
        self.retrieval_ms_total.fetch_add(ms, Ordering::Relaxed);
    }

    /// Rekod satu operasi generation yang mengambil `ms` milisaat.
    pub fn observe_generate(&self, ms: u64) {
        self.generate_count.fetch_add(1, Ordering::Relaxed);
        self.generate_ms_total.fetch_add(ms, Ordering::Relaxed);
    }

    /// Render metrik dalam format eksposisi teks Prometheus.
    pub fn render_prometheus(&self) -> String {
        let g = |a: &AtomicU64| a.load(Ordering::Relaxed);

        let chat = g(&self.chat_requests);
        let errs = g(&self.chat_errors);
        let ingest = g(&self.ingest_runs);
        let r_count = g(&self.retrieval_count);
        let r_total = g(&self.retrieval_ms_total);
        let gen_count = g(&self.generate_count);
        let gen_total = g(&self.generate_ms_total);

        let mut s = String::new();
        // Kaunter monotonik.
        s.push_str("# HELP tsuyu_chat_requests_total Jumlah permintaan chat diterima.\n");
        s.push_str("# TYPE tsuyu_chat_requests_total counter\n");
        s.push_str(&format!("tsuyu_chat_requests_total {chat}\n"));

        s.push_str("# HELP tsuyu_chat_errors_total Jumlah permintaan chat yang gagal.\n");
        s.push_str("# TYPE tsuyu_chat_errors_total counter\n");
        s.push_str(&format!("tsuyu_chat_errors_total {errs}\n"));

        s.push_str("# HELP tsuyu_ingest_runs_total Jumlah ingest dicetuskan.\n");
        s.push_str("# TYPE tsuyu_ingest_runs_total counter\n");
        s.push_str(&format!("tsuyu_ingest_runs_total {ingest}\n"));

        // Jumlah masa + kiraan (membenarkan pengiraan purata oleh Prometheus: total/count).
        s.push_str("# HELP tsuyu_retrieval_duration_ms_sum Jumlah masa retrieval (ms).\n");
        s.push_str("# TYPE tsuyu_retrieval_duration_ms_sum counter\n");
        s.push_str(&format!("tsuyu_retrieval_duration_ms_sum {r_total}\n"));
        s.push_str("# HELP tsuyu_retrieval_duration_ms_count Bilangan operasi retrieval.\n");
        s.push_str("# TYPE tsuyu_retrieval_duration_ms_count counter\n");
        s.push_str(&format!("tsuyu_retrieval_duration_ms_count {r_count}\n"));

        s.push_str("# HELP tsuyu_generate_duration_ms_sum Jumlah masa generation (ms).\n");
        s.push_str("# TYPE tsuyu_generate_duration_ms_sum counter\n");
        s.push_str(&format!("tsuyu_generate_duration_ms_sum {gen_total}\n"));
        s.push_str("# HELP tsuyu_generate_duration_ms_count Bilangan operasi generation.\n");
        s.push_str("# TYPE tsuyu_generate_duration_ms_count counter\n");
        s.push_str(&format!("tsuyu_generate_duration_ms_count {gen_count}\n"));

        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kaunter_bertambah() {
        let m = Metrics::new();
        m.inc_chat();
        m.inc_chat();
        m.inc_chat_error();
        m.observe_retrieval(50);
        m.observe_retrieval(30);
        let out = m.render_prometheus();
        assert!(out.contains("tsuyu_chat_requests_total 2"));
        assert!(out.contains("tsuyu_chat_errors_total 1"));
        assert!(out.contains("tsuyu_retrieval_duration_ms_sum 80"));
        assert!(out.contains("tsuyu_retrieval_duration_ms_count 2"));
    }

    #[test]
    fn render_ada_jenis_prometheus() {
        let m = Metrics::new();
        let out = m.render_prometheus();
        assert!(out.contains("# TYPE tsuyu_chat_requests_total counter"));
    }
}
