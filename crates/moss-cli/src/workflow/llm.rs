//! LLM strategy for workflow execution.
//!
//! This module is only compiled when the "llm" feature is enabled.
//! Supports all providers from rig: anthropic, openai, google, cohere, groq, etc.

#[cfg(feature = "llm")]
use rig::{
    client::{CompletionClient, ProviderClient},
    completion::Prompt,
    providers,
};

/// LLM strategy trait for workflow execution.
pub trait LlmStrategy: Send + Sync {
    /// Generate a completion from a prompt.
    fn complete(&self, prompt: &str) -> Result<String, String>;

    /// Generate with system prompt.
    fn complete_with_system(&self, system: &str, prompt: &str) -> Result<String, String>;
}

/// No LLM - for workflows that don't need it.
pub struct NoLlm;

impl LlmStrategy for NoLlm {
    fn complete(&self, _prompt: &str) -> Result<String, String> {
        Err("LLM not configured for this workflow".to_string())
    }

    fn complete_with_system(&self, _system: &str, _prompt: &str) -> Result<String, String> {
        Err("LLM not configured for this workflow".to_string())
    }
}

/// Supported LLM providers.
#[cfg(feature = "llm")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    OpenAI,
    Azure,
    Gemini,
    Cohere,
    DeepSeek,
    Groq,
    Mistral,
    Ollama,
    OpenRouter,
    Perplexity,
    Together,
    XAI,
}

#[cfg(feature = "llm")]
impl Provider {
    /// Parse provider from string.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "anthropic" | "claude" => Some(Self::Anthropic),
            "openai" | "gpt" | "chatgpt" => Some(Self::OpenAI),
            "azure" | "azure-openai" => Some(Self::Azure),
            "google" | "gemini" => Some(Self::Gemini),
            "cohere" => Some(Self::Cohere),
            "deepseek" => Some(Self::DeepSeek),
            "groq" => Some(Self::Groq),
            "mistral" => Some(Self::Mistral),
            "ollama" => Some(Self::Ollama),
            "openrouter" => Some(Self::OpenRouter),
            "perplexity" | "pplx" => Some(Self::Perplexity),
            "together" | "together-ai" => Some(Self::Together),
            "xai" | "grok" => Some(Self::XAI),
            _ => None,
        }
    }

    /// Get default model for this provider.
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::Anthropic => "claude-sonnet-4-20250514",
            Self::OpenAI => "gpt-4o",
            Self::Azure => "gpt-4o",
            Self::Gemini => "gemini-2.0-flash",
            Self::Cohere => "command-r-plus",
            Self::DeepSeek => "deepseek-chat",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::Mistral => "mistral-large-latest",
            Self::Ollama => "llama3.2",
            Self::OpenRouter => "anthropic/claude-3.5-sonnet",
            Self::Perplexity => "llama-3.1-sonar-large-128k-online",
            Self::Together => "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
            Self::XAI => "grok-2-latest",
        }
    }

    /// Get environment variable name for API key.
    pub fn env_var(&self) -> &'static str {
        match self {
            Self::Anthropic => "ANTHROPIC_API_KEY",
            Self::OpenAI => "OPENAI_API_KEY",
            Self::Azure => "AZURE_OPENAI_API_KEY",
            Self::Gemini => "GEMINI_API_KEY",
            Self::Cohere => "COHERE_API_KEY",
            Self::DeepSeek => "DEEPSEEK_API_KEY",
            Self::Groq => "GROQ_API_KEY",
            Self::Mistral => "MISTRAL_API_KEY",
            Self::Ollama => "OLLAMA_API_KEY", // Optional for local
            Self::OpenRouter => "OPENROUTER_API_KEY",
            Self::Perplexity => "PERPLEXITY_API_KEY",
            Self::Together => "TOGETHER_API_KEY",
            Self::XAI => "XAI_API_KEY",
        }
    }

    /// List all providers with their info.
    pub fn all() -> Vec<(Self, &'static str, &'static str, &'static str)> {
        vec![
            (
                Self::Anthropic,
                "anthropic",
                "claude-sonnet-4-20250514",
                "ANTHROPIC_API_KEY",
            ),
            (Self::OpenAI, "openai", "gpt-4o", "OPENAI_API_KEY"),
            (Self::Azure, "azure", "gpt-4o", "AZURE_OPENAI_API_KEY"),
            (Self::Gemini, "gemini", "gemini-2.0-flash", "GEMINI_API_KEY"),
            (Self::Cohere, "cohere", "command-r-plus", "COHERE_API_KEY"),
            (
                Self::DeepSeek,
                "deepseek",
                "deepseek-chat",
                "DEEPSEEK_API_KEY",
            ),
            (
                Self::Groq,
                "groq",
                "llama-3.3-70b-versatile",
                "GROQ_API_KEY",
            ),
            (
                Self::Mistral,
                "mistral",
                "mistral-large-latest",
                "MISTRAL_API_KEY",
            ),
            (Self::Ollama, "ollama", "llama3.2", "OLLAMA_API_KEY"),
            (
                Self::OpenRouter,
                "openrouter",
                "anthropic/claude-3.5-sonnet",
                "OPENROUTER_API_KEY",
            ),
            (
                Self::Perplexity,
                "perplexity",
                "llama-3.1-sonar-large-128k-online",
                "PERPLEXITY_API_KEY",
            ),
            (
                Self::Together,
                "together",
                "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
                "TOGETHER_API_KEY",
            ),
            (Self::XAI, "xai", "grok-2-latest", "XAI_API_KEY"),
        ]
    }
}

#[cfg(feature = "llm")]
pub struct RigLlm {
    provider: Provider,
    model: String,
}

#[cfg(feature = "llm")]
impl RigLlm {
    pub fn new(provider_str: &str, model: Option<&str>) -> Result<Self, String> {
        let provider = Provider::from_str(provider_str)
            .ok_or_else(|| format!("Unsupported provider: {}. Available: anthropic, openai, azure, gemini, cohere, deepseek, groq, mistral, ollama, openrouter, perplexity, together, xai", provider_str))?;

        // Check for API key (ollama is optional since it can be local)
        if provider != Provider::Ollama && std::env::var(provider.env_var()).is_err() {
            return Err(format!(
                "Missing {} environment variable for {} provider",
                provider.env_var(),
                provider_str
            ));
        }

        let model = model
            .map(|m| m.to_string())
            .unwrap_or_else(|| provider.default_model().to_string());

        Ok(Self { provider, model })
    }

    async fn complete_async(&self, system: Option<&str>, prompt: &str) -> Result<String, String> {
        macro_rules! run_provider {
            ($client:expr) => {{
                let client = $client;
                let mut builder = client.agent(&self.model);
                if let Some(sys) = system {
                    builder = builder.preamble(sys);
                }
                let agent = builder.build();
                agent
                    .prompt(prompt)
                    .await
                    .map_err(|e| format!("LLM request failed: {}", e))
            }};
        }

        match self.provider {
            Provider::Anthropic => run_provider!(providers::anthropic::Client::from_env()),
            Provider::OpenAI => run_provider!(providers::openai::Client::from_env()),
            Provider::Azure => run_provider!(providers::azure::Client::from_env()),
            Provider::Gemini => run_provider!(providers::gemini::Client::from_env()),
            Provider::Cohere => run_provider!(providers::cohere::Client::from_env()),
            Provider::DeepSeek => run_provider!(providers::deepseek::Client::from_env()),
            Provider::Groq => run_provider!(providers::groq::Client::from_env()),
            Provider::Mistral => run_provider!(providers::mistral::Client::from_env()),
            Provider::Ollama => run_provider!(providers::ollama::Client::from_env()),
            Provider::OpenRouter => run_provider!(providers::openrouter::Client::from_env()),
            Provider::Perplexity => run_provider!(providers::perplexity::Client::from_env()),
            Provider::Together => run_provider!(providers::together::Client::from_env()),
            Provider::XAI => run_provider!(providers::xai::Client::from_env()),
        }
    }
}

#[cfg(feature = "llm")]
impl LlmStrategy for RigLlm {
    fn complete(&self, prompt: &str) -> Result<String, String> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create runtime: {}", e))?;
        rt.block_on(self.complete_async(None, prompt))
    }

    fn complete_with_system(&self, system: &str, prompt: &str) -> Result<String, String> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| format!("Failed to create runtime: {}", e))?;
        rt.block_on(self.complete_async(Some(system), prompt))
    }
}

/// Build an LLM strategy from workflow config.
pub fn build_llm_strategy(_provider: Option<&str>, _model: Option<&str>) -> Box<dyn LlmStrategy> {
    #[cfg(feature = "llm")]
    {
        if let Some(provider) = _provider {
            match RigLlm::new(provider, _model) {
                Ok(llm) => return Box::new(llm),
                Err(e) => {
                    eprintln!("Warning: Failed to initialize LLM: {}", e);
                }
            }
        }
    }

    Box::new(NoLlm)
}

/// List available providers.
#[cfg(feature = "llm")]
pub fn list_providers() -> Vec<(&'static str, &'static str, &'static str)> {
    Provider::all()
        .into_iter()
        .map(|(_, name, model, env)| (name, model, env))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_llm() {
        let llm = NoLlm;
        assert!(llm.complete("test").is_err());
    }

    #[test]
    fn test_build_llm_strategy_without_provider() {
        let strategy = build_llm_strategy(None, None);
        assert!(strategy.complete("test").is_err());
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_provider_parsing() {
        assert_eq!(Provider::from_str("anthropic"), Some(Provider::Anthropic));
        assert_eq!(Provider::from_str("claude"), Some(Provider::Anthropic));
        assert_eq!(Provider::from_str("openai"), Some(Provider::OpenAI));
        assert_eq!(Provider::from_str("gpt"), Some(Provider::OpenAI));
        assert_eq!(Provider::from_str("google"), Some(Provider::Gemini));
        assert_eq!(Provider::from_str("gemini"), Some(Provider::Gemini));
        assert_eq!(Provider::from_str("groq"), Some(Provider::Groq));
        assert_eq!(Provider::from_str("ollama"), Some(Provider::Ollama));
        assert_eq!(Provider::from_str("unknown"), None);
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_all_providers_have_defaults() {
        for (provider, _, _, _) in Provider::all() {
            assert!(!provider.default_model().is_empty());
            assert!(!provider.env_var().is_empty());
        }
    }
}
