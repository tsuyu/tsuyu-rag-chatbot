//! Had kadar ringkas (fixed-window) per-IP, tanpa dependency luaran.
//!
//! Setiap IP ada kiraan dalam tetingkap satu minit. Apabila tetingkap berlalu,
//! kiraan ditetapkan semula. Sesuai untuk perlindungan asas dalaman TSUYU; untuk
//! trafik besar/teragih, pertimbang penyelesaian khusus (cth. di reverse proxy).

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, State};
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::error::AppError;
use crate::state::AppState;

const WINDOW: Duration = Duration::from_secs(60);

/// Keadaan kiraan bagi satu IP dalam tetingkap semasa.
struct Bucket {
    count: u32,
    window_start: Instant,
}

/// Penyimpan kiraan per-IP. Dikongsi melalui `AppState` (dibungkus `Arc`).
#[derive(Default)]
pub struct RateLimiter {
    buckets: Mutex<HashMap<std::net::IpAddr, Bucket>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Daftar satu permintaan dari `ip`. Pulang `true` jika dibenarkan (di bawah had).
    ///
    /// `limit` ialah bilangan permintaan maksimum setiap tetingkap minit.
    fn check(&self, ip: std::net::IpAddr, limit: u32) -> bool {
        let now = Instant::now();
        // Jika mutex teracun, gagal-selamat dengan membenarkan permintaan (jangan panik).
        let mut map = match self.buckets.lock() {
            Ok(m) => m,
            Err(_) => return true,
        };

        let bucket = map.entry(ip).or_insert(Bucket {
            count: 0,
            window_start: now,
        });

        if now.duration_since(bucket.window_start) >= WINDOW {
            bucket.count = 0;
            bucket.window_start = now;
        }

        bucket.count += 1;
        bucket.count <= limit
    }
}

/// Middleware: hadkan kadar permintaan per-IP. Tidak aktif jika `RATE_LIMIT_RPM` = 0.
pub async fn limit(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let limit = state.config.rate_limit_rpm;
    if limit == 0 {
        return Ok(next.run(req).await);
    }

    if state.rate_limiter.check(addr.ip(), limit) {
        Ok(next.run(req).await)
    } else {
        Err(AppError::TooManyRequests)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benarkan_sehingga_had_kemudian_sekat() {
        let rl = RateLimiter::new();
        let ip = "127.0.0.1".parse().unwrap();
        assert!(rl.check(ip, 3));
        assert!(rl.check(ip, 3));
        assert!(rl.check(ip, 3));
        // Permintaan ke-4 melebihi had.
        assert!(!rl.check(ip, 3));
    }

    #[test]
    fn ip_berbeza_dikira_berasingan() {
        let rl = RateLimiter::new();
        let a = "10.0.0.1".parse().unwrap();
        let b = "10.0.0.2".parse().unwrap();
        assert!(rl.check(a, 1));
        assert!(!rl.check(a, 1)); // a sudah habis
        assert!(rl.check(b, 1)); // b masih ada kuota
    }
}
