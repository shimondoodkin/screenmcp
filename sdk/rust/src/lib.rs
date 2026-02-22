mod client;
mod error;
mod types;

pub use client::ScreenMCPClient;
pub use error::{Result, ScreenMCPError};
pub use types::{
    CameraInfo, CameraResult, ClipboardResult, ClientOptions, CommandResponse, CopyResult,
    ListCamerasResult, ScreenshotResult, ScrollDirection, TextResult, UiTreeResult,
};
