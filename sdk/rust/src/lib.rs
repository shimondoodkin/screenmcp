mod client;
mod error;
mod types;

pub use client::{DeviceConnection, ScreenMCPClient};
pub use error::{Result, ScreenMCPError};
pub use types::{
    CameraInfo, CameraResult, ClipboardResult, ClientOptions, CommandResponse, CopyResult,
    DeviceInfo, ListCamerasResult, ScreenshotResult, ScrollDirection, TextResult, UiTreeResult,
};
