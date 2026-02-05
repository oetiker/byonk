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

    #[error("Not found")]
    NotFound,

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

    #[error("Dither error: {0}")]
    Dither(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            ApiError::MissingHeader(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            ApiError::DeviceNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_error_missing_header() {
        let error = ApiError::MissingHeader("Authorization");
        assert_eq!(error.to_string(), "Missing required header: Authorization");
    }

    #[test]
    fn test_api_error_device_not_found() {
        let error = ApiError::DeviceNotFound;
        assert_eq!(error.to_string(), "Device not found");
    }

    #[test]
    fn test_api_error_not_found() {
        let error = ApiError::NotFound;
        assert_eq!(error.to_string(), "Not found");
    }

    #[test]
    fn test_api_error_content() {
        let error = ApiError::Content("Script failed".to_string());
        assert_eq!(error.to_string(), "Content error: Script failed");
    }

    #[test]
    fn test_api_error_internal() {
        let error = ApiError::Internal("Database error".to_string());
        assert_eq!(error.to_string(), "Internal error: Database error");
    }

    #[test]
    fn test_render_error_svg_parse() {
        let error = RenderError::SvgParse("Invalid XML".to_string());
        assert_eq!(error.to_string(), "SVG parse error: Invalid XML");
    }

    #[test]
    fn test_render_error_unsupported_dimensions() {
        let error = RenderError::UnsupportedDimensions {
            width: 9999,
            height: 9999,
        };
        assert_eq!(error.to_string(), "Unsupported dimensions: 9999x9999");
    }

    #[test]
    fn test_render_error_image_too_large() {
        let error = RenderError::ImageTooLarge {
            size: 100_000,
            max: 90_000,
        };
        assert_eq!(
            error.to_string(),
            "Image too large: 100000 bytes (max 90000)"
        );
    }

    #[test]
    fn test_render_error_pixmap_allocation() {
        let error = RenderError::PixmapAllocation;
        assert_eq!(error.to_string(), "Failed to allocate pixmap");
    }

    #[test]
    fn test_render_error_png_encode() {
        let error = RenderError::PngEncode("Encoding failed".to_string());
        assert_eq!(error.to_string(), "PNG encode error: Encoding failed");
    }

    #[test]
    fn test_api_error_from_render_error() {
        let render_error = RenderError::PixmapAllocation;
        let api_error: ApiError = render_error.into();
        match api_error {
            ApiError::Render(_) => {}
            _ => panic!("Expected Render variant"),
        }
    }

    #[test]
    fn test_api_error_into_response_status_codes() {
        use axum::response::IntoResponse;

        // MissingHeader -> BAD_REQUEST
        let response = ApiError::MissingHeader("ID").into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        // DeviceNotFound -> NOT_FOUND
        let response = ApiError::DeviceNotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // NotFound -> NOT_FOUND
        let response = ApiError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        // Content -> INTERNAL_SERVER_ERROR
        let response = ApiError::Content("error".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        // Internal -> INTERNAL_SERVER_ERROR
        let response = ApiError::Internal("error".to_string()).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        // Render -> INTERNAL_SERVER_ERROR
        let response = ApiError::Render(RenderError::PixmapAllocation).into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
