use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("Missing required header: {0}")]
    MissingHeader(&'static str),

    #[error("Device not found")]
    DeviceNotFound,

    #[error("Invalid or expired URL signature")]
    InvalidSignature,

    #[error("Rendering error: {0}")]
    Render(#[from] RenderError),

    #[error("Content error: {0}")]
    Content(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl From<crate::services::content_pipeline::ContentError> for ApiError {
    fn from(e: crate::services::content_pipeline::ContentError) -> Self {
        ApiError::Content(e.to_string())
    }
}

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("SVG parse error: {0}")]
    SvgParse(String),

    #[error("Unsupported dimensions: {width}x{height}")]
    UnsupportedDimensions { width: u32, height: u32 },

    #[error("Image too large: {size} bytes (max {max})")]
    ImageTooLarge { size: usize, max: usize },

    #[error("Failed to allocate pixmap")]
    PixmapAllocation,

    #[error("PNG encode error: {0}")]
    PngEncode(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::MissingHeader(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::DeviceNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::InvalidSignature => (StatusCode::FORBIDDEN, self.to_string()),
            ApiError::Render(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::Content(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = Json(json!({
            "status": status.as_u16(),
            "error": message,
        }));

        (status, body).into_response()
    }
}
