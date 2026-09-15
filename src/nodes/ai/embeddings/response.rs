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
    index: Option<usize>,
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

    let mut response: OpenAiEmbeddingResponse = serde_json::from_str(body).map_err(|error| {
        anyhow::anyhow!(
            "Invalid OpenAI embedding response at line {} column {}",
            error.line(),
            error.column()
        )
    })?;
    if response.data.iter().any(|item| item.index.is_some()) {
        response.data.sort_unstable_by_key(|item| item.index);
        anyhow::ensure!(
            response
                .data
                .iter()
                .enumerate()
                .all(|(index, item)| item.index == Some(index)),
            "OpenAI embedding indices must contain every input index exactly once"
        );
    }
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

    let response: OllamaEmbedResponse = serde_json::from_str(body).map_err(|error| {
        anyhow::anyhow!(
            "Invalid Ollama embedding response at line {} column {}",
            error.line(),
            error.column()
        )
    })?;
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

    #[test]
    fn rejects_duplicate_missing_mixed_and_out_of_range_indices() {
        for indices in [
            serde_json::json!([0, 0]),
            serde_json::json!([0, 2]),
            serde_json::json!([0, null]),
            serde_json::json!([-1, 1]),
        ] {
            let body = serde_json::json!({"data": indices.as_array().unwrap().iter()
                .map(|index| serde_json::json!({"index": index, "embedding": [1.0]})).collect::<Vec<_>>()});
            assert!(parse_openai_response(StatusCode::OK, &body.to_string()).is_err());
        }
    }
}
