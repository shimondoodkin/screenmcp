//! Provider-tuned default screenshot size, computed from the real screen dimensions.
//! See screenmcp/docs/model-sizing.md. Shared verbatim across windows/mac/linux.

/// Returns (max_width, max_height) for the given model, or None for an unknown model.
pub fn provider_default_size(model: &str, w: u32, h: u32) -> Option<(u32, u32)> {
    let (wf, hf) = (w as f64, h as f64);
    match model {
        "claude" => {
            let max_pixels: f64 = 1_176_000.0;
            let max_edge: f64 = 1568.0;
            let s = 1.0_f64
                .min(max_edge / wf.max(hf))
                .min((max_pixels / (wf * hf)).sqrt());
            let mut mw = (wf * s).floor() as u32;
            let mut mh = (hf * s).floor() as u32;
            while (mw as u64) * (mh as u64) > max_pixels as u64 && (mw > 1 || mh > 1) {
                if mw >= mh { mw -= 1; } else { mh -= 1; }
            }
            Some((mw, mh))
        }
        "gemini" => {
            let (short_cap, long_cap) = (1080.0_f64, 1920.0_f64);
            let s = if w >= h {
                1.0_f64.min(long_cap / wf).min(short_cap / hf)
            } else {
                1.0_f64.min(long_cap / hf).min(short_cap / wf)
            };
            Some(((wf * s).round() as u32, (hf * s).round() as u32))
        }
        "chatgpt" => {
            let short = wf.min(hf);
            let mut s = 1.0_f64.min(768.0 / short);
            let long = wf.max(hf);
            if long * s > 2048.0 {
                s = 2048.0 / long;
            }
            Some((round16(wf * s), round16(hf * s)))
        }
        _ => None,
    }
}

fn round16(x: f64) -> u32 {
    (((x / 16.0).round() as i64) * 16).max(16) as u32
}

#[cfg(test)]
mod tests {
    use super::provider_default_size;

    // Canonical vectors — identical across all clients.
    const VECTORS: &[(u32, u32, (u32, u32), (u32, u32), (u32, u32))] = &[
        // (w, h, claude, gemini, chatgpt)
        (2560, 1440, (1445, 813), (1920, 1080), (1360, 768)),
        (1920, 1080, (1445, 813), (1920, 1080), (1360, 768)),
        (3840, 2160, (1445, 813), (1920, 1080), (1360, 768)),
        (1080, 2400, (705, 1568), (864, 1920), (768, 1712)),
        (1440, 3120, (723, 1568), (886, 1920), (768, 1664)),
        (1080, 3000, (564, 1567), (691, 1920), (736, 2048)),
        (1000, 1000, (1000, 1000), (1000, 1000), (768, 768)),
        (640, 480, (640, 480), (640, 480), (640, 480)),
    ];

    #[test]
    fn matches_canonical_table() {
        for &(w, h, c, g, o) in VECTORS {
            assert_eq!(provider_default_size("claude", w, h), Some(c), "claude {w}x{h}");
            assert_eq!(provider_default_size("gemini", w, h), Some(g), "gemini {w}x{h}");
            assert_eq!(provider_default_size("chatgpt", w, h), Some(o), "chatgpt {w}x{h}");
        }
    }

    #[test]
    fn claude_never_exceeds_caps() {
        for &(w, h, _, _, _) in VECTORS {
            let (mw, mh) = provider_default_size("claude", w, h).unwrap();
            assert!((mw as u64) * (mh as u64) <= 1_176_000, "{w}x{h} -> {mw}x{mh}");
            assert!(mw.max(mh) <= 1568, "{w}x{h} long edge");
        }
    }

    #[test]
    fn chatgpt_outputs_multiples_of_16() {
        for &(w, h, _, _, _) in VECTORS {
            let (mw, mh) = provider_default_size("chatgpt", w, h).unwrap();
            assert_eq!(mw % 16, 0, "{w}x{h} width not /16");
            assert_eq!(mh % 16, 0, "{w}x{h} height not /16");
        }
    }

    #[test]
    fn unknown_model_returns_none() {
        assert_eq!(provider_default_size("gpt-5", 1920, 1080), None);
    }
}
