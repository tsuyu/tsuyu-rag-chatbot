//! Kad Watak (Character Card) — persona pembantu yang boleh ditala oleh admin.
//!
//! Disimpan sebagai fail JSON (laluan: `CHARACTER_CARD_PATH`). Admin boleh edit
//! melalui UI `/admin` atau terus pada fail. Nilai disuntik ke dalam *system prompt*
//! (lihat [`crate::services::generate::build_prompt`]). Jika fail tiada/rosak, lalai
//! munasabah digunakan supaya sistem tetap berfungsi.

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Persona pembantu. `#[serde(default)]` membenarkan JSON separa — medan yang hilang
/// guna lalai, jadi admin boleh tetapkan hanya apa yang mereka mahu ubah.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterCard {
    /// Nama pembantu (cth. "Ayu").
    pub name: String,
    /// Peranan ringkas (cth. "Pembantu pegawai TSUYU").
    pub role: String,
    /// Nada (cth. "Formal tetapi mesra").
    pub tone: String,
    /// Bahasa jawapan (cth. "Bahasa Malaysia").
    pub language: String,
    /// Panjang jawapan: "short" | "medium" | "long" (atau teks bebas).
    pub verbosity: String,
    /// Benarkan emoji dalam jawapan.
    pub emoji: bool,
    /// Peraturan tambahan khusus (setiap satu jadi satu arahan).
    pub special_rules: Vec<String>,
}

impl Default for CharacterCard {
    fn default() -> Self {
        Self {
            name: "Pembantu TSUYU".to_string(),
            role: "pembantu AI dalaman untuk TSUYU".to_string(),
            tone: "Formal tetapi mesra".to_string(),
            language: "Bahasa Malaysia".to_string(),
            verbosity: "medium".to_string(),
            emoji: false,
            special_rules: vec![
                "Sentiasa gunakan istilah rasmi kerajaan".to_string(),
                "Berikan rujukan dokumen jika ada".to_string(),
            ],
        }
    }
}

impl CharacterCard {
    /// Terjemah verbosity kepada arahan panjang jawapan.
    fn verbosity_instruction(&self) -> &str {
        match self.verbosity.trim().to_lowercase().as_str() {
            "short" | "ringkas" => "Jawab ringkas dan padat (1-3 ayat).",
            "long" | "panjang" => "Jawab dengan terperinci dan menyeluruh.",
            // "medium" atau lain-lain
            _ => "Jawab sederhana — cukup lengkap tanpa berjela.",
        }
    }

    /// Bina bahagian persona untuk *system prompt*. Ini ialah input ADMIN yang
    /// dipercayai, jadi tidak perlu disanitasi seperti kandungan dokumen.
    ///
    /// Nota: peraturan keras (jawab dari konteks sahaja, anti-injection) dirantai
    /// SELEPAS blok ini dalam `build_prompt`, jadi persona tidak boleh mengatasinya.
    pub fn to_prompt_section(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("Anda ialah {}, {}.\n", self.name.trim(), self.role.trim()));
        s.push_str(&format!("Jawab dalam {}.\n", self.language.trim()));
        s.push_str(&format!("Nada: {}.\n", self.tone.trim()));
        s.push_str(self.verbosity_instruction());
        s.push('\n');
        if self.emoji {
            s.push_str("Anda boleh gunakan emoji yang sesuai dan berpada.\n");
        } else {
            s.push_str("Jangan gunakan emoji.\n");
        }
        let rules: Vec<&String> = self
            .special_rules
            .iter()
            .filter(|r| !r.trim().is_empty())
            .collect();
        if !rules.is_empty() {
            s.push_str("Peraturan khas:\n");
            for r in rules {
                s.push_str(&format!("- {}\n", r.trim()));
            }
        }
        s
    }

    /// Muat kad watak dari fail JSON. Jika fail tiada atau rosak, pulang lalai
    /// (dengan amaran log) supaya sistem tetap berfungsi.
    pub fn load(path: &str) -> Self {
        match std::fs::read_to_string(path) {
            Ok(teks) => match serde_json::from_str::<CharacterCard>(&teks) {
                Ok(card) => {
                    tracing::info!("kad watak dimuat dari {path}");
                    card
                }
                Err(e) => {
                    tracing::warn!("kad watak {path} rosak ({e}); guna lalai");
                    Self::default()
                }
            },
            Err(_) => {
                tracing::info!("kad watak {path} tiada; guna lalai");
                Self::default()
            }
        }
    }

    /// Simpan kad watak ke fail JSON (format kemas). Digunakan oleh endpoint admin.
    pub fn save(&self, path: &str) -> Result<(), AppError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("gagal serialisasi kad watak: {e}")))?;
        std::fs::write(path, json)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("gagal tulis kad watak {path}: {e}")))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lalai_munasabah() {
        let c = CharacterCard::default();
        assert_eq!(c.language, "Bahasa Malaysia");
        assert!(!c.emoji);
        assert!(!c.special_rules.is_empty());
    }

    #[test]
    fn json_separa_guna_lalai() {
        // Hanya tetapkan name; selebihnya patut jatuh ke lalai.
        let c: CharacterCard = serde_json::from_str(r#"{"name":"Ayu"}"#).unwrap();
        assert_eq!(c.name, "Ayu");
        assert_eq!(c.language, "Bahasa Malaysia"); // lalai
        assert_eq!(c.verbosity, "medium"); // lalai
    }

    #[test]
    fn prompt_section_ada_persona() {
        let c = CharacterCard {
            name: "Ayu".to_string(),
            role: "Pembantu Ana".to_string(),
            tone: "Formal tetapi mesra".to_string(),
            language: "Bahasa Malaysia".to_string(),
            verbosity: "short".to_string(),
            emoji: false,
            special_rules: vec!["Gunakan istilah rasmi".to_string()],
        };
        let s = c.to_prompt_section();
        assert!(s.contains("Anda ialah Ayu, Pembantu Ana."));
        assert!(s.contains("Bahasa Malaysia"));
        assert!(s.contains("ringkas")); // verbosity short
        assert!(s.contains("Jangan gunakan emoji"));
        assert!(s.contains("- Gunakan istilah rasmi"));
    }

    #[test]
    fn emoji_dihidupkan() {
        let c = CharacterCard { emoji: true, ..Default::default() };
        assert!(c.to_prompt_section().contains("boleh gunakan emoji"));
    }

    #[test]
    fn peraturan_kosong_dilangkau() {
        let c = CharacterCard { special_rules: vec!["".to_string(), "  ".to_string()], ..Default::default() };
        assert!(!c.to_prompt_section().contains("Peraturan khas:"));
    }
}
