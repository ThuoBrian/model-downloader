#[derive(Debug, Clone)]
pub struct ModelEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    /// Hugging Face GGUF repo (e.g. "bartowski/DeepSeek-R1-Distill-Qwen-14B-GGUF")
    pub hf_repo: Option<&'static str>,
    /// Ollama registry tag (e.g. "deepseek-r1:14b")
    pub ollama_tag: Option<&'static str>,
    /// Approximate size in GB for Q4_K_M quant
    pub size_gb_q4: f64,
    /// Parameter count in billions
    pub params_b: f64,
    pub reasoning_strength: ReasoningStrength,
}

#[derive(Debug, Clone, Copy, PartialEq, clap::ValueEnum)]
pub enum ReasoningStrength {
    /// Dedicated reasoning model trained with RL / CoT
    Dedicated,
    /// Strong general model with good reasoning ability
    Strong,
    /// Capable but not primarily a reasoning model
    Moderate,
}

impl ReasoningStrength {
    pub fn label(&self) -> &'static str {
        match self {
            ReasoningStrength::Dedicated => "Dedicated reasoning",
            ReasoningStrength::Strong => "Strong reasoning",
            ReasoningStrength::Moderate => "Moderate reasoning",
        }
    }
}

/// Curated list of open-weights models known for strong reasoning, all available in GGUF.
pub fn catalog() -> &'static [ModelEntry] {
    &CATALOG
}

static CATALOG: [ModelEntry; 13] = [
    ModelEntry {
        id: "dsr1-distill-qwen-7b",
        display_name: "DeepSeek-R1-Distill-Qwen-7B",
        description: "Distilled from DeepSeek-R1 into Qwen2.5-7B. Excellent reasoning-to-size ratio. Runs on most hardware.",
        hf_repo: Some("bartowski/DeepSeek-R1-Distill-Qwen-7B-GGUF"),
        ollama_tag: Some("deepseek-r1:7b"),
        size_gb_q4: 4.7,
        params_b: 7.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "dsr1-distill-qwen-14b",
        display_name: "DeepSeek-R1-Distill-Qwen-14B",
        description: "Distilled from DeepSeek-R1 into Qwen2.5-14B. Strong reasoning quality, needs ~10 GB RAM.",
        hf_repo: Some("bartowski/DeepSeek-R1-Distill-Qwen-14B-GGUF"),
        ollama_tag: Some("deepseek-r1:14b"),
        size_gb_q4: 9.4,
        params_b: 14.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "dsr1-distill-qwen-32b",
        display_name: "DeepSeek-R1-Distill-Qwen-32B",
        description: "Distilled from DeepSeek-R1 into Qwen2.5-32B. Near frontier-level reasoning for open weights.",
        hf_repo: Some("bartowski/DeepSeek-R1-Distill-Qwen-32B-GGUF"),
        ollama_tag: Some("deepseek-r1:32b"),
        size_gb_q4: 20.0,
        params_b: 32.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "dsr1-distill-llama-8b",
        display_name: "DeepSeek-R1-Distill-Llama-8B",
        description: "Distilled from DeepSeek-R1 into Llama-3.1-8B. Good reasoning in a small, fast package.",
        hf_repo: Some("bartowski/DeepSeek-R1-Distill-Llama-8B-GGUF"),
        ollama_tag: Some("deepseek-r1:8b"),
        size_gb_q4: 5.2,
        params_b: 8.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "dsr1-distill-llama-70b",
        display_name: "DeepSeek-R1-Distill-Llama-70B",
        description: "Distilled from DeepSeek-R1 into Llama-3.3-70B. Top-tier reasoning, needs ~40 GB RAM.",
        hf_repo: Some("bartowski/DeepSeek-R1-Distill-Llama-70B-GGUF"),
        ollama_tag: Some("deepseek-r1:70b"),
        size_gb_q4: 42.0,
        params_b: 70.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "qwen3-8b",
        display_name: "Qwen3-8B",
        description: "Qwen3 8B with thinking mode. Strong multilingual reasoning and tool use.",
        hf_repo: Some("bartowski/Qwen_Qwen3-8B-GGUF"),
        ollama_tag: Some("qwen3:8b"),
        size_gb_q4: 5.2,
        params_b: 8.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "qwen3-14b",
        display_name: "Qwen3-14B",
        description: "Qwen3 14B with thinking mode. Excellent balance of reasoning quality and resource use.",
        hf_repo: Some("bartowski/Qwen_Qwen3-14B-GGUF"),
        ollama_tag: Some("qwen3:14b"),
        size_gb_q4: 9.4,
        params_b: 14.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "qwen3-32b",
        display_name: "Qwen3-32B",
        description: "Qwen3 32B with thinking mode. Frontier-class reasoning among open-weights dense models.",
        hf_repo: Some("bartowski/Qwen_Qwen3-32B-GGUF"),
        ollama_tag: Some("qwen3:32b"),
        size_gb_q4: 20.0,
        params_b: 32.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "qwen3-30b-a3b",
        display_name: "Qwen3-30B-A3B (MoE)",
        description: "Mixture-of-experts: 30B total, 3B active. Fast inference with strong reasoning for its footprint.",
        hf_repo: Some("bartowski/Qwen_Qwen3-30B-A3B-GGUF"),
        ollama_tag: Some("qwen3:30b-a3b"),
        size_gb_q4: 18.0,
        params_b: 30.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "llama-3.3-70b",
        display_name: "Llama-3.3-70B-Instruct",
        description: "Meta's 70B instruct model. Strong general reasoning, not CoT-trained but very capable.",
        hf_repo: Some("bartowski/Llama-3.3-70B-Instruct-GGUF"),
        ollama_tag: Some("llama3.3:70b"),
        size_gb_q4: 42.0,
        params_b: 70.0,
        reasoning_strength: ReasoningStrength::Strong,
    },
    ModelEntry {
        id: "phi-4-reasoning",
        display_name: "Phi-4-Reasoning",
        description: "Microsoft's 14B reasoning model. Trained on synthetic CoT data, strong math and logic.",
        hf_repo: Some("bartowski/microsoft_Phi-4-reasoning-GGUF"),
        ollama_tag: Some("phi4-reasoning:14b"),
        size_gb_q4: 9.0,
        params_b: 14.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "gpt-oss-20b",
        display_name: "GPT-OSS-20B",
        description: "OpenAI's open-weights 20B MoE model (3.6B active). Strong reasoning, designed for local deployment.",
        hf_repo: Some("bartowski/openai_gpt-oss-20b-GGUF"),
        ollama_tag: Some("gpt-oss:20b"),
        size_gb_q4: 12.0,
        params_b: 20.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "gpt-oss-120b",
        display_name: "GPT-OSS-120B",
        description: "OpenAI's open-weights 120B MoE model. Frontier-class reasoning, needs high-end hardware.",
        hf_repo: Some("bartowski/openai_gpt-oss-120b-GGUF"),
        ollama_tag: Some("gpt-oss:120b"),
        size_gb_q4: 67.0,
        params_b: 120.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
];

/// Shown before anyone browses or downloads an abliterated model.
pub const ABLITERATED_NOTICE: &str = "Abliterated models have their safety refusals removed, so they can produce \
     harmful or false content without pushing back. They are third-party modifications of the original \
     model and may reason slightly worse. You are responsible for how you use them.";

/// Uncensored variants, kept out of `catalog()` so they only appear when asked for.
pub fn abliterated() -> &'static [ModelEntry] {
    &ABLITERATED
}

static ABLITERATED: [ModelEntry; 3] = [
    ModelEntry {
        id: "qwen3-8b-abliterated",
        display_name: "Qwen3-8B (abliterated)",
        description: "mlabonne's abliterated Qwen3-8B: Qwen3 8B with thinking mode and safety refusals removed.",
        hf_repo: Some("bartowski/mlabonne_Qwen3-8B-abliterated-GGUF"),
        ollama_tag: None,
        size_gb_q4: 5.0,
        params_b: 8.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "qwen3-14b-abliterated",
        display_name: "Qwen3-14B (abliterated)",
        description: "huihui-ai's abliterated Qwen3-14B: Qwen3 14B with thinking mode and safety refusals removed.",
        hf_repo: Some("bartowski/huihui-ai_Qwen3-14B-abliterated-GGUF"),
        ollama_tag: None,
        size_gb_q4: 9.0,
        params_b: 14.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
    ModelEntry {
        id: "gpt-oss-20b-abliterated",
        display_name: "GPT-OSS-20B (abliterated)",
        description: "huihui-ai's abliterated GPT-OSS-20B: OpenAI's 20B MoE model with safety refusals removed.",
        hf_repo: Some("bartowski/huihui-ai_Huihui-gpt-oss-20b-BF16-abliterated-GGUF"),
        ollama_tag: None,
        size_gb_q4: 11.7,
        params_b: 20.0,
        reasoning_strength: ReasoningStrength::Dedicated,
    },
];

/// Curated and live abliterated models alike have "abliterated" in their ID.
pub fn is_abliterated(model: &ModelEntry) -> bool {
    model.id.contains("abliterated")
}

/// A catalog entry for a repo found by searching Hugging Face at runtime.
/// `ModelEntry` holds `&'static str`s, so the strings are leaked; that's a
/// few hundred bytes per search, freed when the program exits.
pub fn live_entry(repo: &str, q4_bytes: u64) -> &'static ModelEntry {
    let name = repo.rsplit('/').next().unwrap_or(repo);
    let name = name.strip_suffix("-GGUF").unwrap_or(name);
    let leak = |s: String| -> &'static str { Box::leak(s.into_boxed_str()) };
    Box::leak(Box::new(ModelEntry {
        id: leak(name.to_lowercase()),
        display_name: leak(name.to_string()),
        description: "Found on Hugging Face; not checked by model-downloader.",
        hf_repo: Some(leak(repo.to_string())),
        ollama_tag: None,
        size_gb_q4: q4_bytes as f64 / 1e9,
        params_b: params_from_name(name).unwrap_or(0.0),
        reasoning_strength: ReasoningStrength::Moderate,
    }))
}

/// "mlabonne_Qwen3-8B-abliterated" → 8.0. Tokens like "A3B" (active
/// parameters) or "BF16" don't parse, so the first real size wins.
fn params_from_name(name: &str) -> Option<f64> {
    name.split(['-', '_'])
        .find_map(|t| t.strip_suffix(['B', 'b'])?.parse::<f64>().ok())
}

fn all_models() -> impl Iterator<Item = &'static ModelEntry> {
    catalog().iter().chain(abliterated())
}

pub fn find_by_id(id: &str) -> Option<&'static ModelEntry> {
    all_models().find(|m| m.id.eq_ignore_ascii_case(id))
}

/// Up to three catalog IDs closest to a mistyped one, best match first.
pub fn suggest(query: &str) -> Vec<&'static str> {
    let q = query.to_lowercase();
    let mut scored: Vec<(f64, &'static str)> = all_models()
        .map(|m| {
            let name = m.display_name.to_lowercase();
            let similarity = strsim::jaro_winkler(&q, m.id).max(strsim::jaro_winkler(&q, &name));
            // A substring hit ("70b", "gpt-oss") always qualifies, but a
            // near-identical ID still ranks above it.
            let score = if m.id.contains(&q) || name.contains(&q) {
                0.9 + 0.1 * similarity
            } else {
                similarity
            };
            (score, m.id)
        })
        .filter(|(score, _)| *score >= 0.85)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    scored.into_iter().take(3).map(|(_, id)| id).collect()
}

/// Like `find_by_id`, but the error suggests close matches.
pub fn lookup(id: &str) -> anyhow::Result<&'static ModelEntry> {
    find_by_id(id).ok_or_else(|| {
        let mut msg = format!("Model '{id}' not found.");
        let close = suggest(id);
        if !close.is_empty() {
            msg += &format!("\nDid you mean: {}?", close.join(", "));
        }
        msg += "\nRun 'model-downloader list' to see every model.";
        anyhow::anyhow!(msg)
    })
}

pub fn available_quants() -> &'static [&'static str] {
    &["Q3_K_M", "Q4_K_M", "Q5_K_M", "Q6_K", "Q8_0"]
}

/// One-line guidance shown next to each quant in the interactive menu.
pub fn quant_note(quant: &str) -> &'static str {
    match quant {
        "Q3_K_M" => "smallest, noticeably lower quality",
        "Q4_K_M" => "recommended balance of size and quality",
        "Q5_K_M" => "better quality, about 15% larger",
        "Q6_K" => "near-original quality, about 35% larger",
        "Q8_0" => "best quality, about 70% larger",
        _ => "",
    }
}

/// Approximate file size of a quant relative to Q4_K_M (matches the notes above).
pub fn quant_size_factor(quant: &str) -> f64 {
    match quant {
        "Q3_K_M" => 0.82,
        "Q5_K_M" => 1.15,
        "Q6_K" => 1.35,
        "Q8_0" => 1.7,
        _ => 1.0,
    }
}

impl ModelEntry {
    /// Estimated file size in bytes for a quant, before asking the server.
    pub fn estimated_bytes(&self, quant: &str) -> u64 {
        (self.size_gb_q4 * quant_size_factor(quant) * 1e9) as u64
    }
}

/// Catalog models from smallest to largest, the order the menus show them in.
pub fn by_size() -> Vec<&'static ModelEntry> {
    let mut models: Vec<_> = catalog().iter().collect();
    models.sort_by(|a, b| a.size_gb_q4.total_cmp(&b.size_gb_q4));
    models
}

/// Default quantization — best balance of size and quality
pub const DEFAULT_QUANT: &str = "Q4_K_M";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_models() {
        assert!(!catalog().is_empty(), "catalog should not be empty");
    }

    #[test]
    fn every_model_has_at_least_one_source() {
        for m in all_models() {
            assert!(
                m.hf_repo.is_some() || m.ollama_tag.is_some(),
                "Model {} has no download source",
                m.id
            );
        }
    }

    #[test]
    fn find_by_id_works() {
        assert!(find_by_id("dsr1-distill-qwen-14b").is_some());
        assert!(find_by_id("nonexistent").is_none());
        assert!(find_by_id("Qwen3-14B").is_some(), "lookup ignores case");
    }

    #[test]
    fn suggests_close_ids() {
        assert_eq!(suggest("qwen-14b").first(), Some(&"qwen3-14b"));
        assert_eq!(suggest("gpt-oss").len(), 3, "both sizes plus the abliterated one");
        assert!(suggest("zzz").is_empty());
    }

    #[test]
    fn lookup_error_mentions_suggestion() {
        let err = lookup("qwen-14b").unwrap_err().to_string();
        assert!(err.contains("Did you mean: qwen3-14b"), "{err}");
    }

    #[test]
    fn every_quant_has_a_note() {
        for q in available_quants() {
            assert!(!quant_note(q).is_empty(), "{q} has no note");
        }
    }

    #[test]
    fn default_quant_is_in_available_quants() {
        assert!(available_quants().contains(&DEFAULT_QUANT));
    }

    #[test]
    fn all_ids_are_unique() {
        let ids: Vec<_> = all_models().map(|m| m.id).collect();
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique.len(), "duplicate model IDs in catalog");
    }

    #[test]
    fn abliterated_models_stay_out_of_the_normal_lists() {
        assert!(catalog().iter().chain(by_size()).all(|m| !is_abliterated(m)));
        assert!(abliterated().iter().all(is_abliterated));
    }

    #[test]
    fn abliterated_models_can_be_found_by_id() {
        let m = find_by_id("qwen3-8b-abliterated").expect("found");
        assert!(is_abliterated(m));
        assert!(!is_abliterated(find_by_id("qwen3-8b").unwrap()));
    }

    #[test]
    fn reads_parameter_count_from_repo_names() {
        assert_eq!(params_from_name("mlabonne_Qwen3-8B-abliterated"), Some(8.0));
        assert_eq!(params_from_name("Qwen2.5-Coder-1.5B-Instruct-abliterated"), Some(1.5));
        assert_eq!(params_from_name("Qwen_Qwen3-30B-A3B"), Some(30.0));
        assert_eq!(params_from_name("huihui-ai_Huihui-gpt-oss-20b-BF16-abliterated"), Some(20.0));
        assert_eq!(params_from_name("huihui-ai_Qwen3-Coder-Next-abliterated"), None);
    }

    #[test]
    fn live_entries_are_marked_abliterated() {
        let m = live_entry("bartowski/mlabonne_gemma-3-4b-it-abliterated-GGUF", 2_500_000_000);
        assert_eq!(m.id, "mlabonne_gemma-3-4b-it-abliterated");
        assert_eq!(m.params_b, 4.0);
        assert!(is_abliterated(m));
    }
}