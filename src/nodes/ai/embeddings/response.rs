use anyhow::Result;
use reqwest::StatusCode;
use serde::Deserialize;

#[derive(Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f64>,
}

#[derive(Deserialize)]
struct OpenAiErrorResponse {
    error: OpenAiErrorDetail,
}

#[derive(Deserialize)]
struct OpenAiErrorDetail {
    message: String,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f64>>,
}

#[derive(Deserialize)]
struct OllamaErrorResponse {
    error: String,
}

pub(super) fn parse_openai_response(status: StatusCode, body: &str) -> Result<Vec<Vec<f64>>> {
    if !status.is_success() {
        if let Ok(error) = serde_json::from_str::<OpenAiErrorResponse>(body) {
            anyhow::bail!("OpenAI API error ({}): {}", status, error.error.message);
        }
        anyhow::bail!("OpenAI API error ({}): {}", status, body);
    }

    let response: OpenAiEmbeddingResponse = serde_json::from_str(body)
        .map_err(|error| anyhow::anyhow!("Failed to parse OpenAI response: {}", error))?;
    Ok(response
        .data
        .into_iter()
        .map(|item| item.embedding)
        .collect())
}

pub(super) fn parse_ollama_response(status: StatusCode, body: &str) -> Result<Vec<Vec<f64>>> {
    if !status.is_success() {
        if let Ok(error) = serde_json::from_str::<OllamaErrorResponse>(body) {
            anyhow::bail!("Ollama error ({}): {}", status, error.error);
        }
        anyhow::bail!("Ollama error ({}): {}", status, body);
    }

    let response: OllamaEmbedResponse = serde_json::from_str(body)
        .map_err(|error| anyhow::anyhow!("Failed to parse Ollama response: {}", error))?;
    Ok(response.embeddings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_openai_embeddings_in_response_order() {
        let body = r#"{"data":[{"embedding":[1.0,2.0]},{"embedding":[3.0,4.0]}]}"#;
        assert_eq!(
            parse_openai_response(StatusCode::OK, body).unwrap(),
            vec![vec![1.0, 2.0], vec![3.0, 4.0]]
        );
    }

    #[test]
    fn surfaces_structured_ollama_errors() {
        let error = parse_ollama_response(
            StatusCode::BAD_REQUEST,
            r#"{"error":"unknown embedding model"}"#,
        )
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Ollama error (400 Bad Request): unknown embedding model"
        );
    }
}
